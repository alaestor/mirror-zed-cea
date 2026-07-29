[
  (lua_directive)
  (asm_directive)
  (compiler_directive)
] @preproc

(section_header) @keyword

[
  (block_comment)
  (line_comment)
] @comment

(label_definition
  name: (identifier) @label)

(address_definition) @label

(aa_command
  name: (identifier) @function)

(operation
  name: (identifier) @function)

(register) @variable.special
(number) @number
(decimal_value) @number
(type_cast) @type

[
  "("
  ")"
  "["
  "]"
] @punctuation.bracket

[
  ","
  ":"
] @punctuation.delimiter

[
  "+"
  "-"
  "*"
] @operator
