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

1. Open Zed's command palette.
2. Run `zed: install dev extension`.
3. Select this repository.
4. Open a `.cea` file.

For unpublished grammar work, temporarily replace the grammar repository in
`extension.toml` with an absolute `file://` URL to this repository. Do not
commit that machine-specific URL.

## Grammar development

The grammar source is under `grammars/cea`. With Tree-sitter CLI 0.26:

```sh
npm install
npm run generate
npm test
```

On NixOS, run the CLI in a shell containing native build tools:

```sh
nix shell nixpkgs#tree-sitter nixpkgs#gcc
cd grammars/cea
tree-sitter test
```

The corpus includes sanitized AA, Lua, mixed-mode, and malformed-input cases.
