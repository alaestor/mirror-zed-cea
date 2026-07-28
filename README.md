# Cheat Engine Auto Assembler for Zed

Syntax highlighting and initial language-server support for Cheat Engine Auto
Assembler (`.cea`) scripts, including embedded Lua sections.

## Features

- Cheat Engine directives such as `{$STRICT}`, `{$lua}`, and `{$asm}`
- `[ENABLE]` and `[DISABLE]` sections
- Auto Assembler commands, labels, x86/x64 operations, registers, numbers,
  casts, and comments
- Lua syntax highlighting inside `{$lua}` regions
- Error-tolerant parsing while scripts are incomplete
- Document symbols for sections, labels, allocations, and definitions
- Parser diagnostics for malformed CEA structure

The standalone CEA language server is editor-independent. Lua language-server
proxying is not implemented yet, and assembly LSP integration is intentionally
out of scope. See [TODO.md](TODO.md) for the remaining roadmap.

## Language server

Build or run the language server through the flake:

```sh
nix build .#cea-language-server
nix run .#cea-language-server
```

For development:

```sh
nix develop -c cargo test --manifest-path server/Cargo.toml
nix develop -c cargo run --manifest-path server/Cargo.toml
```

The server communicates using standard LSP over stdin and stdout. It supports
full-text document synchronization, CEA and embedded Lua diagnostics, document
symbols, Lua completion, hover, signature help, definitions, references, rename,
code actions, and inlay hints.
Embedded Lua is proxied to `lua-language-server`, which shares the project
workspace and can resolve standalone `.lua` and `.d.lua` files. The Zed
extension resolves `cea-language-server` from the worktree shell's `PATH` and
launches it directly.

Both `cea-language-server` and `lua-language-server` must be available on
Zed's `PATH`. Set `CEA_LUA_LANGUAGE_SERVER` to override the LuaLS executable.
When `LUA_PATH` is present, its module patterns and library roots are forwarded
to LuaLS. Relative entries are resolved from the first workspace folder, and
Lua's `;;` marker expands to the default `?.lua` and `?/init.lua` layouts.

The root Cargo package is the small Zed WebAssembly extension. The native
language server remains an independent Cargo package under `server/`.

The development shell includes `cea-language-server` on `PATH`. Launching Zed
from that shell makes the server available to the extension:

```sh
nix develop -c rustup toolchain install stable --profile minimal
nix develop -c zeditor .
```

Zed uses `rustup` to install the `wasm32-wasip2` target when compiling the
extension, so a rustup-managed default toolchain must exist.

The flake also exposes the grammar independently as
`packages.<system>.tree-sitter-cea`.

## Local installation

After the grammar commit referenced by `extension.toml` has been pushed:

1. On NixOS, start Zed with `nix develop -c zeditor .` so extension builds can
   find the native compiler toolchain. On other systems, open Zed normally.
2. Run `zed: install dev extension`.
3. Select this repository.
4. Open a `.cea` file.

Zed uses `grammars/cea` as its private checkout directory while building the
extension, so the maintained grammar source lives in `grammar/`.

On NixOS, Zed's downloaded WASI compiler may not run because it is a generic
Linux binary. After the first installation attempt creates Zed's grammar
checkout, compile the WebAssembly grammar with the native Nix toolchain:

```sh
nix develop
npm run build:zed-grammar
```

Retry `zed: install dev extension`. Zed will use the newer
`grammars/cea.wasm` instead of invoking its downloaded compiler.

## Grammar development

The grammar source is under `grammar`. Enter the development shell and run the
tests:

```sh
nix develop
npm test
```

The shell provides Rust, Node.js, Tree-sitter CLI 0.26, and the native compiler
toolchain used to build the parser. Run `npm run generate` after changing
`grammar.js`.

On non-Nix systems, `npm install` provides the same CLI through the existing
development dependency.

The corpus includes sanitized AA, Lua, mixed-mode, and malformed-input cases.
