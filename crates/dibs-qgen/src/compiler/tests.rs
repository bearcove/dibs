use dibs_db_schema::{Column, PgType, Schema, SourceLocation, Table};
use dibs_pg_catalog::CatalogSnapshot;
use dibs_query_ir::{HirExpressionKind, HirStatementKind};
use dibs_query_syntax::{DibsParser, SourceId};
use indexmap::IndexMap;

use super::{CompileDiagnosticCode, resolve::resolve_file};

#[test]
fn parsed_ast_resolves_relation_fields_aliases_and_parameters_to_ids() {
    let source = r#"query Find(id: bigint) -> many {
        select r.id as run_id
        from run as r
        where r.id = :id
        order by run_id
        limit 1 offset 0
    }"#;
    let source_id = SourceId::new(91);
    let file = DibsParser::new().parse_strict(source_id, source).unwrap();
    let resolved = resolve_file(
        source_id,
        file,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .unwrap();

    let query = &resolved[0].hir;
    assert_eq!(query.parameters[0].id.get(), 1);
    assert_eq!(query.parameters[0].ordinal, 0);
    let HirStatementKind::Select(select) = &query.statement.kind else {
        panic!("expected select")
    };
    assert_eq!(select.from[0].id.get(), 1);
    assert_eq!(select.projections[0].field_id.get(), 1);
    assert_eq!(select.projections[0].alias, "run_id");
    assert!(matches!(
        select.projections[0].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding.get() == 1
    ));
    assert!(select.predicate.is_some());
    assert_eq!(select.order_by.len(), 1);
    assert!(select.limit.is_some());
    assert!(select.offset.is_some());
}

#[test]
fn resolver_reports_unknown_relation_field_ambiguity_and_parameter_mismatch() {
    let cases = [
        (
            "query Q() -> many { select id from absent }",
            catalog(&[]),
            CompileDiagnosticCode::UnknownRelation,
        ),
        (
            "query Q() -> many { select missing from run }",
            catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
            CompileDiagnosticCode::UnknownField,
        ),
        (
            "query Q() -> many { select id from run, account }",
            catalog(&[
                table("run", &[column("id", PgType::BigInt, false)]),
                table("account", &[column("id", PgType::BigInt, false)]),
            ]),
            CompileDiagnosticCode::AmbiguousField,
        ),
        (
            "query Q(id: bigint) -> many { select id from run where id = :other }",
            catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
            CompileDiagnosticCode::UnknownParameter,
        ),
        (
            "query Q(id: bigint, unused: text) -> many { select id from run where id = :id }",
            catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
            CompileDiagnosticCode::UnusedParameter,
        ),
    ];

    for (source, catalog, expected) in cases {
        let source_id = SourceId::new(92);
        let file = DibsParser::new().parse_strict(source_id, source).unwrap();
        let diagnostics = resolve_file(source_id, file, &catalog).unwrap_err();
        assert_eq!(diagnostics[0].code, expected, "{source}");
        assert_eq!(diagnostics[0].span.source_id, source_id);
    }
}

fn catalog(tables: &[Table]) -> CatalogSnapshot {
    let schema = Schema {
        tables: tables
            .iter()
            .cloned()
            .map(|table| (table.name.clone(), table))
            .collect::<IndexMap<_, _>>(),
    };
    CatalogSnapshot::from_schema_postgres_18(&schema).unwrap()
}

fn table(name: &str, columns: &[Column]) -> Table {
    Table {
        name: name.to_string(),
        columns: columns.to_vec(),
        check_constraints: Vec::new(),
        trigger_checks: Vec::new(),
        foreign_keys: Vec::new(),
        indices: Vec::new(),
        source: SourceLocation::default(),
        doc: None,
        icon: None,
    }
}

fn column(name: &str, pg_type: PgType, nullable: bool) -> Column {
    Column {
        name: name.to_string(),
        pg_type,
        rust_type: Some(pg_type.to_rust_type().to_string()),
        nullable,
        default: None,
        primary_key: name == "id",
        unique: false,
        auto_generated: false,
        long: false,
        label: false,
        enum_variants: Vec::new(),
        doc: None,
        lang: None,
        icon: None,
        subtype: None,
    }
}
