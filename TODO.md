# CEA language tooling roadmap

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

## Language server

Zed's Tree-sitter injections parse and highlight Lua regions, but Zed does not
currently forward completions, diagnostics, hover, or navigation requests for
injected ranges to a Lua language server. The generic implementation proposed
in [zed#46870](https://github.com/zed-industries/zed/pull/46870) was closed
unmerged.

Instead of maintaining user-visible shadow files, build a CEA language server
that owns the mixed document and proxies Lua requests to
`lua-language-server`.

For each open CEA document, the server should construct a virtual Lua document
by replacing non-Lua characters with spaces while preserving the original line
endings. Lua content remains unchanged. This keeps line, column, and UTF-16
coordinates aligned across the CEA and virtual documents, including documents
with multiple Lua regions. Virtual documents should use internal `.lua` URIs
and must never appear in project search or version control.

### Initial milestone

- Identify Lua regions using the CEA grammar.
- Start or connect to `lua-language-server`.
- Forward diagnostics, completion, hover, and signature-help requests within
  Lua regions.
- Translate virtual document URIs back to their CEA documents.
- Ignore Lua requests and diagnostics outside injected Lua ranges.
- Test LF and CRLF documents, non-ASCII text, malformed mode transitions, and
  multiple Lua regions.

Incremental synchronization, rename, code actions, semantic tokens, automatic
LuaLS installation, and cross-file behavior can follow after the proxy design
has been validated.

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
- Have the Zed extension locate and launch the standalone CEA language server.

Separate repositories may be considered if the components develop independent
release cycles or external consumers make that useful. Repository separation is
not required for the initial implementation.
