/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const commaSep = (rule) => optional(seq(rule, repeat(seq(",", rule)), optional(",")));
const commaSep1 = (rule) => seq(rule, repeat(seq(",", rule)), optional(","));
const kw = (word) => alias(new RegExp(word.split("").map((char) => `[${char.toLowerCase()}${char.toUpperCase()}]`).join("")), word);

const PREC = {
  or: 1,
  and: 2,
  not: 3,
  predicate: 4,
  compare: 5,
  generic: 6,
  additive: 7,
  multiplicative: 8,
  exponent: 9,
  unary: 10,
  postfix: 11,
  atom: 12,
};

module.exports = grammar({
  name: "dibs_query",

  extras: ($) => [/\s+/, $.line_comment, $.block_comment],
  externals: ($) => [$._dollar_quoted_literal],
  word: ($) => $.identifier,

  conflicts: ($) => [
    [$._expr, $.qualified_name],
    [$.function_call, $.aggregate_call],
    [$.parenthesized_expr, $.row_expr],
    [$.parenthesized_query, $.scalar_subquery_expr],
    [$.relation_primary, $.joined_relation],
  ],

  rules: {
    source_file: ($) => repeat(field("query", $.query_decl)),

    query_decl: ($) => seq(
      repeat(field("documentation", $.documentation_comment)),
      kw("query"),
      field("name", $.declaration_identifier),
      "(",
      commaSep(field("parameter", $.parameter_decl)),
      ")",
      "->",
      field("result_mode", $.result_mode),
      "{",
      field("statement", $._statement),
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
      repeat(field("array", $.array_suffix)),
    ),

    type_modifier: ($) => seq(
      "(",
      field("value", $.numeric_literal),
      repeat(seq(",", field("value", $.numeric_literal))),
      ")",
    ),
    array_suffix: () => seq("[", "]"),
    result_mode: () => choice(kw("many"), kw("optional"), kw("one"), kw("exec")),

    _statement: ($) => choice(
      $.with_statement,
      $.select_statement,
      $.values_statement,
      $.insert_statement,
      $.update_statement,
      $.delete_statement,
    ),

    statement_body: ($) => field("value", $._statement),

    with_statement: ($) => seq(
      field("with", $.with_clause),
      field("body", $.statement_body),
    ),

    with_clause: ($) => seq(
      kw("with"),
      optional(field("recursive", $.recursive_marker)),
      commaSep1(field("cte", $.common_table_expr)),
    ),

    common_table_expr: ($) => seq(
      field("name", $.declaration_identifier),
      optional(field("columns", $.column_name_list)),
      kw("as"),
      optional(field("materialization", $.materialization_clause)),
      "(",
      field("statement", $.statement_body),
      ")",
    ),

    materialization_clause: () => choice(
      kw("materialized"),
      seq(kw("not"), kw("materialized")),
    ),

    select_statement: ($) => prec.left(seq(
      field("body", $.select_set_expression),
      optional(field("order_by", $.order_by_clause)),
      optional(field("limit", $.limit_clause)),
      optional(field("offset", $.offset_clause)),
      optional(field("fetch", $.fetch_clause)),
      repeat(field("lock", $.locking_clause)),
    )),

    select_set_expression: ($) => field("value", choice($.select_core, $.values_statement, $.parenthesized_query, $.set_operation)),

    set_operation: ($) => choice(
      prec.left(1, seq(
        field("left", $.select_set_expression),
        field("operator", $.union_except_operator),
        optional(field("quantifier", $.set_quantifier)),
        field("right", $.select_set_expression),
      )),
      prec.left(2, seq(
        field("left", $.select_set_expression),
        field("operator", $.intersect_operator),
        optional(field("quantifier", $.set_quantifier)),
        field("right", $.select_set_expression),
      )),
    ),

    set_quantifier: () => choice(kw("all"), kw("distinct")),

    parenthesized_query: ($) => seq("(", field("statement", $.statement_body), ")"),

    select_core: ($) => seq(
      kw("select"),
      optional(field("quantifier", $.select_quantifier)),
      commaSep1(field("target", $.select_target)),
      optional(field("from", $.from_clause)),
      optional(field("where", $.where_clause)),
      optional(field("group_by", $.group_by_clause)),
      optional(field("having", $.having_clause)),
      optional(field("window", $.window_clause)),
    ),

    select_quantifier: ($) => choice(
      kw("all"),
      kw("distinct"),
      seq(kw("distinct"), kw("on"), "(", commaSep1(field("expression", $._expr)), ")"),
    ),

    select_target: ($) => field("value", choice(
      $.wildcard_target,
      $.qualified_wildcard_target,
      $.expression_target,
    )),
    wildcard_target: () => "*",
    qualified_wildcard_target: ($) => seq(field("qualifier", $.qualified_name), ".", "*"),
    expression_target: ($) => seq(
      field("expression", $._expr),
      optional(field("alias", $.column_alias)),
    ),
    column_alias: ($) => seq(kw("as"), field("name", $.declaration_identifier)),

    from_clause: ($) => seq(kw("from"), commaSep1(field("relation", $._relation))),
    _relation: ($) => choice($.joined_relation, $.table_relation, $.derived_relation, $.function_relation, $.parenthesized_relation),

    relation_primary: ($) => field("value", choice(
      $.table_relation,
      $.derived_relation,
      $.function_relation,
      $.parenthesized_relation,
    )),

    table_relation: ($) => seq(
      optional(field("only", $.only_marker)),
      field("name", $.qualified_name),
      optional(field("alias", $.relation_alias)),
    ),

    derived_relation: ($) => seq(
      optional(field("lateral", $.lateral_marker)),
      "(",
      field("statement", $.statement_body),
      ")",
      optional(field("alias", $.relation_alias)),
    ),

    function_relation: ($) => seq(
      optional(field("lateral", $.lateral_marker)),
      field("function", $.function_call),
      optional(field("ordinality", $.with_ordinality_clause)),
      optional(field("alias", $.relation_alias)),
    ),

    with_ordinality_clause: () => seq(kw("with"), kw("ordinality")),

    parenthesized_relation: ($) => seq(
      "(",
      field("relation", $._relation),
      ")",
      optional(field("alias", $.relation_alias)),
    ),

    relation_alias: ($) => seq(
      optional(kw("as")),
      field("name", $.declaration_identifier),
      optional(field("columns", $.column_name_list)),
    ),

    column_name_list: ($) => seq("(", commaSep1(field("column", $.declaration_identifier)), ")"),

    joined_relation: ($) => prec.left(seq(
      field("left", $.relation_primary),
      repeat1(field("join", $.join_tail)),
    )),

    join_tail: ($) => seq(
      field("operator", $.join_operator),
      field("right", $.relation_primary),
      optional(field("condition", $.join_condition)),
    ),

    join_operator: () => choice(
      kw("join"),
      seq(kw("inner"), kw("join")),
      seq(kw("left"), optional(kw("outer")), kw("join")),
      seq(kw("right"), optional(kw("outer")), kw("join")),
      seq(kw("full"), optional(kw("outer")), kw("join")),
      seq(kw("cross"), kw("join")),
      seq(kw("natural"), optional(choice(kw("inner"), kw("left"), kw("right"), kw("full"))), optional(kw("outer")), kw("join")),
    ),

    join_condition: ($) => choice(
      seq(kw("on"), field("expression", $._expr)),
      seq(kw("using"), field("columns", $.column_name_list)),
    ),

    where_clause: ($) => seq(kw("where"), field("expression", $._expr)),
    having_clause: ($) => seq(kw("having"), field("expression", $._expr)),

    group_by_clause: ($) => seq(
      kw("group"), kw("by"),
      optional(field("quantifier", $.set_quantifier)),
      commaSep1(field("element", $.grouping_element)),
    ),

    grouping_element: ($) => field("value", choice(
      $.grouping_expression,
      $.empty_grouping_set,
      $.grouping_tuple,
      $.rollup_clause,
      $.cube_clause,
      $.grouping_sets_clause,
    )),
    grouping_expression: ($) => field("expression", $._expr),
    empty_grouping_set: () => seq("(", ")"),
    grouping_tuple: ($) => seq("(", commaSep1(field("expression", $._expr)), ")"),
    rollup_clause: ($) => seq(kw("rollup"), "(", commaSep1(field("element", $.grouping_element)), ")"),
    cube_clause: ($) => seq(kw("cube"), "(", commaSep1(field("element", $.grouping_element)), ")"),
    grouping_sets_clause: ($) => seq(kw("grouping"), kw("sets"), "(", commaSep1(field("element", $.grouping_element)), ")"),

    window_clause: ($) => seq(kw("window"), commaSep1(field("definition", $.named_window_definition))),
    named_window_definition: ($) => seq(
      field("name", $.declaration_identifier),
      kw("as"),
      field("specification", $.window_specification),
    ),

    window_specification: ($) => seq(
      "(",
      optional(field("base", $.declaration_identifier)),
      optional(field("partition", $.partition_by_clause)),
      optional(field("order_by", $.order_by_clause)),
      optional(field("frame", $.window_frame_clause)),
      ")",
    ),

    partition_by_clause: ($) => seq(kw("partition"), kw("by"), commaSep1(field("expression", $._expr))),

    window_frame_clause: ($) => seq(
      field("mode", $.frame_mode),
      choice(
        field("start", $.frame_bound),
        seq(kw("between"), field("start", $.frame_bound), kw("and"), field("end", $.frame_bound)),
      ),
      optional(field("exclusion", $.frame_exclusion)),
    ),
    frame_mode: () => choice(kw("range"), kw("rows"), kw("groups")),
    frame_bound: ($) => choice(
      seq(kw("unbounded"), field("direction", $.frame_direction)),
      seq(kw("current"), kw("row")),
      seq(field("offset", $._expr), field("direction", $.frame_direction)),
    ),
    frame_exclusion: () => choice(
      seq(kw("exclude"), kw("current"), kw("row")),
      seq(kw("exclude"), kw("group")),
      seq(kw("exclude"), kw("ties")),
      seq(kw("exclude"), kw("no"), kw("others")),
    ),

    order_by_clause: ($) => seq(kw("order"), kw("by"), commaSep1(field("item", $.order_by_item))),
    order_by_item: ($) => seq(
      field("expression", $._expr),
      optional(field("using_operator", $.using_operator_clause)),
      optional(field("direction", $.sort_direction)),
      optional(field("nulls", $.nulls_order_clause)),
    ),
    using_operator_clause: ($) => seq(kw("using"), field("operator", $.operator_name)),
    nulls_order_clause: () => seq(kw("nulls"), choice(kw("first"), kw("last"))),

    limit_clause: ($) => seq(kw("limit"), field("value", choice($._expr, $.all_literal))),
    offset_clause: ($) => seq(kw("offset"), field("value", $._expr), optional(choice(kw("row"), kw("rows")))),
    fetch_clause: ($) => seq(
      kw("fetch"),
      choice(kw("first"), kw("next")),
      optional(field("value", $._expr)),
      choice(kw("row"), kw("rows")),
      field("policy", $.fetch_policy),
    ),
    all_literal: () => kw("all"),
    recursive_marker: () => kw("recursive"),
    only_marker: () => kw("only"),
    lateral_marker: () => kw("lateral"),
    not_marker: () => kw("not"),
    symmetric_marker: () => kw("symmetric"),
    union_except_operator: () => choice(kw("union"), kw("except")),
    intersect_operator: () => kw("intersect"),
    frame_direction: () => choice(kw("preceding"), kw("following")),
    sort_direction: () => choice(kw("asc"), kw("desc")),
    fetch_policy: () => choice(kw("only"), seq(kw("with"), kw("ties"))),
    is_test: ($) => choice(
      kw("null"), kw("true"), kw("false"), kw("unknown"), kw("document"),
      seq(kw("normalized"), optional(field("form", $.normal_form))),
    ),
    pattern_operator: () => choice(kw("like"), kw("ilike"), kw("similar")),
    comparison_quantifier: () => choice(kw("any"), kw("some"), kw("all")),
    aggregate_quantifier: () => choice(kw("all"), kw("distinct")),

    locking_clause: ($) => seq(
      kw("for"),
      field("strength", $.lock_strength),
      optional(seq(kw("of"), commaSep1(field("target", $.qualified_name)))),
      optional(field("wait", $.lock_wait_policy)),
    ),
    lock_strength: () => choice(
      kw("update"),
      seq(kw("no"), kw("key"), kw("update")),
      kw("share"),
      seq(kw("key"), kw("share")),
    ),
    lock_wait_policy: () => choice(kw("nowait"), seq(kw("skip"), kw("locked"))),

    values_statement: ($) => seq(kw("values"), commaSep1(field("row", $.values_row))),
    values_row: ($) => seq("(", commaSep1(field("value", $.insert_value)), ")"),
    insert_value: ($) => field("value", choice($.default_literal, $._expr)),
    default_literal: () => kw("default"),

    insert_statement: ($) => seq(
      kw("insert"), kw("into"),
      field("target", $.qualified_name),
      optional(field("alias", $.relation_alias)),
      optional(field("columns", $.column_name_list)),
      field("source", $.insert_source),
      optional(field("conflict", $.conflict_clause)),
      optional(field("returning", $.returning_clause)),
    ),
    insert_source: ($) => field("value", choice(
      $.values_statement,
      $.select_statement,
      $.with_statement,
      $.default_values_clause,
    )),
    default_values_clause: () => seq(kw("default"), kw("values")),

    conflict_clause: ($) => seq(
      kw("on"), kw("conflict"),
      optional(field("target", $.conflict_target)),
      kw("do"),
      field("action", $.conflict_action),
    ),
    conflict_target: ($) => field("value", choice(
      $.conflict_inference,
      $.conflict_constraint,
    )),
    conflict_inference: ($) => seq(
      "(", commaSep1(field("element", $.conflict_target_element)), ")",
      optional(field("predicate", $.where_clause)),
    ),
    conflict_constraint: ($) => seq(kw("on"), kw("constraint"), field("constraint", $.qualified_name)),
    conflict_target_element: ($) => seq(
      field("expression", $._expr),
      optional(field("collation", $.collate_clause)),
      optional(field("operator_class", $.qualified_name)),
    ),
    conflict_action: ($) => field("value", choice(
      $.conflict_do_nothing,
      $.conflict_do_update,
    )),
    conflict_do_nothing: () => kw("nothing"),
    conflict_do_update: ($) => seq(
      kw("update"), kw("set"),
      commaSep1(field("assignment", $.assignment)),
      optional(field("predicate", $.where_clause)),
    ),

    update_statement: ($) => seq(
      kw("update"),
      field("target", $.qualified_name),
      optional(field("alias", $.relation_alias)),
      kw("set"),
      commaSep1(field("assignment", $.assignment)),
      optional(field("from", $.from_clause)),
      optional(field("where", $.where_clause)),
      optional(field("returning", $.returning_clause)),
    ),

    assignment: ($) => choice(
      seq(field("target", $.assignment_target), "=", field("value", $.insert_value)),
      seq(field("targets", $.assignment_target_list), "=", field("value", choice($.row_expr, $.scalar_subquery_expr))),
    ),
    assignment_target: ($) => seq(
      field("name", $.declaration_identifier),
      repeat(field("indirection", $.indirection)),
    ),
    assignment_target_list: ($) => seq("(", commaSep1(field("target", $.assignment_target)), ")"),

    delete_statement: ($) => seq(
      kw("delete"), kw("from"),
      field("target", $.qualified_name),
      optional(field("alias", $.relation_alias)),
      optional(field("using", $.using_clause)),
      optional(field("where", $.where_clause)),
      optional(field("returning", $.returning_clause)),
    ),
    using_clause: ($) => seq(kw("using"), commaSep1(field("relation", $._relation))),
    returning_clause: ($) => seq(kw("returning"), commaSep1(field("target", $.select_target))),

    _expr: ($) => choice(
      $.or_expr,
      $.and_expr,
      $.not_expr,
      $.is_test_expr,
      $.distinct_test_expr,
      $.between_expr,
      $.in_expr,
      $.like_expr,
      $.quantified_comparison_expr,
      $.binary_expr,
      $.unary_expr,
      $.postfix_expr,
      $._atom_expr,
    ),

    _atom_expr: ($) => choice(
      $.function_call,
      $.case_expr,
      $.coalesce_expr,
      $.nullif_expr,
      $.greatest_expr,
      $.least_expr,
      $.extract_expr,
      $.position_expr,
      $.substring_expr,
      $.overlay_expr,
      $.exists_expr,
      $.scalar_subquery_expr,
      $.array_expr,
      $.row_expr,
      $.parenthesized_expr,
      $.qualified_name_expr,
      $.named_bind,
      $.escaped_string_literal,
      $.unicode_string_literal,
      $.bit_string_literal,
      $.hex_string_literal,
      $.string_literal,
      $.dollar_quoted_literal,
      $.interval_literal,
      $.numeric_literal,
      $.boolean_literal,
      $.null_literal,
      $.current_value_expr,
    ),

    or_expr: ($) => prec.left(PREC.or, seq(field("left", $._expr), kw("or"), field("right", $._expr))),
    and_expr: ($) => prec.left(PREC.and, seq(field("left", $._expr), kw("and"), field("right", $._expr))),
    not_expr: ($) => prec.right(PREC.not, seq(kw("not"), field("expression", $._expr))),

    is_test_expr: ($) => prec.left(PREC.predicate, seq(
      field("expression", $._expr),
      kw("is"), optional(field("negated", $.not_marker)),
      field("test", $.is_test),
    )),
    normal_form: () => choice(kw("nfc"), kw("nfd"), kw("nfkc"), kw("nfkd")),

    distinct_test_expr: ($) => prec.left(PREC.predicate, seq(
      field("left", $._expr), kw("is"), optional(field("negated", $.not_marker)), kw("distinct"), kw("from"), field("right", $._expr),
    )),

    between_expr: ($) => prec.left(PREC.predicate, seq(
      field("expression", $._expr),
      optional(field("negated", $.not_marker)),
      kw("between"), optional(field("symmetric", $.symmetric_marker)),
      field("lower", $._expr), kw("and"), field("upper", $._expr),
    )),

    in_expr: ($) => prec.left(PREC.predicate, seq(
      field("expression", $._expr), optional(field("negated", $.not_marker)), kw("in"), field("values", $.in_rhs),
    )),
    in_rhs: ($) => choice(
      seq("(", commaSep1(field("value", $._expr)), ")"),
      $.parenthesized_query,
    ),

    like_expr: ($) => prec.left(PREC.predicate, seq(
      field("expression", $._expr), optional(field("negated", $.not_marker)),
      field("operator", $.pattern_operator),
      optional(kw("to")),
      field("pattern", $._expr),
      optional(seq(kw("escape"), field("escape", $._expr))),
    )),

    quantified_comparison_expr: ($) => prec.left(PREC.compare, seq(
      field("left", $._expr), field("operator", $.comparison_operator),
      field("quantifier", $.comparison_quantifier),
      "(", field("right", choice($._expr, $.statement_body)), ")",
    )),

    binary_expr: ($) => choice(
      prec.left(PREC.compare, seq(field("left", $._expr), field("operator", $.comparison_operator), field("right", $._expr))),
      prec.left(PREC.generic, seq(field("left", $._expr), field("operator", $.generic_operator), field("right", $._expr))),
      prec.left(PREC.additive, seq(field("left", $._expr), field("operator", $.additive_operator), field("right", $._expr))),
      prec.left(PREC.multiplicative, seq(field("left", $._expr), field("operator", $.multiplicative_operator), field("right", $._expr))),
      prec.left(PREC.exponent, seq(field("left", $._expr), field("operator", $.exponent_operator), field("right", $._expr))),
    ),

    comparison_operator: () => choice("=", "<>", "!=", "<", ">", "<=", ">="),
    generic_operator: () => choice(
      "&&", "@>", "<@", "->", "->>", "#>", "#>>", "#-", "?", "?|", "?&",
      "~", "!~", "~*", "!~*", "&", "|", "#", "<<", ">>", "<->", "<=>",
    ),
    additive_operator: () => choice("+", "-", "||"),
    multiplicative_operator: () => choice("*", "/", "%"),
    exponent_operator: () => "^",
    unary_operator: () => choice("+", "-", "~", "@"),
    operator_name: ($) => choice($.comparison_operator, $.generic_operator, "+", "-", "*", "/", "%", "^", "||"),

    unary_expr: ($) => prec.right(PREC.unary, seq(field("operator", $.unary_operator), field("expression", $._expr))),

    postfix_expr: ($) => prec.left(PREC.postfix, seq(
      field("base", $._atom_expr),
      repeat1(field("operation", $.postfix_operation)),
    )),
    postfix_operation: ($) => field("value", choice(
      $.cast_suffix,
      $.collate_suffix,
      $.at_time_zone_suffix,
      $.indirection,
      $.window_suffix,
    )),
    cast_suffix: ($) => seq("::", field("type_name", $.pg_type_name)),
    collate_suffix: ($) => field("collation", $.collate_clause),
    collate_clause: ($) => seq(kw("collate"), field("name", $.qualified_name)),
    at_time_zone_suffix: ($) => seq(kw("at"), kw("time"), kw("zone"), field("zone", $._expr)),
    window_suffix: ($) => seq(kw("over"), field("window", choice($.declaration_identifier, $.window_specification))),
    indirection: ($) => choice(
      seq("[", optional(field("lower", $._expr)), optional(seq(":", optional(field("upper", $._expr)))), "]"),
      seq(".", field("name", $.declaration_identifier)),
      seq(".", "*"),
    ),

    cast_expr: ($) => seq(kw("cast"), "(", field("expression", $._expr), kw("as"), field("type_name", $.pg_type_name), ")"),

    function_call: ($) => prec(PREC.atom, seq(
      field("name", $.qualified_name),
      "(",
      commaSep(field("argument", $.function_argument)),
      ")",
    )),
    function_argument: ($) => choice(
      seq(field("name", $.declaration_identifier), field("notation", choice("=>", ":=")), field("value", $._expr)),
      field("value", $._expr),
    ),

    aggregate_call: ($) => prec(PREC.atom, seq(
      field("name", $.qualified_name),
      "(",
      choice(
        field("star", "*"),
        seq(
          optional(field("quantifier", $.aggregate_quantifier)),
          commaSep(field("argument", $.function_argument)),
          optional(field("order_by", $.order_by_clause)),
        ),
      ),
      ")",
      optional(field("within_group", $.within_group_clause)),
      optional(field("filter", $.filter_clause)),
    )),
    within_group_clause: ($) => seq(kw("within"), kw("group"), "(", field("order_by", $.order_by_clause), ")"),
    filter_clause: ($) => seq(kw("filter"), "(", kw("where"), field("expression", $._expr), ")"),

    window_expr: ($) => prec.left(PREC.postfix, seq(
      field("expression", choice($.aggregate_call, $.function_call)),
      kw("over"),
      field("window", choice($.declaration_identifier, $.window_specification)),
    )),

    case_expr: ($) => seq(
      kw("case"),
      optional(field("operand", $._expr)),
      repeat1(field("branch", $.case_branch)),
      optional(seq(kw("else"), field("else_expression", $._expr))),
      kw("end"),
    ),
    case_branch: ($) => seq(kw("when"), field("when", $._expr), kw("then"), field("then", $._expr)),

    coalesce_expr: ($) => seq(kw("coalesce"), "(", commaSep1(field("argument", $._expr)), ")"),
    nullif_expr: ($) => seq(kw("nullif"), "(", field("left", $._expr), ",", field("right", $._expr), ")"),
    greatest_expr: ($) => seq(kw("greatest"), "(", commaSep1(field("argument", $._expr)), ")"),
    least_expr: ($) => seq(kw("least"), "(", commaSep1(field("argument", $._expr)), ")"),

    extract_expr: ($) => seq(kw("extract"), "(", field("field", $.extract_field), kw("from"), field("source", $._expr), ")"),
    extract_field: () => choice(
      kw("century"), kw("day"), kw("decade"), kw("dow"), kw("doy"), kw("epoch"), kw("hour"), kw("isodow"), kw("isoyear"), kw("julian"), kw("microseconds"), kw("millennium"), kw("milliseconds"), kw("minute"), kw("month"), kw("quarter"), kw("second"), kw("timezone"), kw("timezone_hour"), kw("timezone_minute"), kw("week"), kw("year"),
    ),

    position_expr: ($) => seq(kw("position"), "(", field("substring", $._expr), kw("in"), field("string", $._expr), ")"),
    substring_expr: ($) => seq(
      kw("substring"), "(", field("string", $._expr),
      choice(
        seq(kw("from"), field("start", $._expr), optional(seq(kw("for"), field("count", $._expr)))),
        seq(kw("for"), field("count", $._expr)),
        seq(",", field("start", $._expr), optional(seq(",", field("count", $._expr)))),
      ),
      ")",
    ),
    overlay_expr: ($) => seq(
      kw("overlay"), "(", field("string", $._expr), kw("placing"), field("replacement", $._expr), kw("from"), field("start", $._expr), optional(seq(kw("for"), field("count", $._expr))), ")",
    ),

    exists_expr: ($) => seq(kw("exists"), "(", field("statement", $.statement_body), ")"),
    scalar_subquery_expr: ($) => seq("(", field("statement", $.statement_body), ")"),

    array_expr: ($) => choice(
      seq(kw("array"), "[", commaSep(field("element", $._expr)), "]"),
      seq(kw("array"), "(", field("statement", $.statement_body), ")"),
    ),
    row_expr: ($) => choice(
      seq(kw("row"), "(", commaSep(field("element", $._expr)), ")"),
      seq("(", field("element", $._expr), ",", commaSep(field("element", $._expr)), ")"),
    ),
    parenthesized_expr: ($) => seq("(", field("expression", $._expr), ")"),

    current_value_expr: () => choice(
      kw("current_date"), kw("current_time"), kw("current_timestamp"), kw("localtime"), kw("localtimestamp"), kw("current_user"), kw("session_user"), kw("system_user"), kw("current_role"), kw("current_catalog"), kw("current_schema"),
    ),

    interval_literal: ($) => seq(
      kw("interval"),
      field("value", choice($.string_literal, $.escaped_string_literal)),
      optional(field("field", $.interval_field)),
      optional(seq(kw("to"), field("to_field", $.interval_field))),
      optional(seq("(", field("precision", $.numeric_literal), ")")),
    ),
    interval_field: () => choice(kw("year"), kw("month"), kw("day"), kw("hour"), kw("minute"), kw("second")),

    qualified_name_expr: ($) => field("name", $.qualified_name),
    qualified_name: ($) => seq(field("part", $.declaration_identifier), repeat(seq(".", field("part", $.declaration_identifier)))),

    named_bind: () => token(seq(":", /[A-Za-z_\u0080-\u{10FFFF}][A-Za-z0-9_$\u0080-\u{10FFFF}]*/u)),
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
    boolean_literal: () => choice(kw("true"), kw("false")),
    null_literal: () => kw("null"),

    documentation_comment: () => token(seq("///", /[^\n]*/)),
    line_comment: () => token(seq("--", /[^\n]*/)),
    block_comment: () => token(nested("/*", "*/")),
  },
});
