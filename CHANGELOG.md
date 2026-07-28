# Changelog

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
