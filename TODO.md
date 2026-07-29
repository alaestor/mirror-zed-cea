# Post-LLM-Review: CEA language tooling roadmap

This monorepo contains three closely related deliverables:

- `grammar/`: the reusable Tree-sitter CEA grammar;
- `server/`: the standalone, editor-agnostic CEA language server; and
- `languages/cea/`, `extension.toml`, and `src/`: the Zed integration.

Keep them together while they share a release cycle. The grammar and language
server should remain usable without Zed, and the server must continue to speak
standard LSP over stdio without Zed-specific behavior.

## Current baseline

The initial architecture is implemented:

- error-tolerant CEA parsing, highlighting, and structural diagnostics;
- document symbols and an open-document CEA symbol index;
- native CEA completion, navigation, references, highlights, rename, and
  cross-language links from direct Lua address API calls;
- position-preserving virtual Lua documents;
- a supervised, restartable private LuaLS process;
- embedded Lua diagnostics, completion, hover, signature help, navigation,
  references, rename, code actions, and inlay hints;
- project Lua runtime paths, workspace libraries, inherited `LUA_PATH`, and
  multiple workspace folders;
- standalone Nix packages for the server and grammar; and
- unit, Tree-sitter corpus, and stdio integration coverage.

The roadmap below contains remaining work rather than a history of completed
milestones.

## Priority 0: restore a trustworthy LuaLS integration baseline

The stdio test
`resolves_definitions_across_workspace_and_lua_path_fixtures` currently returns
a null first definition with LuaLS `3.18.2-dev`, while the other integration
tests pass.

- [x] Determine whether the failure is a LuaLS workspace-index readiness race,
  a configuration change, or a module-resolution regression.
- [x] Make the test wait for an observable ready condition or use a bounded retry
  only around asynchronous indexing; do not add an unconditional sleep.
- [x] Verify direct `?.lua`, nested `?/init.lua`, external `LUA_PATH`, `.d.lua`,
  and cross-CEA definitions independently so a failure identifies the affected
  path.
- [x] Keep the test deterministic across the LuaLS version supplied by the
  pinned `nixpkgs`.

## Priority 1: bundle a versioned Cheat Engine Lua API library

Ship curated LuaLS declarations for the CE Lua environment. Start with the
provided CE 7.7-oriented reference material, but keep only declarations and
constants. Do not ship the stateful mock implementation; it's taken from
another repo as reference. The canonical celua.txt from 7.7 has been included
for cross-reference, but it's quite large so it should be used sparingly.

### Declaration data

- [x] Create a versioned declaration tree, for example:

  ```text
  cheat-engine-api/
    7.7/
      manifest.json
      core.d.lua
      memory.d.lua
      address-list.d.lua
      mono.d.lua
      ui.d.lua
  ```

- [x] Mark every library file with `---@meta` and keep it free of runtime mocks
  and side effects.
- [x] Remove ALCE-specific wording and resolve known cleanup items in the
  references, including the duplicate `disableWithoutExecute` member, the
  ambiguous `Type` alias, typos, and `METHOD_ATTRIBUTE_*` annotations currently
  typed as `FieldAttribute`.
- [x] Record the snapshot's CE version, provenance, coverage, and known
  uncertainties in its manifest. Describe 7.7 as a curated subset until its
  coverage warrants a stronger claim.
- [x] Prefer complete snapshots per CE version initially. Introduce shared base
  files or generated overlays only if multiple versions create meaningful
  duplication.

Note: these recommendations originally included the following:

> - [ ] Convert primitive conceptual types such as addresses and symbol names to
  documented aliases where LuaLS models aliases more accurately than classes.

However, I'm hesitant. @class types, as demonstrated in the existing files,
seems to provide more clear semantic meaning to the annotations and 
helps detect simple errors early. Aliases are a bit too loose in terms of emitting
diagnostics, however I might be mistaken. It's worth some experimentation to see
if aliases are sufficient type information.

### Configuration

Add a CE API setting alongside the existing `luaLanguageServer` setting:

```json
{
  "lsp": {
    "cea-language-server": {
      "initialization_options": {
        "cheatEngineApi": {
          "enabled": true,
          "version": "7.7"
        },
        "luaLanguageServer": {
          "path": "lua-language-server",
          "runtimeVersion": "LuaJIT",
          "runtimePath": ["scripts/?.lua", "scripts/?/init.lua"],
          "workspaceLibrary": ["types"]
        }
      }
    }
  }
}
```

- [x] Enable the only bundled version, initially 7.7, by default.
- [x] Support `enabled: false` for projects with their own complete declarations
  or users who want an unmodified LuaLS environment.
- [x] Treat CE API version and Lua `runtimeVersion` as independent settings.
- [x] Select exact supported version strings and surface a clear diagnostic or
  initialization warning for unsupported versions instead of silently falling
  back.
- [x] Continue merging user `workspaceLibrary` entries so project-specific CE
  extensions can coexist with the bundled declarations.
- [x] Document that configuration changes take effect after restarting the
  language server; live CE-version switching is not required initially.

### Packaging and LuaLS integration

LuaLS requires declaration files on disk. The language server is currently
distributed as a standalone binary, so do not rely on every installation method
preserving adjacent resource files.

- [x] Embed the declaration snapshots in the server binary.
- [x] Materialize the selected snapshot atomically into a deterministic cache
  path keyed by CE version and content hash.
- [x] Reuse matching cached content, avoid writes inside the user's workspace,
  and add the extracted directory to `Lua.workspace.library`.
- [x] Preserve stable declaration URIs so go-to-definition opens useful files.
- [x] Ensure LuaLS restart/resynchronization reuses the selected library.

### Tests

- [x] Validate manifests, supported versions, and declaration-only file content.
- [x] Unit-test default selection, disabled mode, unsupported versions, cache
  extraction, and merging with explicit and inherited Lua paths/libraries.
- [x] Add LuaLS integration coverage for representative CE global completion,
  hover, signature help, and go-to-definition.
- [x] Verify representative CE calls no longer produce undefined-global
  diagnostics.
- [x] Verify disabling the bundled API restores ordinary LuaLS behavior.

## Priority 2: make native CEA intelligence workspace-complete

The current `WorkspaceSymbolIndex` contains open documents only. Expand it to
unopened `.cea` files so "workspace-aware" navigation has its conventional LSP
meaning.

- [x] Discover `.cea` files under every workspace folder at initialization and
  when folders are added.
- [x] Define sensible exclusions and limits so dependency, build, and very large
  directory trees are not scanned without bound.
- [x] Register or consume watched-file changes for create, change, and delete.
- [x] Keep open-buffer contents authoritative over disk snapshots.
- [x] On close, fall back to the current on-disk file if it remains in a
  workspace instead of dropping it from the index.
- [x] Cover definitions, references, rename, completion, duplicate diagnostics,
  and file deletion across open and unopened files.
- [x] Until this is implemented, describe the native index accurately as
  spanning open CEA documents.

## Priority 3: focused maintainability and UX

- [x] When feature work next touches `lua.rs`, separate configuration,
  process/protocol supervision, and URI/diagnostic translation into modules.
- [x] Likewise, move native CEA feature handlers out of `backend.rs` when doing
  so reduces the scope of an active change.
- [x] Improve the user-visible status when LuaLS is unavailable while preserving
  native CEA features.
- [x] Review native completion context so CEA symbols are offered where useful
  without overwhelming unrelated Lua or comment contexts.
- [x] Keep the README feature list, configuration schema, and actual capability
  coverage synchronized.

## Priority 4: CEA-specific language features

- [x] .cea files must have both enable and disable sections to be valid.
- [x] `{$STRICT}` requires that labels be declared before usage in assembly.

... more. This section should be expanded. Refer to:
https://wiki.cheatengine.org/index.php?title=Cheat_Engine:Auto_Assembler
mainly sections:
- Assigning a Script to a CheatTable
- Value Notation
- General Information

And for the list of commands: https://wiki.cheatengine.org/index.php?title=Auto_Assembler:Commands

Documentation is poor... I'll do what I can.


## Deferred until evidence justifies them

- Incremental Tree-sitter edits and incremental semantic-diagnostic updates.
  Full synchronization is simpler and appropriate for typical CEA document
  sizes until profiling shows otherwise.
- Lua semantic-token forwarding. It requires correct legend negotiation or
  translation; Tree-sitter already highlights injected Lua.
- Synchronizing unsaved standalone `.lua` buffers between Zed's LuaLS and the
  managed LuaLS process.
- Assembly language-server integration.
- Repository splitting.

These are not release blockers for the current product.

## Validation commands

Run the supported checks through the Nix development shell on NixOS:

```sh
nix develop -c cargo test --manifest-path server/Cargo.toml
nix develop -c npm test
nix develop -c cargo check
```

After changing `grammar/grammar.js`, run `npm run generate` in the development
shell and commit the regenerated parser artifacts with the grammar change.
