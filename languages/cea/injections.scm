; Parse each contiguous block around [ENABLE]/[DISABLE] as a Lua document.
((lua_chunk) @injection.content
 (#set! injection.language "lua"))
