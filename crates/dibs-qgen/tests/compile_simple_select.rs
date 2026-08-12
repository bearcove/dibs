use dibs_db_schema::{Column, PgType, Schema, SourceLocation, Table};
use dibs_pg_catalog::CatalogSnapshot;
use dibs_qgen::{
    CompileDiagnosticCode, compile_query_source, generate_compiled_rust, render_compiled_sql,
};
use dibs_query_ir::{
    CompiledQuery, HirExpression, HirExpressionKind, HirRelationKind, HirStatementKind, RelationId,
};
use dibs_query_syntax::SourceId;
use indexmap::IndexMap;
use std::sync::LazyLock;

use dibs_query_syntax::DibsParser;

static PARSER: LazyLock<DibsParser> = LazyLock::new(DibsParser::new);

fn parser() -> &'static DibsParser {
    &PARSER
}

#[test]
fn parsed_simple_select_reaches_checked_artifact_and_existing_backends() {
    let source = r#"query FindRun(id: bigint, owner: text?) -> many {
    select run.id as run_id, owner
    from run as run
    where run.id = :id and owner = :owner
    order by run_id desc, run.owner
    limit 10 offset 2
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(7),
        source,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, true),
            ],
        )]),
    )
    .expect("representative simple SELECT compiles");

    assert_eq!(compiled.len(), 1);
    let query = compiled[0].validate().expect("artifact is checked");
    assert_eq!(query.query_name, "FindRun");
    assert_eq!(query.ordered_parameters.len(), 2);
    assert_eq!(query.ordered_parameters[0].source_name, "id");
    assert_eq!(query.ordered_parameters[1].source_name, "owner");
    assert_eq!(query.ordered_output_fields.len(), 2);
    assert_eq!(query.ordered_output_fields[0].sql_label, "run_id");
    assert_eq!(query.ordered_output_fields[1].sql_label, "owner");

    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT HIR")
    };
    assert_eq!(select.from.len(), 1);
    assert_eq!(select.from[0].id, RelationId::new(1));
    assert_eq!(select.from[0].alias.as_ref().unwrap().name, "run");
    assert!(matches!(
        select.projections[0].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding == RelationId::new(1)
    ));
    assert!(select.predicate.is_some());
    assert_eq!(select.order_by.len(), 2);
    assert!(select.limit.is_some());
    assert!(select.offset.is_some());
    assert!(query.query_origin.primary.is_some());
    assert!(select.projections.iter().all(|projection| {
        projection.expression.origin.primary.is_some() && projection.alias_origin.primary.is_some()
    }));

    let rendered = render_compiled_sql(query).expect("existing SQL backend renders artifact");
    assert_eq!(rendered.ordered_binds.len(), 2);
    assert!(rendered.sql.contains("FROM \"public\".\"run\" AS \"run\""));
    assert!(rendered.sql.contains("WHERE"));
    assert!(rendered.sql.contains("ORDER BY"));
    assert!(rendered.sql.contains("LIMIT 10 OFFSET 2"));

    let generated = generate_compiled_rust(query).expect("existing Rust backend renders artifact");
    assert!(generated.source.contains("pub async fn find_run"));
    assert!(generated.source.contains("pub struct FindRunResult"));
    assert!(generated.source.contains("id: &i64"));
    assert!(generated.source.contains("owner: &Option<String>"));
}

#[test]
fn inner_join_on_compiles_through_recursive_relation_hir() {
    let source = r#"query JoinRuns() -> many {
    select run.id as run_id, account.name as account_name
    from run inner join account on run.account_id = account.id
}"#;
    let compiled = compile_query_source(
        parser(),
        SourceId::new(22),
        source,
        &catalog(&[
            table(
                "run",
                &[
                    column("id", PgType::BigInt, false),
                    column("account_id", PgType::BigInt, false),
                ],
            ),
            table(
                "account",
                &[
                    column("id", PgType::BigInt, false),
                    column("name", PgType::Text, false),
                ],
            ),
        ]),
    )
    .expect("INNER JOIN ... ON compiles");

    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT HIR")
    };
    assert_eq!(select.from.len(), 1);
    let HirRelationKind::Join {
        kind,
        left,
        right,
        predicate: Some(predicate),
        lateral: false,
    } = &select.from[0].kind
    else {
        panic!("expected recursive INNER JOIN HIR")
    };
    assert_eq!(*kind, dibs_query_ir::JoinKind::Inner);
    assert!(matches!(left.kind, HirRelationKind::Table { .. }));
    assert!(matches!(right.kind, HirRelationKind::Table { .. }));
    assert!(matches!(predicate.kind, HirExpressionKind::Operator { .. }));
    assert!(matches!(
        select.projections[0].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding == left.id
    ));
    assert!(matches!(
        select.projections[1].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding == right.id
    ));
    assert_eq!(
        query
            .lineage
            .catalog_columns_for_field(query.ordered_output_fields[0].id)
            .len(),
        1
    );

    let rendered = render_compiled_sql(query).expect("join SQL renders");
    assert!(rendered.sql.contains("INNER JOIN"));
    assert!(rendered.sql.contains(" ON "));
}

#[test]
fn outer_and_cross_join_kinds_preserve_typed_null_extension() {
    let source = r#"query LeftJoin() -> many {
    select account.name as account_name
    from run left join account on run.account_id = account.id
}
query RightJoin() -> many {
    select run.id as run_id
    from run right join account on run.account_id = account.id
}
query FullJoin() -> many {
    select run.id as run_id, account.name as account_name
    from run full join account on run.account_id = account.id
}
query CrossJoin() -> many {
    select run.id as run_id
    from run cross join account
}"#;
    let compiled = compile_query_source(
        parser(),
        SourceId::new(23),
        source,
        &catalog(&[
            table(
                "run",
                &[
                    column("id", PgType::BigInt, false),
                    column("account_id", PgType::BigInt, false),
                ],
            ),
            table(
                "account",
                &[
                    column("id", PgType::BigInt, false),
                    column("name", PgType::Text, false),
                ],
            ),
        ]),
    )
    .expect("outer and cross joins compile");

    let expected = [
        (dibs_query_ir::JoinKind::Left, vec![true]),
        (dibs_query_ir::JoinKind::Right, vec![true]),
        (dibs_query_ir::JoinKind::Full, vec![true, true]),
        (dibs_query_ir::JoinKind::Cross, vec![false]),
    ];
    for (query, (expected_kind, expected_nullability)) in compiled.iter().zip(expected) {
        let query = query.validate().expect("artifact is checked");
        let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
            panic!("expected typed SELECT")
        };
        let dibs_query_ir::TypedRelationKind::Join {
            kind, predicate, ..
        } = &select.from[0].kind
        else {
            panic!("expected typed join")
        };
        assert_eq!(*kind, expected_kind);
        assert_eq!(
            predicate.is_some(),
            expected_kind != dibs_query_ir::JoinKind::Cross
        );
        assert_eq!(
            select
                .projections
                .iter()
                .map(|projection| projection.output_nullability().is_nullable())
                .collect::<Vec<_>>(),
            expected_nullability
        );
    }
}

#[test]
fn group_by_and_having_compile_through_aggregate_checker() {
    let source = r#"query OwnerCounts() -> many {
    select owner, count(*) as run_count
    from run
    group by owner
    having count(*) > 1
}"#;
    let compiled = compile_query_source(
        parser(),
        SourceId::new(24),
        source,
        &catalog(&[table("run", &[column("owner", PgType::Text, false)])]),
    )
    .expect("GROUP BY and HAVING compile");

    let query = compiled[0].validate().expect("artifact is checked");
    let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
        panic!("expected typed SELECT")
    };
    assert_eq!(select.group_by.len(), 1);
    assert!(select.having.is_some());
    assert!(matches!(
        select.projections[1].expression.kind,
        dibs_query_ir::TypedExpressionKind::Call(_)
    ));
    let rendered = render_compiled_sql(query).expect("grouped SQL renders");
    assert!(rendered.sql.contains(" GROUP BY "));
    assert!(rendered.sql.contains(" HAVING "));
}

#[test]
fn grouped_query_rejects_ungrouped_projection() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(25),
        r#"query InvalidGrouping() -> many {
    select owner, state, count(*) as run_count
    from run
    group by owner
}"#,
        &catalog(&[table(
            "run",
            &[
                column("owner", PgType::Text, false),
                column("state", PgType::Text, false),
            ],
        )]),
    )
    .expect_err("ungrouped state projection must be rejected");
    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::TypeMismatch);
    assert!(diagnostics[0].message.contains("ungrouped"));
}

#[test]
fn grouped_expression_matches_structurally_equivalent_projection() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(46),
        r#"query BucketCounts() -> many {
    select id + 1 as bucket, count(*) as run_count
    from run
    group by id + 1
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("equivalent grouped expression compiles");
    assert_eq!(compiled.len(), 1);
}

#[test]
fn aggregate_call_is_rejected_in_where() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(47),
        r#"query InvalidAggregatePhase() -> many {
    select id
    from run
    where count(*) > 0
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect_err("aggregate call in WHERE must be rejected");
    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::TypeMismatch);
    assert!(diagnostics[0].message.contains("WHERE"));
    assert!(diagnostics[0].message.contains("aggregate"));
}

#[test]
fn distinct_on_requires_matching_order_prefix() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(26),
        r#"query LatestByOwner() -> many {
    select distinct on (owner) owner, id
    from run
    order by id desc, owner
}"#,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
            ],
        )]),
    )
    .expect_err("DISTINCT ON must match the leading ORDER BY expressions");
    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::TypeMismatch);
    assert!(diagnostics[0].message.contains("DISTINCT ON"));
}

#[test]
fn distinct_on_accepts_reordered_leading_key_set() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(27),
        r#"query LatestByOwnerAndState() -> many {
    select distinct on (owner, state) owner, state, id
    from run
    order by state, owner, id desc
}"#,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
                column("state", PgType::Text, false),
            ],
        )]),
    )
    .expect("leading DISTINCT ON keys may be reordered");
    assert_eq!(compiled.len(), 1);
}

#[test]
fn inline_window_application_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(28),
        r#"query RankedRuns() -> many {
    select row_number() over (
        partition by owner
        order by id
        rows between unbounded preceding and current row
    ) as rank
    from run
}"#,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
            ],
        )]),
    )
    .expect("inline window application compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
        panic!("expected typed SELECT")
    };
    let dibs_query_ir::TypedExpressionKind::Call(call) = &select.projections[0].expression.kind
    else {
        panic!("expected window call")
    };
    let Some(dibs_query_ir::WindowReference::Inline(window)) = &call.over else {
        panic!("expected inline window")
    };
    assert_eq!(window.partition_by.len(), 1);
    assert_eq!(window.order_by.len(), 1);
    assert!(window.frame.is_some());
    let rendered = render_compiled_sql(query).expect("window SQL renders");
    assert!(rendered.sql.contains("OVER (PARTITION BY"));
    assert!(
        rendered
            .sql
            .contains("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW")
    );
}

#[test]
fn set_operations_compile_and_render() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(49),
        r#"query CombinedRuns() -> many {
    select id from run where id < 3
    union all
    select id from run where id > 1
    order by id
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("set query compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
        panic!("expected typed SELECT")
    };
    assert!(matches!(
        select.from[0].kind,
        dibs_query_ir::TypedRelationKind::SetOperation {
            kind: dibs_query_ir::SetOperationKind::Union,
            all: true,
            ..
        }
    ));
    let rendered = render_compiled_sql(query).expect("set SQL renders");
    assert!(rendered.sql.contains("UNION ALL"));
}

#[test]
fn named_window_definition_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(29),
        r#"query RankedRuns() -> many {
    select row_number() over ranked as rank
    from run
    window ranked as (partition by owner order by id)
}"#,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
            ],
        )]),
    )
    .expect("named window compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
        panic!("expected typed SELECT")
    };
    assert_eq!(select.windows.len(), 1);
    assert_eq!(select.windows[0].name, "ranked");
    let dibs_query_ir::TypedExpressionKind::Call(call) = &select.projections[0].expression.kind
    else {
        panic!("expected window call")
    };
    assert!(matches!(
        &call.over,
        Some(dibs_query_ir::WindowReference::Named(name)) if name == "ranked"
    ));
    let rendered = render_compiled_sql(query).expect("named window SQL renders");
    assert!(rendered.sql.contains("OVER \"ranked\""));
    assert!(rendered.sql.contains(" WINDOW \"ranked\" AS ("));
}

#[test]
fn case_and_array_expressions_compile_and_render() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(50),
        r#"query Expressions() -> many {
    select
        case when id > 1 then id else 0 end as classified,
        array[id, id + 1] as neighbors
    from run
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("structural expressions compile");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("expression SQL renders");
    assert!(rendered.sql.contains("CASE WHEN"));
    assert!(rendered.sql.contains("ARRAY["));
}

#[test]
#[ignore = "production-shaped parse remains above the compiler latency budget"]
fn trials_ci_attempt_line_bounds_compiles() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(51),
        r#"query CiAttemptLineBounds(attempt_id: text) -> many {
    select min(line.line_number) as first_line, max(line.line_number) as last_line
    from fleet_attempt as attempt
    left join fleet_log_line as line on line.fleet_attempt_id = attempt.id
    where attempt.attempt_id = :attempt_id
    group by attempt.id
}"#,
        &catalog(&[
            table(
                "fleet_attempt",
                &[
                    column("id", PgType::BigInt, false),
                    column("attempt_id", PgType::Text, false),
                ],
            ),
            table(
                "fleet_log_line",
                &[
                    column("fleet_attempt_id", PgType::BigInt, false),
                    column("line_number", PgType::BigInt, false),
                ],
            ),
        ]),
    )
    .expect("Trials line-bounds query compiles");
    let query = compiled[0]
        .validate()
        .expect("Trials-shaped artifact validates");
    let rendered = render_compiled_sql(query).expect("Trials-shaped SQL renders");
    assert!(rendered.sql.contains("LEFT JOIN"));
    assert!(rendered.sql.contains("GROUP BY"));
    assert!(rendered.sql.contains("pg_catalog\".\"min"));
    assert!(rendered.sql.contains("pg_catalog\".\"max"));
}

#[test]
fn derived_relation_outputs_bind_by_alias() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(30),
        r#"query DerivedRuns() -> many {
    select recent.run_id
    from (
        select id as run_id
        from run
    ) as recent
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("derived relation output compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT HIR")
    };
    assert!(matches!(select.from[0].kind, HirRelationKind::Subquery(_)));
    assert!(matches!(
        select.projections[0].expression.kind,
        HirExpressionKind::DerivedColumn { binding, field_id }
            if binding == select.from[0].id && field_id.get() == 1
    ));
    assert_eq!(
        query.ordered_output_fields[0].type_id.as_str(),
        "pg18:type:base:pg_catalog.bigint"
    );
    let rendered = render_compiled_sql(query).expect("derived SQL renders");
    assert!(rendered.sql.contains("FROM (SELECT"));
    assert!(rendered.sql.contains("AS \"recent\""));
}

#[test]
fn derived_relation_preserves_projected_volatility() {
    let mut catalog = catalog(&[]);
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .expect("bigint type")
        .id
        .clone();
    catalog
        .register_scalar(
            dibs_pg_catalog::ScalarSignature {
                qualified_name: "app.next_value".to_string(),
                arguments: Vec::new(),
                result: bigint,
            },
            dibs_pg_catalog::ScalarCallableFacts {
                volatility: dibs_pg_catalog::Volatility::Volatile,
                strict: false,
                result_nullability: dibs_pg_catalog::Nullability::NotNull,
            },
        )
        .expect("volatile scalar registration");
    let compiled = compile_query_source(
        parser(),
        SourceId::new(31),
        r#"query DerivedVolatile() -> many {
    select generated.value
    from (
        select app.next_value() as value
    ) as generated
}"#,
        &catalog,
    )
    .expect("volatile derived relation compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
        panic!("expected typed SELECT")
    };
    assert_eq!(
        select.projections[0].expression.volatility,
        dibs_query_ir::Volatility::Volatile
    );
}

#[test]
fn correlated_scalar_subquery_resolves_outer_binding() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(32),
        r#"query CorrelatedRun() -> many {
    select (select source.id as value limit 1) as copied_id
    from run as source
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("correlated scalar subquery compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected outer SELECT")
    };
    let HirExpressionKind::ScalarSubquery(statement) = &select.projections[0].expression.kind
    else {
        panic!("expected scalar subquery")
    };
    let HirStatementKind::Select(nested) = &statement.kind else {
        panic!("expected nested SELECT")
    };
    assert!(matches!(
        nested.projections[0].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding == select.from[0].id
    ));
    let rendered = render_compiled_sql(query).expect("correlated scalar SQL renders");
    assert!(
        rendered
            .sql
            .contains("(SELECT \"source\".\"id\" AS \"value\" LIMIT 1)")
    );
    let outer_column = match &nested.projections[0].expression.kind {
        HirExpressionKind::Column { column_id, .. } => column_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(
        query
            .resolved_references
            .references_to(&dibs_query_ir::ReferenceTarget::Column(
                outer_column.clone()
            ))
            .len(),
        1
    );
    assert_eq!(
        query
            .lineage
            .catalog_columns_for_field(query.ordered_output_fields[0].id),
        vec![outer_column]
    );
}

#[test]
fn non_lateral_derived_relation_cannot_see_preceding_input() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(33),
        r#"query InvalidCorrelation() -> many {
    select recent.run_id
    from run as source, (
        select source.id as run_id
    ) as recent
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect_err("non-LATERAL derived relation must not see preceding input");
    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::UnknownRelation);
    assert!(diagnostics[0].message.contains("source"));
}

#[test]
fn lateral_derived_relation_sees_preceding_input() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(34),
        r#"query LateralRun() -> many {
    select recent.run_id
    from run as source, lateral (
        select source.id as run_id
    ) as recent
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("LATERAL derived relation compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT")
    };
    let HirRelationKind::Join {
        lateral,
        left,
        right,
        ..
    } = &select.from[0].kind
    else {
        panic!("expected normalized CROSS JOIN LATERAL")
    };
    assert!(*lateral);
    let HirRelationKind::Subquery(statement) = &right.kind else {
        panic!("expected lateral subquery relation")
    };
    let HirStatementKind::Select(nested) = &statement.kind else {
        panic!("expected nested SELECT")
    };
    assert!(matches!(
        nested.projections[0].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding == left.id
    ));
    let rendered = render_compiled_sql(query).expect("LATERAL SQL renders");
    assert!(
        rendered
            .sql
            .contains("LATERAL (SELECT \"source\".\"id\" AS \"run_id\")")
    );
    assert_eq!(
        query
            .lineage
            .catalog_columns_for_field(query.ordered_output_fields[0].id)
            .len(),
        1
    );
}

#[test]
fn cross_join_lateral_marks_join_and_sees_left_input() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(35),
        r#"query JoinedLateralRun() -> many {
    select recent.run_id
    from run as source cross join lateral (
        select source.id as run_id
    ) as recent
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("CROSS JOIN LATERAL compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT")
    };
    let HirRelationKind::Join {
        lateral,
        left,
        right,
        ..
    } = &select.from[0].kind
    else {
        panic!("expected join")
    };
    assert!(*lateral);
    let HirRelationKind::Subquery(statement) = &right.kind else {
        panic!("expected lateral right subquery")
    };
    let HirStatementKind::Select(nested) = &statement.kind else {
        panic!("expected nested SELECT")
    };
    assert!(matches!(
        nested.projections[0].expression.kind,
        HirExpressionKind::Column { binding, .. } if binding == left.id
    ));
}

#[test]
fn left_join_lateral_null_extends_derived_output() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(36),
        r#"query OptionalLateralRun() -> many {
    select recent.run_id
    from run as source left join lateral (
        select source.id as run_id
    ) as recent on true
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("LEFT JOIN LATERAL compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let dibs_query_ir::TypedStatementKind::Select(select) = &query.typed_statement.kind else {
        panic!("expected typed SELECT")
    };
    let dibs_query_ir::TypedRelationKind::Join {
        kind,
        lateral,
        right,
        ..
    } = &select.from[0].kind
    else {
        panic!("expected lateral join")
    };
    assert_eq!(*kind, dibs_query_ir::JoinKind::Left);
    assert!(*lateral);
    assert!(select.projections[0].output_nullability().is_nullable());
    assert!(matches!(
        right.kind,
        dibs_query_ir::TypedRelationKind::Subquery(_)
    ));
}

#[test]
fn lateral_output_preserves_volatility_references_and_lineage() {
    let mut catalog = catalog(&[table("run", &[column("id", PgType::BigInt, false)])]);
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .expect("bigint type")
        .id
        .clone();
    catalog
        .register_scalar(
            dibs_pg_catalog::ScalarSignature {
                qualified_name: "app.next_value".to_string(),
                arguments: vec![bigint.clone()],
                result: bigint,
            },
            dibs_pg_catalog::ScalarCallableFacts {
                volatility: dibs_pg_catalog::Volatility::Volatile,
                strict: true,
                result_nullability: dibs_pg_catalog::Nullability::NotNull,
            },
        )
        .expect("volatile scalar registration");
    let compiled = compile_query_source(
        parser(),
        SourceId::new(37),
        r#"query VolatileLateralRun() -> many {
    select generated.value
    from run as source cross join lateral (
        select app.next_value(source.id) as value
    ) as generated
}"#,
        &catalog,
    )
    .expect("volatile LATERAL query compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    assert_eq!(
        query.read_write_lock_manifest.volatility,
        dibs_query_ir::Volatility::Volatile
    );
    assert_eq!(
        query
            .resolved_references
            .references
            .iter()
            .filter(|reference| matches!(
                reference.target,
                dibs_query_ir::ReferenceTarget::Callable(_)
            ))
            .count(),
        1
    );
    assert_eq!(
        query
            .lineage
            .catalog_columns_for_field(query.ordered_output_fields[0].id)
            .len(),
        1
    );
}
#[test]
fn resolved_operator_references_use_checked_catalog_identity() {
    let source = r#"query FindRun(id: bigint) -> many {
    select id
    from run
    where id = :id
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(14),
        source,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("operator-bearing SELECT compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(hir_select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT HIR")
    };
    let HirExpressionKind::Operator {
        operator_id: hir_authored,
        ..
    } = &hir_select.predicate.as_ref().expect("predicate").kind
    else {
        panic!("expected HIR operator")
    };
    let dibs_query_ir::TypedStatementKind::Select(typed_select) = &query.typed_statement.kind
    else {
        panic!("expected typed SELECT")
    };
    let dibs_query_ir::TypedExpressionKind::Operator {
        authored_operator_id,
        operator_id: resolved_operator_id,
        ..
    } = &typed_select
        .predicate
        .as_ref()
        .expect("typed predicate")
        .kind
    else {
        panic!("expected typed operator")
    };
    assert_eq!(authored_operator_id, hir_authored);
    assert!(authored_operator_id.as_str().starts_with("unresolved:"));
    assert!(
        resolved_operator_id
            .as_str()
            .starts_with("pg18:operator:pg_catalog.=")
    );

    let operator_targets = query
        .resolved_references
        .references
        .iter()
        .filter_map(|reference| match &reference.target {
            dibs_query_ir::ReferenceTarget::Operator(operator_id) => Some(operator_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(operator_targets, vec![resolved_operator_id]);
}

#[test]
fn scalar_and_aggregate_calls_resolve_through_semantic_checker() {
    let source = r#"query Normalize() -> many {
    select lower(owner) as normalized_owner from run
}
query CountRuns() -> one {
    select count(*) as run_count from run
}"#;
    let compiled = compile_query_source(
        parser(),
        SourceId::new(15),
        source,
        &catalog(&[table("run", &[column("owner", PgType::Text, false)])]),
    )
    .expect("scalar and aggregate calls compile");
    assert_eq!(compiled.len(), 2);
    for query in &compiled {
        let query = query.validate().expect("artifact is checked");
        let HirStatementKind::Select(hir_select) = &query.resolved_hir.statement.kind else {
            panic!("expected SELECT HIR")
        };
        let dibs_query_ir::TypedStatementKind::Select(typed_select) = &query.typed_statement.kind
        else {
            panic!("expected typed SELECT")
        };
        for (hir_projection, typed_projection) in
            hir_select.projections.iter().zip(&typed_select.projections)
        {
            let HirExpressionKind::Call(hir_call) = &hir_projection.expression.kind else {
                panic!("expected HIR call")
            };
            let dibs_query_ir::TypedExpressionKind::Call(typed_call) =
                &typed_projection.expression.kind
            else {
                panic!("expected typed call")
            };
            assert_eq!(typed_call.authored_callable_id, hir_call.callable_id);
            assert!(hir_call.callable_id.as_str().starts_with("unresolved:"));
            assert!(
                typed_call
                    .callable_id
                    .as_str()
                    .starts_with("pg18:callable:")
            );
        }

        let callable_targets = query
            .resolved_references
            .references
            .iter()
            .filter_map(|reference| match &reference.target {
                dibs_query_ir::ReferenceTarget::Callable(callable_id) => Some(callable_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(callable_targets.len(), 1);
        assert!(callable_targets[0].as_str().starts_with("pg18:callable:"));
        if query.resolved_hir.name == "Normalize" {
            assert_eq!(query.declared_result_mode, dibs_query_ir::ResultMode::Many);
            assert_eq!(
                query
                    .lineage
                    .catalog_columns_for_field(query.ordered_output_fields[0].id)
                    .len(),
                1
            );
        } else {
            assert_eq!(query.resolved_hir.name, "CountRuns");
            assert_eq!(query.declared_result_mode, dibs_query_ir::ResultMode::One);
            assert!(
                query
                    .lineage
                    .catalog_columns_for_field(query.ordered_output_fields[0].id)
                    .is_empty()
            );
        }
    }
}

#[test]
fn aliases_qualified_and_unqualified_columns_resolve_to_stable_bindings() {
    let source = r#"query Pair(id: bigint) -> many {
    select r.id as run_id, account.name as account_name
    from run as r, account
    where r.account_id = account.id and r.id = :id
    order by account_name asc
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(8),
        source,
        &catalog(&[
            table(
                "run",
                &[
                    column("id", PgType::BigInt, false),
                    column("account_id", PgType::BigInt, false),
                ],
            ),
            table(
                "account",
                &[
                    column("id", PgType::BigInt, false),
                    column("name", PgType::Text, false),
                ],
            ),
        ]),
    )
    .expect("qualified join-like SELECT compiles");

    let query = compiled[0].validate().unwrap();
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT HIR")
    };
    assert_eq!(select.from[0].id, RelationId::new(1));
    assert_eq!(select.from[1].id, RelationId::new(2));
    assert_eq!(select.projections[0].field_id.get(), 1);
    assert_eq!(select.projections[1].field_id.get(), 2);
    assert_eq!(select.order_by.len(), 1);
}

#[test]
fn unknown_relation_is_structured() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(9),
        "query Missing() -> many { select id from absent }",
        &catalog(&[]),
    )
    .unwrap_err();

    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::UnknownRelation);
    assert!(diagnostics[0].message.contains("absent"));
    assert!(diagnostics[0].span.span.start < diagnostics[0].span.span.end);
}

#[test]
fn unknown_field_is_structured() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(10),
        "query Missing() -> many { select missing from run }",
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .unwrap_err();

    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::UnknownField);
    assert!(diagnostics[0].message.contains("missing"));
}

#[test]
fn ambiguous_unqualified_field_is_structured() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(11),
        "query Ambiguous() -> many { select id from run, account }",
        &catalog(&[
            table("run", &[column("id", PgType::BigInt, false)]),
            table("account", &[column("id", PgType::BigInt, false)]),
        ]),
    )
    .unwrap_err();

    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::AmbiguousField);
    assert!(diagnostics[0].message.contains("id"));
}

#[test]
fn parameter_mismatch_is_structured() {
    let undeclared = compile_query_source(
        parser(),
        SourceId::new(12),
        "query Bad(id: bigint) -> many { select id from run where id = :other }",
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .unwrap_err();
    assert_eq!(undeclared[0].code, CompileDiagnosticCode::UnknownParameter);

    let unused = compile_query_source(
        parser(),
        SourceId::new(13),
        "query Bad(id: bigint, unused: text) -> many { select id from run where id = :id }",
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .unwrap_err();
    assert_eq!(unused[0].code, CompileDiagnosticCode::UnusedParameter);
}

#[test]
fn boolean_and_binds_looser_than_comparison() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(16),
        "query Filter(id: bigint, owner: text) -> many { select id from run where id = :id and owner = :owner }",
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
            ],
        )]),
    )
    .expect("AND must combine two comparison predicates");
    let query = compiled[0].validate().expect("artifact is checked");
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT HIR")
    };
    let HirExpressionKind::Operator {
        operator_id,
        operands,
    } = &select.predicate.as_ref().expect("predicate").kind
    else {
        panic!("expected predicate operator")
    };
    assert_eq!(
        operator_id.as_str(),
        dibs_query_typing::SYNTAX_AND_OPERATOR_ID
    );
    assert!(
        operands
            .iter()
            .all(|operand| matches!(operand.kind, HirExpressionKind::Operator { .. }))
    );
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

#[test]
fn or_binds_looser_than_and() {
    let query = compile_predicate(
        "id = :id or owner = :owner and id = :id",
        "query Filter(id: bigint, owner: text) -> many",
    );
    assert_operator(
        select_predicate(&query),
        dibs_query_typing::SYNTAX_OR_OPERATOR_ID,
        |operands| {
            assert_comparison(&operands[0]);
            assert_operator(
                &operands[1],
                dibs_query_typing::SYNTAX_AND_OPERATOR_ID,
                |operands| {
                    assert!(
                        operands.iter().all(|operand| matches!(
                            operand.kind,
                            HirExpressionKind::Operator { .. }
                        ))
                    );
                },
            );
        },
    );
}

#[test]
fn not_binds_looser_than_comparison() {
    let query = compile_predicate("not id = :id", "query Filter(id: bigint) -> many");
    assert_operator(
        select_predicate(&query),
        dibs_query_typing::SYNTAX_NOT_OPERATOR_ID,
        |operands| {
            assert_comparison(&operands[0]);
        },
    );
}

#[test]
fn parenthesized_comparisons_are_preserved() {
    let query = compile_projection("(id = id) = (id = id)");
    assert_operator(
        select_projection(&query),
        "unresolved:operator:pg_catalog.=",
        |operands| {
            assert!(
                operands
                    .iter()
                    .all(|operand| matches!(operand.kind, HirExpressionKind::Operator { .. }))
            );
        },
    );
}

fn compile_predicate(predicate: &str, declaration: &str) -> CompiledQuery {
    let source = format!("{declaration} {{ select id from run where {predicate} }}");
    compile_query_source(
        parser(),
        SourceId::new(20),
        &source,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
            ],
        )]),
    )
    .expect("predicate compiles")
    .remove(0)
}

fn compile_projection(expression: &str) -> CompiledQuery {
    let source = format!("query Project() -> many {{ select {expression} as value from run }}");
    compile_query_source(
        parser(),
        SourceId::new(21),
        &source,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("name", PgType::Text, false),
                column("suffix", PgType::Text, false),
            ],
        )]),
    )
    .expect("projection compiles")
    .remove(0)
}

fn select_predicate(query: &CompiledQuery) -> &HirExpression {
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT")
    };
    select.predicate.as_ref().expect("predicate")
}

fn select_projection(query: &CompiledQuery) -> &HirExpression {
    let HirStatementKind::Select(select) = &query.resolved_hir.statement.kind else {
        panic!("expected SELECT")
    };
    &select.projections[0].expression
}

fn assert_comparison(expression: &HirExpression) {
    let HirExpressionKind::Operator { operator_id, .. } = &expression.kind else {
        panic!("expected operator")
    };
    assert!(
        operator_id
            .as_str()
            .starts_with("unresolved:operator:pg_catalog.")
    );
}

fn assert_operator(
    expression: &HirExpression,
    expected: &str,
    inspect: impl FnOnce(&[HirExpression]),
) {
    let HirExpressionKind::Operator {
        operator_id,
        operands,
    } = &expression.kind
    else {
        panic!("expected operator")
    };
    assert_eq!(operator_id.as_str(), expected);
    inspect(operands);
}
