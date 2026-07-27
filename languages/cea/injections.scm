; Parse all non-marker lines in a Lua section as one injected Lua document.
((lua_content
  (lua_line) @injection.content)
 (#set! injection.language "lua")
 (#set! injection.combined))
