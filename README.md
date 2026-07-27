# Cheat Engine Auto Assembler for Zed

Syntax highlighting for Cheat Engine Auto Assembler (`.cea`) scripts, including
embedded Lua sections.

## Features

- Cheat Engine directives such as `{$STRICT}`, `{$lua}`, and `{$asm}`
- `[ENABLE]` and `[DISABLE]` sections
- Auto Assembler commands, labels, x86/x64 operations, registers, numbers,
  casts, and comments
- Lua syntax highlighting inside `{$lua}` regions
- Error-tolerant parsing while scripts are incomplete

The extension intentionally does not attach an assembly or Lua language server.
See [TODO.md](TODO.md) for the Lua LSP constraints and possible future designs.

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

The shell provides Node.js, Tree-sitter CLI 0.26, and the native compiler
toolchain used to build the parser. Run `npm run generate` after changing
`grammar.js`.

On non-Nix systems, `npm install` provides the same CLI through the existing
development dependency.

The corpus includes sanitized AA, Lua, mixed-mode, and malformed-input cases.
