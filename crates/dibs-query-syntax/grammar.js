/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const commaSep = (rule) => optional(seq(rule, repeat(seq(",", rule)), optional(",")));

module.exports = grammar({
  name: "dibs_query",

  extras: ($) => [/\s+/, $.line_comment, $.block_comment],

  externals: ($) => [$._dollar_quoted_literal],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat(field("query", $.query_decl)),

    query_decl: ($) => seq(
      repeat(field("documentation", $.documentation_comment)),
      "query",
      field("name", $.declaration_identifier),
      "(",
      commaSep(field("parameter", $.parameter_decl)),
      ")",
      "->",
      field("result_mode", $.result_mode),
      "{",
      field("statement", $.statement),
      optional(";"),
      "}",
    ),

    parameter_decl: ($) => seq(
      field("name", $.declaration_identifier),
      ":",
      field("type_name", $.pg_type_name),
      optional(field("nullable", "?")),
    ),

    pg_type_name: ($) => seq(
      optional(seq(field("schema", $.declaration_identifier), ".")),
      field("name", $.declaration_identifier),
      optional(field("typmod", $.type_modifier)),
      repeat(field("arrays", $.array_suffix)),
    ),

    type_modifier: ($) => seq(
      "(",
      field("value", $.numeric_literal),
      repeat(seq(",", field("value", $.numeric_literal))),
      ")",
    ),

    array_suffix: () => seq("[", "]"),

    result_mode: () => choice("many", "optional", "one", "exec"),

    statement: ($) => repeat1(field("item", $._statement_item)),

    _statement_item: ($) => choice(
      $.named_bind,
      $.escaped_string_literal,
      $.unicode_string_literal,
      $.bit_string_literal,
      $.hex_string_literal,
      $.string_literal,
      $.dollar_quoted_literal,
      $.quoted_identifier,
      $.numeric_literal,
      $.boolean_literal,
      $.null_literal,
      $.identifier,
      $.colon_operator,
      $.statement_symbol,
    ),

    named_bind: () => token(seq(":", /[A-Za-z_\u0080-\u{10FFFF}][A-Za-z0-9_$\u0080-\u{10FFFF}]*/u)),
    colon_operator: () => choice("::", ":="),

    escaped_string_literal: () => token(seq(/[eE]/, "'", repeat(choice("''", /\\./, /[^'\\]+/)), "'")),
    unicode_string_literal: () => token(seq(/[uU]&/, "'", repeat(choice("''", /\\./, /[^'\\]+/)), "'")),
    bit_string_literal: () => token(seq(/[bB]/, "'", repeat(choice("''", /[^']+/)), "'")),
    hex_string_literal: () => token(seq(/[xX]/, "'", repeat(choice("''", /[^']+/)), "'")),
    string_literal: () => token(seq("'", repeat(choice("''", /[^']+/)), "'")),
    dollar_quoted_literal: ($) => $._dollar_quoted_literal,

    declaration_identifier: ($) => choice($.identifier, $.quoted_identifier),
    identifier: () => /[A-Za-z_\u0080-\u{10FFFF}][A-Za-z0-9_$\u0080-\u{10FFFF}]*/u,
    quoted_identifier: () => token(seq('"', repeat(choice('""', /[^"\u0000]+/)), '"')),

    numeric_literal: () => token(choice(
      /(?:\d+\.\d*|\.\d+)(?:[eE][+-]?\d+)?/,
      /\d+[eE][+-]?\d+/,
      /\d+/,
    )),
    boolean_literal: () => choice("true", "false"),
    null_literal: () => "null",

    documentation_comment: () => token(seq("///", /[^\n]*/)),
    line_comment: () => token(seq("--", /[^\n]*/)),
    block_comment: () => token(nested("/*", "*/")),

    statement_symbol: () => token(choice(
      "(", ")", "[", "]", ",", ".", "+", "-", "*", "/", "%", "^",
      "=", "<>", "!=", "<", ">", "<=", ">=", "||", "&&", "@>", "<@",
      "->", "->>", "#>", "#>>", "?", "?|", "?&", "~", "!~", "~*", "!~*",
      "#", "@", ":", "&", "|", "!",
    )),
  },
});
