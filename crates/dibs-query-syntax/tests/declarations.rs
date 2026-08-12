use dibs_query_syntax::{DiagnosticCode, DibsParser, ParserInputEdit, ResultMode, SourceId};
use std::sync::LazyLock;

static PARSER: LazyLock<DibsParser> = LazyLock::new(DibsParser::new);

fn parser() -> &'static DibsParser {
    &PARSER
}

fn parse(source: &str) -> dibs_query_syntax::SourceFile {
    parser()
        .parse_strict(SourceId::test(), source)
        .unwrap_or_else(|diagnostics| panic!("strict parse failed: {diagnostics:#?}"))
}

#[test]
fn parser_reports_postgresql_18_language_identity() {
    let version = DibsParser::new().language_version();
    assert_eq!(version.grammar, 1);
    assert_eq!(version.postgres_major, 18);
}

#[test]
fn parses_query_signature_and_named_binds() {
    let source = r#"
query FindRun(id: bigint, owner: text?,) -> optional {
    select id from run where id = :id and owner = :owner
}
"#;
    let file = parse(source);
    let query = &file.queries[0];
    assert_eq!(query.name.value, "FindRun");
    assert!(query.parameters[1].nullable);
    assert_eq!(query.result_mode, ResultMode::Optional);
    assert_eq!(
        query
            .bind_occurrences()
            .map(|bind| bind.value.as_str())
            .collect::<Vec<_>>(),
        [":id", ":owner"]
    );
}

#[test]
fn parses_documentation_qualified_types_typmods_arrays_and_semicolon() {
    let file = parse(
        r#"
/// Fetch rows.
query Q(amount: pg_catalog.numeric(20, 6)?, ids: uuid[][]) -> many {
    select :amount, :ids;
}
"#,
    );
    let query = &file.queries[0];
    assert_eq!(query.documentations[0].value, "/// Fetch rows.");
    assert_eq!(
        query.parameters[0].type_name.schema.as_ref().unwrap().value,
        "pg_catalog"
    );
    assert_eq!(query.parameters[0].type_name.name.value, "numeric");
    assert_eq!(
        query.parameters[0]
            .type_name
            .typmod
            .as_ref()
            .unwrap()
            .values
            .iter()
            .map(|value| value.value.as_str())
            .collect::<Vec<_>>(),
        ["20", "6"]
    );
    assert_eq!(query.parameters[1].type_name.arraies.len(), 2);
    assert_eq!(query.result_mode, ResultMode::Many);
}

#[test]
fn bind_lexer_ignores_postgresql_quoted_regions_and_casts() {
    let source = r#"query Q(x: text) -> one { select ':x', $$:x$$, :x::text }"#;
    let file = parse(source);
    assert_eq!(file.queries[0].bind_occurrences().count(), 1);
}

#[test]
fn lexical_negative_fixture_preserves_only_real_binds() {
    let source = include_str!("fixtures/lexical-negative.dibs");
    let file = parse(source);
    let query = &file.queries[0];
    assert_eq!(
        query
            .bind_occurrences()
            .map(|bind| bind.value.as_str())
            .collect::<Vec<_>>(),
        [":real"]
    );
    let parser = DibsParser::new();
    let recovered = parser.parse_recovering(SourceId::test(), source).unwrap();
    let kinds = recovered
        .tree
        .descendants()
        .map(|node| node.kind().to_owned())
        .collect::<Vec<_>>();
    assert!(kinds.iter().any(|kind| kind == "dollar_quoted_literal"));
    assert!(kinds.iter().any(|kind| kind == "escaped_string_literal"));
    assert!(kinds.iter().any(|kind| kind == "unicode_string_literal"));
    assert!(kinds.iter().any(|kind| kind == "quoted_identifier"));
    assert!(kinds.iter().any(|kind| kind == "cast_expr"));
    assert!(kinds.iter().any(|kind| kind == "function_argument"));
}

#[test]
fn postgresql_18_unicode_identifier_policy_accepts_non_ascii_letters() {
    let file = parse("query Ångström(étiquette: text) -> one { select :étiquette }");
    assert_eq!(file.queries[0].name.value, "Ångström");
    assert_eq!(file.queries[0].parameters[0].name.value, "étiquette");
    assert_eq!(file.queries[0].bind_occurrences().count(), 1);
}

#[test]
fn strict_mode_rejects_missing_and_recovered_facts() {
    let parser = DibsParser::new();
    let diagnostics = parser
        .parse_strict(
            SourceId::test(),
            "query Broken(id: bigint) -> optional { select :id",
        )
        .unwrap_err();
    assert!(!diagnostics.is_empty());
    assert!(diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            DiagnosticCode::ParseFailed
                | DiagnosticCode::UnexpectedToken
                | DiagnosticCode::MissingToken
        )
    }));
}

#[test]
fn strict_mode_rejects_recovery_diagnostics_before_lowering() {
    let parser = DibsParser::new();
    let source = "query Broken() -> one { select ; 1 }";
    let recovering = parser.parse_recovering(SourceId::test(), source).unwrap();
    assert!(!recovering.diagnostics.is_empty());
    assert!(recovering.tree.contains_error());

    let strict = parser.parse_strict(SourceId::test(), source).unwrap_err();
    assert!(strict.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            DiagnosticCode::ParseFailed
                | DiagnosticCode::UnexpectedToken
                | DiagnosticCode::MissingToken
        )
    }));
    assert!(
        strict
            .iter()
            .all(|diagnostic| diagnostic.code != DiagnosticCode::AstLoweringFailed)
    );
}

#[test]
fn strict_parse_failure_preserves_parser_byte_position() {
    let parser = DibsParser::new();
    let source = "query Broken() -> one { select ; 1 }";
    let expected = 33;
    let diagnostics = parser.parse_strict(SourceId::test(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].primary,
        dibs_query_syntax::Span::empty(expected)
    );
    assert!(diagnostics[0].message.contains(&format!("byte {expected}")));
}

#[test]
fn recovering_session_uses_dollar_quote_scanner() {
    let parser = DibsParser::new();
    let mut session = parser.session(SourceId::test());
    let recovered = session
        .parse_recovering("query Q() -> one { select $$:not_a_bind$$ }")
        .unwrap();
    assert!(recovered.diagnostics.is_empty());
}

#[test]
fn recovering_edit_preserves_surrounding_query_nodes() {
    let parser = DibsParser::new();
    let source = include_str!("fixtures/recovery.dibs");
    let mut session = parser.session(SourceId::test());
    let initial = session.parse_recovering(source).unwrap();
    assert!(initial.diagnostics.is_empty());

    let needle = "select :middle";
    let replacement = "select ; :middle";
    let start = source.find(needle).unwrap();
    let mut edited = source.to_owned();
    edited.replace_range(start..start + needle.len(), replacement);
    let recovered = session
        .reparse_recovering(
            ParserInputEdit::new(start, start + needle.len(), start + replacement.len()),
            edited,
        )
        .unwrap();

    assert!(!recovered.diagnostics.is_empty());
    let query_count = recovered
        .tree
        .descendants()
        .filter(|node| node.kind() == "query_decl")
        .count();
    assert_eq!(query_count, 3, "recovered tree: {:#?}", recovered.tree);
}

#[test]
fn parses_expression_precedence_and_postgresql_special_forms() {
    let file = parse(include_str!("fixtures/expressions.dibs"));
    let query = &file.queries[0];
    assert_eq!(query.name.value, "ExpressionForms");
    assert_eq!(query.bind_occurrences().count(), 12);
}

#[test]
fn parses_joins_lateral_derived_tables_and_registered_table_functions() {
    let file = parse(include_str!("fixtures/relations.dibs"));
    assert_eq!(file.queries[0].name.value, "RelationForms");
    assert_eq!(file.queries[0].bind_occurrences().count(), 2);
}

#[test]
fn parses_plain_distinct_distinct_on_wildcard_and_all_selects() {
    let file = parse(include_str!("fixtures/select-quantifiers.dibs"));
    assert_eq!(file.queries.len(), 5);
    assert_eq!(file.queries[0].name.value, "DistinctTargets");
    assert_eq!(file.queries[1].name.value, "DistinctWildcard");
    assert_eq!(file.queries[2].name.value, "DistinctOnTargets");
    assert_eq!(file.queries[3].name.value, "AllTargets");
    assert_eq!(file.queries[4].name.value, "QualifiedWildcard");

    let source = include_str!("fixtures/select-quantifiers.dibs");
    let parsed = DibsParser::new()
        .parse_recovering(SourceId::test(), source)
        .unwrap();
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let kinds = parsed
        .tree
        .descendants()
        .map(|node| node.kind().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(kinds.contains("distinct_select"));
    assert!(kinds.contains("plain_distinct_select"));
    assert!(kinds.contains("distinct_on_select"));
    assert!(kinds.contains("wildcard_target"));
    assert!(kinds.contains("all_select"));
    assert!(kinds.contains("qualified_wildcard_target"));
}

#[test]
fn parses_grouping_having_filters_and_within_group_aggregates() {
    let file = parse(include_str!("fixtures/aggregates.dibs"));
    assert_eq!(file.queries[0].name.value, "AggregateForms");
}

#[test]
fn parses_named_windows_and_every_frame_family_absent_from_trials() {
    let file = parse(include_str!("fixtures/windows.dibs"));
    assert_eq!(file.queries[0].name.value, "WindowForms");
}
#[test]
fn window_fixture_cst_contains_all_frame_exclusion_kinds() {
    for (index, exclusion) in [
        "exclude current row",
        "exclude group",
        "exclude ties",
        "exclude no others",
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!(
            "query Window{index}() -> many {{ select sum(value) over (rows between unbounded preceding and current row {exclusion}) from sample }}"
        );
        let file = parse(&source);
        assert_eq!(file.queries.len(), 1);
    }
}

#[test]
fn parses_set_operations_attached_order_limit_offset_and_values() {
    let file = parse(include_str!("fixtures/sets-values.dibs"));
    assert_eq!(file.queries[0].name.value, "SetAndValues");
    assert_eq!(file.queries[0].bind_occurrences().count(), 1);
}

#[test]
fn parses_insert_update_delete_returning_and_on_conflict() {
    let source = include_str!("fixtures/mutations.dibs");
    let starts = source
        .match_indices("query ")
        .map(|(start, _)| start)
        .chain(std::iter::once(source.len()))
        .collect::<Vec<_>>();

    for (index, bounds) in starts.windows(2).enumerate() {
        let file = parse(&source[bounds[0]..bounds[1]]);
        assert_eq!(file.queries.len(), 1, "mutation query {index}");
    }

    let recovered = DibsParser::new()
        .parse_recovering(SourceId::test(), source)
        .unwrap();
    assert!(
        recovered.diagnostics.is_empty(),
        "{:#?}",
        recovered.diagnostics
    );
    assert_eq!(
        recovered
            .tree
            .descendants()
            .filter(|node| node.kind() == "query_decl")
            .count(),
        4
    );

    let kinds = recovered
        .tree
        .descendants()
        .map(|node| node.kind().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(kinds.contains("insert_alias"));
    assert!(kinds.contains("qualified_wildcard_target"));
}

#[test]
fn parses_recursive_and_data_modifying_ctes_with_all_lock_forms() {
    let file = parse(include_str!("fixtures/ctes-locks.dibs"));
    assert_eq!(file.queries[0].name.value, "RecursiveAndLocks");
}

#[test]
fn parses_qualified_registered_function_calls_and_lock_targets() {
    let file = parse(include_str!("fixtures/functions-locks.dibs"));
    assert_eq!(file.queries[0].name.value, "RegisteredFunction");
    assert_eq!(file.queries[0].bind_occurrences().count(), 3);
}

#[test]
fn full_language_cst_contains_explicit_structural_nodes() {
    let parser = DibsParser::new();
    let fixtures = [
        include_str!("fixtures/expressions.dibs"),
        include_str!("fixtures/relations.dibs"),
        include_str!("fixtures/aggregates.dibs"),
        include_str!("fixtures/windows.dibs"),
        include_str!("fixtures/sets-values.dibs"),
        include_str!("fixtures/mutations.dibs"),
        include_str!("fixtures/ctes-locks.dibs"),
        include_str!("fixtures/select-quantifiers.dibs"),
        include_str!("fixtures/functions-locks.dibs"),
    ];
    let required = [
        "binary_expr",
        "case_expr",
        "joined_relation",
        "derived_relation",
        "distinct_select",
        "distinct_on_select",
        "plain_distinct_select",
        "function_relation",
        "call_expr",
        "filter_clause",
        "within_group_clause",
        "callable_window_expr",
        "window_frame_clause",
        "frame_exclusion",
        "frame_exclusion_kind",
        "set_operation",
        "values_statement",
        "insert_statement",
        "update_statement",
        "delete_statement",
        "conflict_clause",
        "with_clause",
        "locking_clause",
    ];
    let mut seen = std::collections::BTreeSet::new();
    for source in fixtures {
        let parsed = parser.parse_recovering(SourceId::test(), source).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        for node in parsed.tree.descendants() {
            if required.contains(&node.kind()) {
                seen.insert(node.kind().to_owned());
            }
        }
    }
    assert_eq!(seen, required.into_iter().map(str::to_owned).collect());
}

#[test]
fn arithmetic_precedence_tree_is_layered() {
    let parser = parser();
    let source = "query Q() -> many { select 1 + 2 * 3 }";
    let recovered = parser.parse_recovering(SourceId::test(), source).unwrap();
    assert!(
        recovered.diagnostics.is_empty(),
        "{:#?}",
        recovered.diagnostics
    );
    let additive = recovered
        .tree
        .descendants()
        .find(|node| node.kind() == "additive_expr")
        .expect("additive expression node");
    assert!(
        additive
            .children()
            .any(|child| child.kind() == "multiplicative_expr"),
        "multiplication must be a direct operand of addition"
    );
}

#[test]
fn exponent_tree_is_left_associative() {
    let parser = parser();
    let source = "query Q() -> many { select 2 ^ 3 ^ 4 }";
    let recovered = parser.parse_recovering(SourceId::test(), source).unwrap();
    assert!(
        recovered.diagnostics.is_empty(),
        "{:#?}",
        recovered.diagnostics
    );
    let exponents = recovered
        .tree
        .descendants()
        .filter(|node| node.kind() == "exponent_expr")
        .collect::<Vec<_>>();
    assert_eq!(exponents.len(), 2);
    assert!(
        exponents[0]
            .children()
            .any(|child| child.kind() == "exponent_expr"),
        "left-associated exponent must nest on the left"
    );
}
