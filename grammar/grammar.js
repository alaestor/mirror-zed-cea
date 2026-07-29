module.exports = grammar({
  name: "cea",

  extras: ($) => [/[ \t]/],

  word: ($) => $.identifier,

  rules: {
    // Recursive sections make the current {$lua}/{$asm} mode unambiguous.
    source_file: ($) =>
      seq(repeat($._asm_line), optional($.lua_section)),

    lua_section: ($) =>
      seq(
        $.lua_directive,
        $._newline,
        optional(field("content", $.lua_content)),
        optional($.asm_section),
      ),

    lua_content: ($) =>
      choice(
        $.lua_chunk,
        seq(
          optional($.lua_chunk),
          repeat1(seq($.section_header_line, optional($.lua_chunk))),
        ),
      ),

    lua_chunk: ($) =>
      repeat1(choice($.lua_line, $.empty_line)),

    asm_section: ($) =>
      seq(
        $.asm_directive_line,
        repeat($._asm_line),
        optional($.lua_section),
      ),

    lua_line: () =>
      token(prec(5, choice(/[^\r\n]+\r?\n/, /[^\r\n]+/))),

    lua_directive: () =>
      token(prec(20, /\{\$[lL][uU][aA]\}/)),

    asm_directive_line: ($) =>
      seq($.asm_directive, $._newline),

    asm_directive: () =>
      token(prec(20, /\{\$[aA][sS][mM]\}/)),

    _asm_line: ($) =>
      choice(
        $.section_header_line,
        $.compiler_directive_line,
        $.block_comment_line,
        $.line_comment_line,
        $.address_definition_line,
        $.label_definition_line,
        $.aa_command_line,
        $.operation_line,
        $.empty_line,
        $.unknown_line,
      ),

    section_header_line: ($) =>
      seq($.section_header, $._newline),

    section_header: () =>
      token(prec(20, /\[(?:[eE][nN][aA][bB][lL][eE]|[dD][iI][sS][aA][bB][lL][eE])\]/)),

    compiler_directive_line: ($) =>
      seq($.compiler_directive, $._newline),

    compiler_directive: () =>
      token(prec(15, /\{\$[A-Za-z_][A-Za-z0-9_]*\}/)),

    block_comment_line: ($) =>
      seq($.block_comment, $._newline),

    block_comment: () =>
      token(prec(10, /\{[^$][^}]*\}/)),

    line_comment_line: ($) =>
      seq($.line_comment, $._newline),

    line_comment: () =>
      token(prec(10, /\/\/[^\r\n]*/)),

    label_definition_line: ($) =>
      seq($.label_definition, optional($.line_comment), $._newline),

    address_definition_line: ($) =>
      seq($.address_definition, optional($.line_comment), $._newline),

    address_definition: () =>
      token(prec(9, choice(
        /\[[^\r\n]+:/,
        /[0-9][0-9A-Fa-f]*:/,
      ))),

    label_definition: ($) =>
      prec(10, seq(field("name", $.identifier), ":")),

    aa_command_line: ($) =>
      seq(
        $.aa_command,
        optional($.line_comment),
        $._newline,
      ),

    aa_command: ($) =>
      prec(
        5,
        seq(
          field("name", $.identifier),
          "(",
          optional($.argument_list),
          ")",
        ),
      ),

    argument_list: ($) =>
      repeat1($._atom),

    operation_line: ($) =>
      seq(
        $.operation,
        optional($.line_comment),
        $._newline,
      ),

    // Keep operation names open-ended: CE accepts extensions and new mnemonics.
    operation: ($) =>
      prec(
        1,
        seq(
          field("name", $.identifier),
          repeat($._atom),
        ),
      ),

    _atom: ($) =>
      choice(
        $.register,
        $.typed_number,
        $.number,
        $.type_cast,
        $.identifier,
        "[",
        "]",
        "+",
        "-",
        "*",
        ":",
        ",",
      ),

    register: () =>
      token(prec(7, registerPattern())),

    number: () =>
      token(prec(6, choice(
        /#[+-]?[0-9]+/,
        /\$[0-9A-Fa-f]+/,
        /0[xX][0-9A-Fa-f]+/,
        /[0-9][0-9A-Fa-f]*/,
      ))),

    typed_number: ($) =>
      prec(8, seq($.type_cast, field("value", $.decimal_value))),

    decimal_value: () =>
      token(prec(7, /[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/)),

    type_cast: () =>
      token(prec(6, /\((?:byte|word|dword|qword|int|float|double)\)/i)),

    identifier: () =>
      token(prec(1, /[A-Za-z_?.$][A-Za-z0-9_.$?]*/)),

    empty_line: ($) => $._newline,

    unknown_line: ($) =>
      seq(
        token(
          prec(
            -10,
            choice(
              /\[[^\]\r\n]*/,
              /[^A-Za-z_{$\[\/\s][^\r\n]*/,
            ),
          ),
        ),
        $._newline,
      ),

    _newline: () => /\r?\n/,
  },
});

function registerPattern() {
  return /(?:r(?:1[0-5]|[0-9])(?:b|d|w)?|[re]?(?:ax|bx|cx|dx|si|di|bp|sp)|[abcd][lh]|[cdefgs]s|[er]?ip|xmm(?:[12][0-9]|3[01]|[0-9])|ymm(?:[12][0-9]|3[01]|[0-9])|zmm(?:[12][0-9]|3[01]|[0-9]))/i;
}
