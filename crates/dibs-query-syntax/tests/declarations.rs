use dibs_query_syntax::{
    DiagnosticCode, DibsParser, ParserInputEdit, ResultMode, SourceId, StatementNode,
};

fn parse(source: &str) -> dibs_query_syntax::SourceFile {
    DibsParser::new()
        .parse_strict(SourceId::test(), source)
        .unwrap_or_else(|diagnostics| panic!("strict parse failed: {diagnostics:#?}"))
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
    assert_eq!(query.parameters[1].type_name.arrays.len(), 2);
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
    assert!(query.statement.items.iter().any(
        |item| matches!(item, StatementNode::DollarQuotedLiteral(value) if value.value.starts_with("$tag$"))
    ));
    assert!(query.statement.items.iter().any(
        |item| matches!(item, StatementNode::EscapedStringLiteral(value) if value.value.starts_with("E'"))
    ));
    assert!(query.statement.items.iter().any(
        |item| matches!(item, StatementNode::UnicodeStringLiteral(value) if value.value.starts_with("U&'"))
    ));
    assert!(query.statement.items.iter().any(
        |item| matches!(item, StatementNode::QuotedIdentifier(value) if value.value == "\"quoted:identifier\"")
    ));
    assert_eq!(
        query
            .statement
            .items
            .iter()
            .filter(|item| matches!(item, StatementNode::ColonOperator(_)))
            .count(),
        2
    );
}

#[test]
fn postgresql_16_unicode_identifier_policy_accepts_non_ascii_letters() {
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
