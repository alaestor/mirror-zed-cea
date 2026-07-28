# LLM-Generated CEA language tooling roadmap

This repository will remain a monorepo for the Zed extension, Tree-sitter
grammar, and language server. Each component should nevertheless remain usable
on its own, without depending on Zed.

Planned components:

- `grammar/`: the Tree-sitter CEA grammar and its tests.
- `server/`: a standalone CEA language server.
- `languages/cea/` and `extension.toml`: the Zed integration that consumes the
  grammar and launches the language server.

The flake should expose components as separate package outputs so other editors
and tools can consume the grammar or language server independently. The
language server must speak standard LSP over stdio and contain no Zed-specific
behavior.

## Implemented foundation

- Standalone Rust language server under `server/`.
- Full-text document synchronization and Tree-sitter parsing.
- Parse diagnostics with UTF-16-compatible source ranges.
- Document symbols for sections, labels, allocations, and definitions.
- Unit and end-to-end stdio protocol tests.
- Separate `cea-language-server` and `tree-sitter-cea` flake package outputs.
- Zed extension launcher that resolves `cea-language-server` from `PATH`.
- Position-preserving virtual documents for embedded Lua regions.
- Managed LuaLS process with project workspace and `LUA_PATH` configuration.
- Embedded Lua diagnostics, completion, hover, signature help, definitions,
  and references with virtual URI translation.

## Language server

Zed's Tree-sitter injections parse and highlight Lua regions, but Zed does not
currently forward completions, diagnostics, hover, or navigation requests for
injected ranges to a Lua language server. The generic implementation proposed
in [zed#46870](https://github.com/zed-industries/zed/pull/46870) was closed
unmerged.

Build a CEA language server that owns the mixed document and proxies embedded
Lua requests to its own `lua-language-server` process. Zed's regular Lua
language server is a separate process and cannot be reused directly.

For each open CEA document, the server should construct a virtual Lua document
by replacing non-Lua characters with spaces while preserving the original line
endings. Lua content remains unchanged. This keeps line, column, and UTF-16
coordinates aligned across the CEA and virtual documents, including documents
with multiple Lua regions.

Give each virtual document its source `.cea` URI inside the private LuaLS
process so LuaLS resolves relative modules from the source directory and
recognizes the document as disk-backed during semantic diagnostic passes. The
in-memory text remains the position-preserving Lua projection and must never
overwrite the CEA file or create a user-visible shadow file. Leave URIs for real
`.lua` and `.d.lua` files unchanged so navigation opens the real files.

Launch LuaLS with the project directory as its workspace root so embedded Lua
can resolve symbols from standalone project files, `require` targets, and
`.d.lua` declaration files. Saved standalone Lua changes are visible to both
Zed's LuaLS and the CEA server's LuaLS through the filesystem. Unsaved changes
initially remain local to Zed's separate LuaLS process.

### Lua proxy milestones

#### 1. Session and virtual documents

- Identify Lua regions using the CEA grammar.
- Resolve and start `lua-language-server` from `PATH`.
- Initialize LuaLS with the project workspace and inherited shell environment.
- Construct position-preserving virtual Lua documents.
- Synchronize CEA open, full-text change, and close notifications with LuaLS.
- Shut down the child process cleanly and surface startup or protocol failures.

#### 2. Workspace and module resolution

- Parse `LUA_PATH`, including `?.lua` and `?/init.lua` patterns.
- Translate module patterns into LuaLS `Lua.runtime.path` configuration.
- Add external library roots to `Lua.workspace.library`.
- Index project `.lua` files and `.d.lua` declarations.
- Preserve the virtual document's source directory for relative module
  resolution.
- Test `require` and definition lookup across CEA, project Lua, declaration
  files, and external `LUA_PATH` libraries.

#### 3. Diagnostics

- Forward LuaLS diagnostics for virtual documents to their source CEA files.
- Ignore diagnostics outside embedded Lua regions.
- Clear stale diagnostics when regions change or documents close.
- Preserve real Lua URIs for related information and definition locations.

#### 4. Interactive language features

- Forward completion, completion resolution, hover, and signature help inside
  Lua regions.
- Forward definition, declaration, type-definition, implementation, and
  reference requests where URI and edit mapping are reliable.
- Reject Lua requests made outside embedded regions.

#### 5. Hardening and later capabilities

- Test LF and CRLF documents, non-ASCII text, malformed mode transitions,
  multiple Lua regions, and LuaLS restarts.
- Define CEA-specific configuration for LuaLS command, workspace libraries,
  runtime version, and path overrides.
- Evaluate synchronization of unsaved standalone `.lua` buffers without
  registering a second competing Lua server in Zed.
- Add incremental synchronization, rename, code actions, semantic tokens, and
  automatic LuaLS installation only after the proxy design is stable.

## Recommendations

The proxy foundation is functional. Prioritize correctness and resilience
before expanding the feature surface.

### 1. Harden the LuaLS proxy

- [x] Restart LuaLS after an unexpected exit and resynchronize open virtual
  documents.
- [x] Forward request cancellation and discard abandoned pending responses.
- [x] Handle LuaLS client requests deliberately, especially
  `workspace/configuration` and dynamic capability registration.
- [x] Translate CEA document URIs in diagnostic related information, nested
  response fields, workspace edits, and other URI-bearing payloads.
- [x] Support all workspace folders instead of selecting only the first.
- [x] Ensure virtual text is never written back to the source `.cea` file.
- [x] Improve request timeout, child-process, and protocol error reporting.

### 2. Complete Lua feature forwarding

- [x] Add completion-item resolution.
- [x] Forward declaration, type-definition, implementation, and document-highlight
  requests.
- [x] Add rename and code actions after workspace-edit URI and range translation is
  reliable.
- [x] Evaluate semantic tokens and inlay hints after the core proxy operations are
  stable.

Inlay hints are forwarded because their positions remain aligned in the virtual
document. Semantic tokens remain disabled: their numeric token types depend on the
legend negotiated with LuaLS, which currently starts after the CEA server has
advertised its own capabilities. Lua regions retain Tree-sitter highlighting.

### 3. Validate cross-file behavior

- [x] Add integration fixtures for CEA references into project `.lua` files,
  `.d.lua` declarations, `require` targets, and external `LUA_PATH` libraries.
- [x] Cover both `?.lua` and `?/init.lua` module layouts.
- [x] Resolve relative `LUA_PATH` roots against the workspace.
- [x] Implement Lua's empty `;;` default-path semantics.
- [x] Test multiple CEA files sharing Lua declarations.
- [x] Expand coverage for LF, CRLF, non-ASCII text, malformed mode transitions,
  and multiple embedded Lua regions.
- [x] Document that saved standalone Lua changes are visible through the
  filesystem while unsaved changes remain isolated in Zed's separate LuaLS
  process.

### 4. Add user configuration

- [ ] Expose settings for the LuaLS executable, runtime version, runtime paths, and
  workspace libraries.
- [ ] Merge explicit project configuration with inherited `LUA_PATH` values.
- [ ] Keep environment variables as convenient overrides while making project
  behavior reproducible without a particular shell setup.

Suggested configuration shape:

```json
{
  "cea": {
    "luaLanguageServer": {
      "path": "lua-language-server",
      "runtimeVersion": "LuaJIT",
      "runtimePath": [],
      "workspaceLibrary": []
    }
  }
}
```

### 5. Build native CEA intelligence

- [ ] Create a shared CEA symbol index for labels, allocations, definitions, and
  registered symbols.
- [ ] Use the index for completion, go-to-definition, references, and rename.
- [ ] Diagnose duplicate declarations, unresolved symbols, invalid section usage,
  malformed mode transitions, and invalid command arguments.
- [ ] Link CEA declarations to Lua APIs that refer to symbols by string, such as
  `getAddress("playerHealth")`.

The recommended next milestone is the native CEA symbol index. It unlocks
several editor features through one shared architecture and provides the basis
for reliable cross-language references.

## CEA-aware features

The language server should eventually provide native CEA intelligence rather
than acting only as a Lua proxy:

- Completion and go-to-definition for known CEA symbols.
- Diagnostics for duplicate declarations, unresolved symbols, and malformed
  section or mode transitions.
- References between CEA declarations and Lua APIs that name symbols, such as
  `getAddress("symbol")`, where this can be determined reliably.

Assembly language-server integration remains out of scope.

## Packaging

- Initially allow users to provide `lua-language-server` on `PATH`; automated
  discovery or installation can be added later.
- Keep the standalone CEA language server available on `PATH` when launching
  Zed.

Separate repositories may be considered if the components develop independent
release cycles or external consumers make that useful. Repository separation is
not required for the initial implementation.
