# Future Lua language-server support

Zed's Tree-sitter injections provide Lua parsing and highlighting, but they do
not currently forward completions, diagnostics, hover, or navigation requests
to the Lua language server.

The generic injection-aware LSP implementation proposed in
[zed#46870](https://github.com/zed-industries/zed/pull/46870) was closed
unmerged. Zed's documented extension API can register a server for a primary
language, but it does not expose virtual documents or request forwarding for
injected ranges.

Possible future approaches, in preferred order:

1. Adopt native injection-aware LSP support if Zed adds it.
2. Add an external companion tool that maintains shadow `.lua` files beside or
   under a cache directory and maps edits back to `.cea` ranges.
3. Build a CEA language server that owns the mixed document, proxies Lua
   requests to `lua-language-server`, and translates positions and diagnostics.

Any shadow-file or proxy design must preserve line/UTF-16 coordinates, exclude
`{$lua}`, `{$asm}`, `[ENABLE]`, and `[DISABLE]` markers, handle multiple Lua
regions, clean up stale files, and avoid exposing generated files to project
search or version control.

Assembly LSP integration is intentionally out of scope.
