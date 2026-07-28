# Cheat Engine Auto Assembler for Zed

Zed language support for Cheat Engine Auto Assembler (`.cea`) scripts with embedded Lua.

## Features

- Error-tolerant Tree-sitter parsing and highlighting for Auto Assembler, x86/x64 operations, sections, directives, and embedded Lua
- Document symbols and workspace-aware completion, definitions, references, highlights, and rename for CEA symbols
- Diagnostics for malformed structure, invalid command arguments and section usage, duplicate declarations, and unresolved explicit symbol references
- Embedded Lua diagnostics, completion, hover, signature help, navigation, rename, code actions, and inlay hints through a managed `lua-language-server`
- Cross-language navigation from direct Lua calls such as `getAddress("playerHealth")`

Semantic tokens and assembly language-server integration are not supported.

> [!NOTE]
>
> **Known Limitation:**
> 
> Because we use virtual files to manage the Lua language server: saved standalone Lua changes are visible to the managed LuaLS through the filesystem, but unsaved standalone Lua changes are isolated in Zed's separate LuaLS process until saved. Though, this is rarely experienced as a problem thanks to Zed's default low-delay for auto-saving.

## Requirements

`cea-language-server` must be in Zed's `PATH`.

`lua-language-server` must either be in `PATH` or have its path configured below. The Nix development shell provides both.

Development installation also requires a rustup-managed stable toolchain for Zed's `wasm32-wasip2` target.

## Lua configuration

Configure the managed LuaLS with Zed's LSP initialization options:

```json
{
  "lsp": {
    "cea-language-server": {
      "initialization_options": {
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

Paths may be absolute or relative to the first workspace folder. Explicit runtime paths and libraries are combined with inherited `LUA_PATH` entries. Relative `LUA_PATH` entries resolve from the workspace, and `;;` expands to Lua's default `?.lua` and `?/init.lua` layouts.

`CEA_LUA_LANGUAGE_SERVER` overrides the configured executable path.

## Build and test

```sh
nix build .#cea-language-server
nix run .#cea-language-server
nix develop -c cargo test --manifest-path server/Cargo.toml
nix develop -c npm test
```

The flake also exposes `tree-sitter-cea`.

Run `npm run generate` after changing `grammar/grammar.js`.

To prepare release metadata and changelog: from a clean tree, run `nix run .#bump -- 0.3.0`. Review and commit the result before tagging.

## Install as a development extension

The grammar commit in `extension.toml` must exist on the remote before Zed can install the extension.

1. On NixOS, the language server packages must be built first and exposed in Path. You can install the stable toolchain once with `nix develop -c rustup toolchain install stable --profile minimal`, then launch Zed with `nix develop -c zeditor .`. Elsewhere, open Zed normally.

2. Run `zed: install dev extension` and select this repository.

3. Open a `.cea` file.

Zed uses `grammars/cea` as its private grammar checkout; maintained grammar sources live under `grammar/`.

If Zed's downloaded WASI compiler cannot run on NixOS, let the first install attempt create the grammar checkout, then run:

```sh
nix develop
npm run build:zed-grammar
```

Retry the installation so Zed uses the generated `grammars/cea.wasm`.

I may look into how to improve the Nix install flow in the future, but for now this is sufficient for my personal use.
