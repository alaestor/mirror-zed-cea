# Cheat Engine Auto Assembler for Zed

Zed language support for Cheat Engine Auto Assembler (`.cea`) scripts with embedded Lua.

## Features

* Tree-sitter parsing and highlighting for Auto Assembler, x86/x64, directives, sections, and embedded Lua
* Workspace-wide symbols, completion, navigation, references, highlights, and rename across `.cea` files
* Diagnostics for malformed structure, invalid sections or arguments, duplicate declarations, label errors, and unresolved symbols
* Integer hovers with hexadecimal, decimal, and signed representations
* Full embedded Lua language support via a managed Lua LSP
* Bundled, versioned Cheat Engine Lua API declarations (v7.7+)
* Cross-language navigation from Lua calls

## Known limitations

It's possible that modern features may be missing e.g. `SHAREDALLOC` implementation was commented out in the [last available source](https://github.com/cheat-engine/cheat-engine/tree/a3e1a24b8cf6b1bafc5aecce676cca5131281ade).

Because we use virtual files to manage the Lua language server: saved standalone Lua changes are visible to the managed LuaLS through the filesystem, but unsaved standalone Lua changes are isolated in Zed's separate LuaLS process until saved. Though, this is rarely experienced as a problem thanks to Zed's default low-delay for auto-saving.

The following areas are not supported:

- `LUACALL` argument handling and functions added at runtime by plugins: out-of-scope
- Semantic tokens: complicated by the meta-language nature of AA and the current architecture of this project
- assembly LSP integration: low value, and AA/Lua symbol resolution would complicate integrating an existing LSP
- `STRUCT`/`ENDSTRUCT` blocks: zero priority for me; requires additional grammar
- `{$CCODE}` blocks: zero priority for me; if I was going to bother writing C code I'd just inject it myself
- C#: very poor documentation and low value; lua->mono is more useful

## Requirements

`cea-language-server` must be in `PATH`.

`lua-language-server` must either be in `PATH` or have its path configured below.

The Nix development shell provides both: `nix develop -c zeditor`

Development installation also requires a rustup-managed stable toolchain for Zed's `wasm32-wasip2` target.

## Lua configuration

Configure the managed LuaLS with Zed's LSP initialization options:

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

Paths may be absolute or relative to the first workspace folder. Explicit runtime paths and libraries are combined with inherited `LUA_PATH` entries. Relative `LUA_PATH` entries resolve from the workspace, and `;;` expands to Lua's default `?.lua` and `?/init.lua` layouts.

The bundled Cheat Engine API snapshot is enabled by default and can be disabled by setting `cheatEngineApi.enabled` to `false`. Only the exact versions in `cheat-engine-api` are currently supported; unsupported versions fail initialization. The CE API version and Lua `runtimeVersion` are independent, and user workspace libraries are merged with the bundled declarations. Restart the CEA language server after changing these settings.

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

> [!NOTE]
> **Release Chores:**
> 
> To prepare release metadata and changelog: from a clean tree, run `nix run .#bump -- 0.3.0`. Review and commit the result before tagging.

## Install as a development extension

The grammar commit in `extension.toml` must exist on the remote before Zed can install the extension.

Run `zed: install dev extension`, select this repository, and open a `.cea` file.

### NixOS workarounds

On NixOS, the language server packages must be built first and exposed in Path. You can install the stable toolchain once with `nix develop -c rustup toolchain install stable --profile minimal`, then launch Zed with `nix develop -c zeditor .`

If Zed's downloaded WASI compiler cannot run on NixOS, let the first install attempt create the grammar checkout, then run:

```sh
nix develop
npm run build:zed-grammar
```

Retry the installation so Zed uses the generated `grammars/cea.wasm`. Zed uses `grammars/cea` as its private grammar checkout; maintained grammar sources live under `grammar/`.

I may look into how to improve the Nix install flow in the future, but for now this is sufficient for my personal use.

## References

Cheat Engine has been in closed-source development since August 2024. Until then it had been source-available under a proprietary license; not FOSS or OSI/OSD, as it was frequently misunderstood to be.

Release-bundled files:

- `celua.txt` is the Lua API documentation published with CE releases. 
- `monoscript.lua` et al. The lua source files are shipped unobfuscated and may be used as a source-of-truth reference, but the documentation is usually good enough for a quick grep.

The following remote references may be outdated, but they're still the best references available. Important behaviour should be confirmed by manual testing in modern release.

- [autoassembler.pas](https://raw.githubusercontent.com/cheat-engine/cheat-engine/refs/heads/master/Cheat%20Engine/autoassembler.pas) (handles AA)

- [autoassemblercode.pas](https://raw.githubusercontent.com/cheat-engine/cheat-engine/refs/heads/master/Cheat%20Engine/autoassemblercode.pas) (handles `{$luacode}` and `{$ccode}`)
