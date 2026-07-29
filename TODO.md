# LLM-Generated Analysis: CEA language tooling

This repository contains three deliverables that currently share a release
cycle:

- `grammar/`: the reusable Tree-sitter CEA grammar;
- `server/`: the standalone, editor-agnostic CEA language server; and
- `languages/cea/`, `extension.toml`, and `src/`: the Zed integration.

The grammar and language server should remain usable without Zed. The server
must continue to speak standard LSP over stdio without editor-specific
behavior.

## Current baseline

The project currently provides:

- error-tolerant CEA parsing and Tree-sitter highlighting for Auto Assembler,
  x86/x64 instructions, directives, sections, and embedded Lua;
- structural and semantic diagnostics, including enable/disable section
  validation, command argument checks, duplicate declarations, unresolved
  explicit references, and `{$STRICT}` label ordering;
- workspace-wide CEA completion, definitions, references, highlights, rename,
  and document symbols across open and unopened `.cea` files;
- position-preserving virtual Lua documents backed by a supervised,
  restartable private LuaLS process;
- embedded Lua diagnostics, completion, hover, signature help, navigation,
  references, rename, code actions, and inlay hints;
- project Lua runtime paths, workspace libraries, inherited `LUA_PATH`, and
  multiple workspace folders;
- cross-language navigation from direct Lua address API calls to CEA symbols;
- bundled, versioned Cheat Engine 7.7 LuaLS declarations for common globals,
  memory, address-list, Mono, UI, and Auto Assembler APIs;
- standalone Nix packages for the server and grammar; and
- unit, Tree-sitter corpus, and stdio integration coverage.

Semantic tokens and assembly language-server integration are not implemented.
Unsaved standalone `.lua` buffers are not synchronized into the managed LuaLS
process.

## Active work

### Expand CEA-specific semantics

Use the Cheat Engine Auto Assembler documentation and real scripts to identify
high-value validation and navigation behavior. In particular:

- [x] Parse and highlight documented decimal (`#100`), integer (`(int)100`),
  float, double, and scientific value notation; diagnose typed notation without
  a value.
- [ ] Continue auditing Auto Assembler commands beyond the documented
  parenthesized core. Argument validation and scan-result navigation now cover
  AOB scans, allocation variants, memory/file commands, and thread commands;
  structure blocks and newer source-only commands remain to be investigated.
- [ ] Add corpus and language-server tests with every new rule.

Primary references:

- <https://wiki.cheatengine.org/index.php?title=Cheat_Engine:Auto_Assembler>
- <https://wiki.cheatengine.org/index.php?title=Auto_Assembler:Commands>

The documentation is incomplete; prefer observed Cheat Engine behavior and
small regression fixtures when sources disagree.

### Improve Cheat Engine API declarations

- [ ] Continue auditing the 7.7 declarations against `celua.txt` and practical
  scripts, adding useful types, overloads, constants, and documentation.
- [ ] Decide whether conceptual types such as `Address` and `SymbolName` are
  better represented as strict `---@class` types or aliases. Preserve the
  current classes unless an experiment shows aliases retain useful diagnostics.
- [ ] Add another CE version only when there is reference material and a real
  compatibility need; keep snapshots complete and independently selectable.

### Reliability and maintenance

- [ ] Remove the declaration snapshot cache-install race that can produce
  `Directory not empty` when multiple language-server integration tests start
  concurrently.
- [ ] Keep the README feature list, configuration schema, bundled declaration
  coverage, and actual capabilities synchronized.
- [ ] Revisit exclusions and workspace indexing limits as larger real-world
  projects expose performance problems.

## Deferred until evidence justifies them

- Incremental Tree-sitter edits and incremental semantic diagnostics. Full
  synchronization remains appropriate for typical CEA document sizes.
- Lua semantic-token forwarding. Tree-sitter already highlights injected Lua,
  and forwarding requires correct legend negotiation or translation.
- Synchronizing unsaved standalone `.lua` buffers between the editor's LuaLS
  and the managed LuaLS process.
- Assembly language-server integration.
- Splitting the repository or release cycle.
- Generated or shared declaration overlays across CE versions.

These are not release blockers for the current product.

## Validation

Run the supported checks through the Nix development shell:

```sh
nix develop -c cargo test --manifest-path server/Cargo.toml
nix develop -c npm test
nix develop -c cargo check
```

After changing `grammar/grammar.js`, run `npm run generate` in the development
shell and commit the regenerated parser artifacts with the grammar change.
