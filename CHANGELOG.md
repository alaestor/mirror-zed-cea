# Changelog

## [0.3.0] - 2026-07-30

### 🚀 Features

- *(server)* Complete priority language tooling
- *(cea)* Expand value and command semantics
- *(cea)* Add integer conversion hovers
- *(cea)* Diagnose incomplete label definitions
- *(api)* Declare structure dissection APIs
- *(lsp)* Aobscanex

### 🐛 Bug Fixes

- *(server)* Tolerate concurrent API cache installs

### 📚 Documentation

- Fix callout/admotion formatting
- *(todo)* Rewrite to reflect current state
- *(todo)* Rewrite to reflect current state
- Document CE references
- CE API explainer
- *(todo)* Record API and cache progress
- Document limitations, restructure, cleanup

### 🧪 Testing

- *(server)* Await LuaLS workspace indexing

## [0.2.0] - 2026-07-28

### 🚀 Features

- *(lsp)* Add standalone CEA language server
- *(zed)* Launch CEA language server from PATH
- *(lsp)* Proxy embedded Lua through LuaLS
- *(server)* Forward additional Lua language features
- *(server)* Forward Lua rename and code actions
- *(server)* Forward Lua inlay hints
- *(lua)* Add project LuaLS configuration
- *(server)* Add native CEA intelligence

### 🐛 Bug Fixes

- *(lsp)* Error diagnostics not emitting
- *(lsp)* Restart LuaLS after unexpected exit
- *(lsp)* Forward LuaLS request cancellation
- *(lsp)* Complete LuaLS proxy hardening
- *(lua)* Resolve workspace module paths
- *(build)* Silence generated parser warnings
- *(server)* Stabilize duplicate diagnostics

### 💼 Other

- Add release bump app

### 📚 Documentation

- Outline CEA language tooling roadmap
- *(lsp)* Define workspace-aware Lua proxy roadmap
- *(lsp)* Record remaining roadmap recommendations
- Polish readme

### 🧪 Testing

- *(lua)* Cover cross-file definition resolution
