use dibs_db_schema::{Column, PgType, Schema, SourceLocation, Table};
use dibs_pg_catalog::CatalogSnapshot;
use dibs_qgen::{
    CompileDiagnosticCode, compile_query_source, generate_compiled_rust, render_compiled_sql,
};
use dibs_query_ir::{
    CardinalityEvidence, CompiledQuery, HirExpression, HirExpressionKind, HirRelationKind,
    HirStatementKind, LowerBound, MutationManifest, ReferenceAccess, ReferenceRole,
    ReferenceTarget, RelationId, TypedExpressionKind, TypedStatementKind, UpperBound,
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
fn interval_literal_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(53),
        r#"query RetryDelay() -> one {
    select interval '5 minutes' as retry_delay
}"#,
        &catalog(&[]),
    )
    .expect("interval literal compiles");

    let query = compiled[0].validate().expect("artifact validates");
    let rendered = render_compiled_sql(query).expect("interval literal renders");
    assert!(
        rendered
            .sql
            .contains("INTERVAL '5 minutes' AS \"retry_delay\"")
    );
}
#[test]
fn named_defaulted_function_argument_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(75),
        r#"query RetryDelay(seconds: integer) -> one {
    select make_interval(secs => :seconds) as retry_delay
}"#,
        &catalog(&[]),
    )
    .expect("named defaulted function argument compiles");

    let query = compiled[0].validate().expect("artifact validates");
    let rendered = render_compiled_sql(query).expect("named function argument renders");
    assert!(
        rendered.sql.contains(
            "\"pg_catalog\".\"make_interval\"(secs => $1::\"pg_catalog\".\"double precision\")"
        ),
        "{}",
        rendered.sql
    );
}

#[test]
fn interval_multiplied_by_explicit_bigint_to_double_cast_compiles() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(61),
        r#"query RetryDelay(seconds: bigint) -> one {
    select interval '1 second' * :seconds::bigint::double precision as delay
}"#,
        &catalog(&[]),
    )
    .expect("interval scaling compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    assert!(
        query
            .deterministic_sql
            .contains("::\"pg_catalog\".\"double precision\"")
    );
}
#[test]
fn top_level_union_all_lowers_through_set_relation() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(62),
        r#"query Combined() -> many {
    select 1 as value
    union all
    select 2 as value
}"#,
        &catalog(&[]),
    )
    .expect("top-level UNION ALL compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("set operation renders");
    assert!(rendered.sql.contains("UNION ALL"));
}

#[test]
fn exists_predicate_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(66),
        r#"query HasRow() -> one {
    select not exists (select 1) as absent
}"#,
        &catalog(&[]),
    )
    .expect("EXISTS predicate compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("EXISTS predicate renders");
    assert!(
        rendered
            .sql
            .contains("NOT (EXISTS (SELECT 1 AS \"__dibs_exists_0\"))"),
        "{}",
        rendered.sql
    );
}

#[test]
fn quantified_array_comparison_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(65),
        r#"query NotPresent() -> one {
    select not 3 = any(array[1, 2]) as missing
}"#,
        &catalog(&[]),
    )
    .expect("quantified array comparison compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("quantified comparison renders");
    assert!(rendered.sql.contains("NOT (3 = ANY(ARRAY[1, 2]))"));
}

#[test]
fn array_element_constructor_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(64),
        r#"query Pair() -> one {
    select array[1, 2]::int4[] as values
}"#,
        &catalog(&[]),
    )
    .expect("array element constructor compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("array constructor renders");
    assert!(rendered.sql.contains("ARRAY[1, 2]"));
}

#[test]
fn array_concatenation_overloads_compile_and_render() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(68),
        r#"query ConcatenateArrays() -> one {
    select
        array[1, 2]::int4[] || array[3, 4]::int4[] as arrays,
        0 || array[1, 2]::int4[] as prepended,
        array['one']::text[] || 'two'::text as appended
}"#,
        &catalog(&[]),
    )
    .expect("PostgreSQL array concatenation overloads compile");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("array concatenation renders");
    assert!(
        rendered.sql.contains("ARRAY[1, 2]")
            && rendered.sql.contains("ARRAY[3, 4]")
            && rendered.sql.contains("AS \"arrays\"")
            && rendered.sql.contains("AS \"prepended\"")
            && rendered.sql.contains("AS \"appended\"")
            && rendered.sql.matches("||").count() == 3,
        "{}",
        rendered.sql
    );
}

#[test]
fn array_cardinality_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(69),
        r#"query ArrayCardinality() -> one {
    select cardinality(array['one', 'two']::text[]) as length
}"#,
        &catalog(&[]),
    )
    .expect("PostgreSQL cardinality(anyarray) compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("array cardinality renders");
    assert!(
        rendered.sql.contains("cardinality") && rendered.sql.contains("AS \"length\""),
        "{}",
        rendered.sql
    );
}

#[test]
fn recursive_cte_uses_anchor_schema_and_renders_union_all() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(63),
        r#"query CountToThree() -> many {
    with recursive path(n) as (
        select 1 as n
        union all
        select path.n + 1 as n
        from path
        where path.n < 3
    )
    select path.n as n
    from path
}"#,
        &catalog(&[]),
    )
    .expect("recursive CTE compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("recursive CTE renders");
    assert!(rendered.sql.contains("WITH RECURSIVE"));
    assert!(rendered.sql.contains("UNION ALL"));
}

#[test]
fn in_value_list_compiles_and_renders_with_sql_null_semantics() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(52),
        r#"query PendingRuns() -> many {
    select id
    from run
    where state in ('pending', 'failed')
}"#,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("state", PgType::Text, false),
            ],
        )]),
    )
    .expect("IN value list compiles");

    let query = compiled[0].validate().expect("artifact validates");
    let rendered = render_compiled_sql(query).expect("IN value list renders");
    assert!(
        rendered.sql.contains(
            "WHERE \"run\".\"state\" IN ('pending'::\"pg_catalog\".\"text\", 'failed'::\"pg_catalog\".\"text\")"
        ),
        "{}",
        rendered.sql
    );
}

#[test]
fn in_value_list_renders_volatile_left_operand_once() {
    let mut catalog = catalog(&[]);
    let numeric = catalog
        .resolve_type("pg_catalog.numeric")
        .expect("numeric type")
        .id
        .clone();
    catalog
        .register_scalar(
            dibs_pg_catalog::ScalarSignature {
                qualified_name: "app.random_value".to_string(),
                arguments: Vec::new(),
                result: numeric,
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
        SourceId::new(53),
        r#"query RandomBucket() -> many {
    select 1 as bucket
    where app.random_value() in (0.1, 0.2)
}"#,
        &catalog,
    )
    .expect("volatile IN value list compiles");

    let query = compiled[0].validate().expect("artifact validates");
    let rendered = render_compiled_sql(query).expect("volatile IN renders");
    assert_eq!(rendered.sql.matches("random_value").count(), 1);
    assert!(rendered.sql.contains("IN (0.1, 0.2)"));
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
fn distinct_on_accepts_matching_cte_column_order_prefix() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(70),
        r#"query LatestCteValue() -> many {
    with values as (
        select owner
        from run
    )
    select distinct on (owner) owner
    from values
    order by owner
}"#,
        &catalog(&[table("run", &[column("owner", PgType::Text, false)])]),
    )
    .expect("matching CTE DISTINCT ON and ORDER BY columns compile");
    assert_eq!(compiled.len(), 1);
}

#[test]
fn distinct_on_accepts_matching_derived_column_order_prefix() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(71),
        r#"query LatestDerivedValue() -> many {
    select distinct on (recent.owner) recent.owner
    from (
        select owner
        from run
    ) as recent
    order by recent.owner
}"#,
        &catalog(&[table("run", &[column("owner", PgType::Text, false)])]),
    )
    .expect("matching derived DISTINCT ON and ORDER BY columns compile");
    assert_eq!(compiled.len(), 1);
}

#[test]
fn text_comparison_operators_contextualize_unknown_literals() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(72),
        r#"query CompareText() -> one {
    select
        'a'::text <> 'b' as ne,
        'a'::text < 'b' as lt,
        'a'::text > 'b' as gt,
        'a'::text <= 'b' as lte,
        'a'::text >= 'b' as gte
}"#,
        &catalog(&[]),
    )
    .expect("PostgreSQL text comparison operators compile");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("text comparisons render");
    for operator in ["<>", "<", ">", "<=", ">="] {
        assert!(rendered.sql.contains(operator), "{}", rendered.sql);
    }
}

#[test]
fn coalesce_compiles_with_common_type_and_single_evaluation_rendering() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(73),
        r#"query AttemptStart() -> many {
    select coalesce(attempt.started_at, attempt.created_at) as started_at
    from attempt
}"#,
        &catalog(&[table(
            "attempt",
            &[
                column("started_at", PgType::Timestamptz, true),
                column("created_at", PgType::Timestamptz, false),
            ],
        )]),
    )
    .expect("COALESCE compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("COALESCE renders");
    assert_eq!(
        rendered.sql.matches("started_at").count(),
        2,
        "{}",
        rendered.sql
    );
    assert_eq!(
        rendered.sql.matches("created_at").count(),
        1,
        "{}",
        rendered.sql
    );
    assert!(rendered.sql.contains("COALESCE("), "{}", rendered.sql);
}
#[test]
fn nullif_preserves_special_form_typing_and_rendering() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(92),
        r#"query Normalize(outcome: text) -> one {
    select nullif(:outcome, '')::text::jsonb as outcome
}"#,
        &catalog(&[]),
    )
    .expect("NULLIF compiles as a PostgreSQL special form");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("NULLIF renders");
    assert!(
        rendered
            .sql
            .contains("NULLIF($1, ''::\"pg_catalog\".\"text\")"),
        "{}",
        rendered.sql
    );
    assert_eq!(rendered.sql.matches("$1").count(), 1, "{}", rendered.sql);
}

#[test]
fn least_and_greatest_mixed_nullability_are_not_null() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(76),
        r#"query Bounds() -> many {
    select least(attempt.optional_value, attempt.required_value) as least_value,
           greatest(attempt.optional_value, attempt.required_value) as greatest_value
    from attempt
}"#,
        &catalog(&[table(
            "attempt",
            &[
                column("optional_value", PgType::Integer, true),
                column("required_value", PgType::Integer, false),
            ],
        )]),
    )
    .expect("LEAST and GREATEST compile");
    let query = compiled[0].validate().expect("artifact is checked");
    assert!(
        query
            .ordered_output_fields
            .iter()
            .all(|field| !field.nullability.is_nullable())
    );
    let rendered = render_compiled_sql(query).expect("LEAST and GREATEST render");
    assert!(rendered.sql.contains("LEAST("), "{}", rendered.sql);
    assert!(rendered.sql.contains("GREATEST("), "{}", rendered.sql);
}

#[test]
fn registered_table_function_relation_compiles_with_column_aliases() {
    let mut catalog = catalog(&[]);
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let boolean = catalog
        .resolve_type("pg_catalog.boolean")
        .unwrap()
        .id
        .clone();
    catalog
        .register_table(
            dibs_pg_catalog::TableSignature {
                qualified_name: "public.retry_job".to_string(),
                arguments: vec![bigint.clone()],
                columns: vec![
                    dibs_pg_catalog::TableOutputColumn {
                        name: "intent_id".to_string(),
                        type_id: bigint,
                        nullability: dibs_pg_catalog::Nullability::NotNull,
                    },
                    dibs_pg_catalog::TableOutputColumn {
                        name: "coalesced".to_string(),
                        type_id: boolean,
                        nullability: dibs_pg_catalog::Nullability::NotNull,
                    },
                ],
            },
            dibs_pg_catalog::TableCallableFacts {
                volatility: dibs_pg_catalog::Volatility::Volatile,
                strict: true,
                cardinality: dibs_pg_catalog::CallableCardinality::SetOfUnknown,
            },
        )
        .unwrap();
    let compiled = compile_query_source(
        parser(),
        SourceId::new(74),
        r#"query RetryJob(id: bigint) -> many {
    select retry.intent, retry.was_coalesced
    from retry_job(:id) as retry(intent, was_coalesced)
}"#,
        &catalog,
    )
    .expect("registered table function relation compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("table function renders");
    assert!(rendered.sql.contains("retry_job"), "{}", rendered.sql);
    assert!(
        rendered
            .sql
            .contains("AS \"retry\" (\"intent\", \"was_coalesced\")"),
        "{}",
        rendered.sql
    );
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
fn trials_ci_attempt_line_bounds_compiles() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(51),
        r#"query CiAttemptLineBounds(attempt_id: text) -> optional {
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
                    unique_column("attempt_id", PgType::Text, false),
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
    assert_eq!(
        query.typed_statement.cardinality.upper(),
        dibs_query_ir::UpperBound::One
    );
    let rendered = render_compiled_sql(query).expect("Trials-shaped SQL renders");
    assert!(rendered.sql.contains("LEFT JOIN"));
    assert!(rendered.sql.contains("GROUP BY"));
    assert!(rendered.sql.contains("pg_catalog\".\"min"));
    assert!(rendered.sql.contains("pg_catalog\".\"max"));
}

#[test]
fn grouped_join_without_unique_predicate_remains_many() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(98),
        r#"query CiAttemptLineBounds() -> optional {
    select min(line.line_number) as first_line, max(line.line_number) as last_line
    from fleet_attempt as attempt
    left join fleet_log_line as line on line.fleet_attempt_id = attempt.id
    group by attempt.id
}"#,
        &catalog(&[
            table(
                "fleet_attempt",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("attempt_id", PgType::Text, false),
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
    .expect_err("unconstrained unique group key cannot prove one group");
    assert_eq!(
        diagnostics[0].code,
        CompileDiagnosticCode::ResultModeMismatch
    );
}

#[test]
fn scalar_subquery_output_does_not_require_an_alias() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(67),
        r#"query ScalarValue() -> one {
    select (select count(*) from run) as run_count
}"#,
        &catalog(&[table("run", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("unaliased scalar subquery output compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("scalar subquery renders");
    assert!(rendered.sql.contains("AS \"__dibs_scalar_0\""));
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
fn unique_key_join_preserves_optional_cardinality() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(77),
        r#"query FindCapability(token: bytea) -> optional {
    select binding.id
    from capability
    join binding on binding.id = capability.binding_id
    where capability.token = :token
}"#,
        &catalog(&[
            table(
                "capability",
                &[
                    column("id", PgType::BigInt, false),
                    column("binding_id", PgType::BigInt, false),
                    unique_column("token", PgType::Bytea, false),
                ],
            ),
            table("binding", &[column("id", PgType::BigInt, false)]),
        ]),
    )
    .expect("unique-key join compiles as optional");
    assert_eq!(compiled[0].inferred_cardinality.upper(), UpperBound::One);
}

#[test]
fn bounded_cte_joined_to_primary_key_preserves_optional_cardinality() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(93),
        r#"query FindAttemptGraph(attempt_id: text) -> optional {
    with target as (
        select attempt.id, attempt.run_binding_id
        from attempt
        where attempt.attempt_id = :attempt_id
    )
    select target.id, graph.generation
    from target
    join graph on graph.run_binding_id = target.run_binding_id
}"#,
        &catalog(&[
            table(
                "attempt",
                &[
                    column("id", PgType::BigInt, false),
                    column("run_binding_id", PgType::BigInt, false),
                    unique_column("attempt_id", PgType::Text, false),
                ],
            ),
            table(
                "graph",
                &[
                    primary_key_column("run_binding_id", PgType::BigInt, false),
                    column("generation", PgType::BigInt, false),
                ],
            ),
        ]),
    )
    .expect("bounded CTE joined to a primary key compiles as optional");
    assert_eq!(compiled[0].inferred_cardinality.upper(), UpperBound::One);
}

#[test]
fn delete_returning_by_composite_primary_key_is_optional() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(94),
        r#"query DeleteLine(attempt_id: bigint, line_number: bigint) -> optional {
    delete from line as line
    where line.attempt_id = :attempt_id
      and line.line_number = :line_number
    returning line.line_number
}"#,
        &catalog(&[table(
            "line",
            &[
                primary_key_column("attempt_id", PgType::BigInt, false),
                primary_key_column("line_number", PgType::BigInt, false),
            ],
        )]),
    )
    .expect("DELETE RETURNING by composite primary key compiles as optional");
    let query = compiled[0].validate().expect("DELETE artifact validates");
    assert_eq!(query.inferred_cardinality.upper(), UpperBound::One);
    let rendered = render_compiled_sql(query).expect("DELETE renders");
    assert!(rendered.sql.starts_with("DELETE FROM \"public\".\"line\""));
    assert!(rendered.sql.contains("RETURNING"));
}

#[test]
fn qualified_wildcard_expands_resolved_cte_columns_in_order() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(95),
        r#"query LockedCursor(id: bigint) -> optional {
    with cursor as (
        select state.id, state.position
        from state
        where state.id = :id
    )
    select cursor.*
    from cursor
}"#,
        &catalog(&[table(
            "state",
            &[
                column("id", PgType::BigInt, false),
                column("position", PgType::BigInt, false),
            ],
        )]),
    )
    .expect("qualified CTE wildcard compiles");
    assert_eq!(compiled[0].ordered_output_fields.len(), 2);
    assert_eq!(compiled[0].ordered_output_fields[0].sql_label, "id");
    assert_eq!(compiled[0].ordered_output_fields[1].sql_label, "position");
}

#[test]
fn polymorphic_to_jsonb_accepts_text() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(96),
        r#"query Encode(value: text) -> one {
    select to_jsonb(:value::text) as value
}"#,
        &catalog(&[]),
    )
    .expect("to_jsonb(anyelement) resolves for text");
    let query = compiled[0].validate().expect("artifact validates");
    let rendered = render_compiled_sql(query).expect("to_jsonb renders");
    assert!(rendered.sql.contains("\"pg_catalog\".\"to_jsonb\""));
}
#[test]
fn explicit_timestamptz_to_text_cast_compiles() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(97),
        r#"query RenderTimestamp() -> one {
    select now()::text as rendered
}"#,
        &catalog(&[]),
    )
    .expect("PostgreSQL explicit timestamptz-to-text I/O cast compiles");
    let query = compiled[0].validate().expect("artifact validates");
    assert_eq!(
        query.ordered_output_fields[0].type_id,
        catalog(&[]).resolve_type("pg_catalog.text").unwrap().id
    );
}

#[test]
fn nested_unique_key_join_preserves_optional_cardinality_from_rightmost_filter() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(84),
        r#"query FindAttempt(attempt_id: text) -> optional {
    select attempt.id
    from capability
    join job on job.binding_id = capability.binding_id
    join attempt on attempt.job_id = job.id
    where attempt.attempt_id = :attempt_id
}"#,
        &catalog(&[
            table(
                "capability",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("binding_id", PgType::BigInt, false),
                ],
            ),
            table(
                "job",
                &[
                    column("id", PgType::BigInt, false),
                    column("binding_id", PgType::BigInt, false),
                ],
            ),
            table(
                "attempt",
                &[
                    column("id", PgType::BigInt, false),
                    column("job_id", PgType::BigInt, false),
                    unique_column("attempt_id", PgType::Text, false),
                ],
            ),
        ]),
    )
    .expect("rightmost unique filter bounds the mirrored nested join chain");
    assert_eq!(compiled[0].inferred_cardinality.upper(), UpperBound::One);
}

#[test]
fn outer_join_subtree_does_not_supply_recursive_unique_proof() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(86),
        r#"query FindProfile(external_id: text) -> optional {
    select account.id
    from account
    left join profile on profile.account_id = account.id
    join audit on audit.account_id = account.id
    where profile.external_id = :external_id
}"#,
        &catalog(&[
            table("account", &[column("id", PgType::BigInt, false)]),
            table(
                "profile",
                &[
                    column("id", PgType::BigInt, false),
                    column("account_id", PgType::BigInt, false),
                    unique_column("external_id", PgType::Text, false),
                ],
            ),
            table(
                "audit",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("account_id", PgType::BigInt, false),
                ],
            ),
        ]),
    )
    .expect_err("outer-join subtrees must not satisfy recursive uniqueness");
    assert_eq!(
        diagnostics[0].code,
        CompileDiagnosticCode::ResultModeMismatch
    );
}
#[test]
fn left_unique_key_join_preserves_optional_cardinality() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(80),
        r#"query FindCapabilityBinding(token: bytea) -> optional {
    select capability.id, binding.id as binding_id
    from capability
    left join binding on binding.id = capability.binding_id
    where capability.token = :token
}"#,
        &catalog(&[
            table(
                "capability",
                &[
                    column("id", PgType::BigInt, false),
                    column("binding_id", PgType::BigInt, false),
                    unique_column("token", PgType::Bytea, false),
                ],
            ),
            table("binding", &[column("id", PgType::BigInt, false)]),
        ]),
    )
    .expect("left unique-key join compiles as optional");
    assert_eq!(compiled[0].inferred_cardinality.upper(), UpperBound::One);
}

#[test]
fn left_unique_join_preserves_exactly_one_lower_bound() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(83),
        r#"query CurrentBinding() -> many {
    select anchor.binding_id, binding.id as matched_binding_id
    from (select 1::bigint as binding_id) as anchor
    left join binding on binding.id = anchor.binding_id
}"#,
        &catalog(&[table("binding", &[column("id", PgType::BigInt, false)])]),
    )
    .expect("LEFT JOIN preserves its exactly-one left input");
    assert_eq!(compiled[0].inferred_cardinality.lower(), LowerBound::One);
    assert_eq!(compiled[0].inferred_cardinality.upper(), UpperBound::One);
}

#[test]
fn lateral_limit_one_preserves_optional_cardinality() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(78),
        r#"query FindCapabilityDelivery(token: bytea) -> optional {
    select capability.id, delivery.id as delivery_id
    from capability
    left join lateral (
        select candidate.id
        from delivery as candidate
        where candidate.capability_id = capability.id
        order by candidate.id
        limit 1
    ) as delivery on true
    where capability.token = :token
}"#,
        &catalog(&[
            table(
                "capability",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("token", PgType::Bytea, false),
                ],
            ),
            table(
                "delivery",
                &[
                    column("id", PgType::BigInt, false),
                    column("capability_id", PgType::BigInt, false),
                ],
            ),
        ]),
    )
    .expect("lateral LIMIT 1 compiles as optional");
    assert_eq!(compiled[0].inferred_cardinality.upper(), UpperBound::One);
}

#[test]
fn one_to_many_join_does_not_compile_as_optional() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(79),
        r#"query FindCapabilityDeliveries(token: bytea) -> optional {
    select capability.id, delivery.id as delivery_id
    from capability
    join delivery on delivery.capability_id = capability.id
    where capability.token = :token
}"#,
        &catalog(&[
            table(
                "capability",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("token", PgType::Bytea, false),
                ],
            ),
            table(
                "delivery",
                &[
                    column("id", PgType::BigInt, false),
                    column("capability_id", PgType::BigInt, false),
                ],
            ),
        ]),
    )
    .expect_err("one-to-many join must not satisfy optional mode");
    assert_eq!(
        diagnostics[0].code,
        CompileDiagnosticCode::ResultModeMismatch
    );
}

#[test]
fn cross_table_unique_equality_does_not_compile_as_optional() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(81),
        r#"query MatchUniqueKeys() -> optional {
    select account.id, profile.id as profile_id
    from account, profile
    where account.external_id = profile.external_id
}"#,
        &catalog(&[
            table(
                "account",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("external_id", PgType::Text, false),
                ],
            ),
            table(
                "profile",
                &[
                    column("id", PgType::BigInt, false),
                    unique_column("external_id", PgType::Text, false),
                ],
            ),
        ]),
    )
    .expect_err("cross-table unique equality does not globally bound the result");
    assert_eq!(
        diagnostics[0].code,
        CompileDiagnosticCode::ResultModeMismatch
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
fn with_locked_candidate_update_from_returning_compiles_atomically() {
    let source = r#"query ClaimDelivery() -> optional {
    with candidate as (
        select id
        from delivery
        where state = 'pending'
        order by id
        for update skip locked
        limit 1
    )
    update delivery as claimed
    set state = 'processing'
    from candidate as candidate_row
    where claimed.id = candidate_row.id
    returning claimed.id as id, claimed.state as state
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(32),
        source,
        &catalog(&[table(
            "delivery",
            &[
                column("id", PgType::BigInt, false),
                column("state", PgType::Text, false),
            ],
        )]),
    )
    .expect("WITH locked candidate UPDATE ... FROM ... RETURNING compiles");

    let query = compiled[0].validate().expect("mutation artifact validates");
    let HirStatementKind::Update(update) = &query.resolved_hir.statement.kind else {
        panic!("expected UPDATE HIR")
    };
    assert_eq!(update.ctes.len(), 1);
    let HirStatementKind::Select(candidate) = &update.ctes[0].statement.kind else {
        panic!("expected candidate SELECT CTE")
    };
    assert_eq!(candidate.locks.len(), 1);
    assert_eq!(update.from.len(), 1);
    assert!(matches!(update.from[0].kind, HirRelationKind::Cte { .. }));
    assert_eq!(update.from[0].alias.as_ref().unwrap().name, "candidate_row");
    let candidate_binding = update.from[0].id;
    let delivery_table = update.target.clone();

    let TypedStatementKind::Update(update) = &query.typed_statement.kind else {
        panic!("expected typed UPDATE")
    };
    assert_eq!(update.returning.len(), 2);
    assert_eq!(query.typed_statement.cardinality.lower(), LowerBound::Zero);
    assert_eq!(query.typed_statement.cardinality.upper(), UpperBound::One);
    assert!(
        query
            .typed_statement
            .cardinality
            .proof()
            .iter()
            .any(|evidence| {
                matches!(
                    evidence,
                    CardinalityEvidence::MutationUniqueCteJoin { cte, columns, .. }
                        if *cte == update.ctes[0].id && columns.len() == 1
                )
            })
    );
    assert!(
        query
            .typed_statement
            .cardinality
            .proof()
            .contains(&CardinalityEvidence::MutationReturning)
    );
    let predicate = update.predicate.as_ref().expect("UPDATE predicate");
    let TypedExpressionKind::Operator { operands, .. } = &predicate.kind else {
        panic!("expected equality predicate")
    };
    assert!(operands.iter().any(|operand| matches!(
        operand.expression.kind,
        TypedExpressionKind::CteColumn { binding, .. } if binding == candidate_binding
    )));
    assert!(matches!(
        query.read_write_lock_manifest.mutation,
        Some(MutationManifest::Update { .. })
    ));
    assert_eq!(
        query.read_write_lock_manifest.writes,
        vec![delivery_table.clone()]
    );
    assert_eq!(query.read_write_lock_manifest.reads, vec![delivery_table]);
    let references = &query.resolved_references.references;
    assert!(references.iter().any(|reference| {
        reference.role == ReferenceRole::AssignmentTarget
            && reference.access == ReferenceAccess::Write
            && matches!(reference.target, ReferenceTarget::Table(_))
    }));
    assert!(references.iter().any(|reference| {
        reference.role == ReferenceRole::AssignmentTarget
            && reference.access == ReferenceAccess::Write
            && matches!(reference.target, ReferenceTarget::Column(_))
    }));
    assert!(references.iter().any(|reference| {
        reference.role == ReferenceRole::CteDependency
            && matches!(reference.target, ReferenceTarget::Cte(_))
    }));
    assert!(references.iter().any(|reference| {
        reference.role == ReferenceRole::Predicate
            && matches!(reference.target, ReferenceTarget::OutputField(_))
    }));
    assert_eq!(
        references
            .iter()
            .filter(|reference| reference.role == ReferenceRole::Returning)
            .count(),
        2
    );

    let rendered = render_compiled_sql(query).expect("claim SQL renders");
    assert!(rendered.sql.starts_with("WITH \"candidate\""));
    assert!(
        rendered.sql.contains("LIMIT 1 FOR UPDATE SKIP LOCKED"),
        "{}",
        rendered.sql
    );
    assert!(rendered.sql.contains("UPDATE \"public\".\"delivery\""));
    assert!(
        rendered
            .sql
            .contains(" FROM \"candidate\" AS \"candidate_row\"")
    );
    assert!(
        rendered
            .sql
            .contains("\"delivery\".\"id\" = \"candidate_row\".\"id\"")
    );
    assert!(rendered.sql.contains(" RETURNING "));
}

#[test]
fn update_returning_by_primary_key_is_optional() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(54),
        r#"query MarkPublished(id: bigint, epoch: bigint) -> optional {
    update publication
    set state = 'published'
    where id = :id
      and epoch = :epoch
    returning id
}"#,
        &catalog(&[table(
            "publication",
            &[
                column("id", PgType::BigInt, false),
                column("epoch", PgType::BigInt, false),
                column("state", PgType::Text, false),
            ],
        )]),
    )
    .expect("primary-key-fenced UPDATE RETURNING compiles as optional");

    let query = compiled[0].validate().expect("mutation artifact validates");
    assert_eq!(query.inferred_cardinality.upper(), UpperBound::One);
    assert!(query.inferred_cardinality.proof().iter().any(|evidence| {
        matches!(evidence, CardinalityEvidence::UniquePredicate { columns, .. } if columns.len() == 1)
    }));
}

#[test]
fn insert_conflict_returning_compiles_with_write_manifest_and_excluded_scope() {
    let source = r#"query UpsertRun(id: bigint, owner: text) -> one {
    insert into run(id, owner)
    values (:id, :owner)
    on conflict (id) do update
    set owner = excluded.owner
    returning run.id as id, run.owner as owner
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(52),
        source,
        &catalog(&[table(
            "run",
            &[
                column("id", PgType::BigInt, false),
                column("owner", PgType::Text, false),
            ],
        )]),
    )
    .expect("INSERT ... ON CONFLICT ... RETURNING compiles");
    let query = compiled[0].validate().expect("mutation artifact validates");
    let TypedStatementKind::Insert(insert) = &query.typed_statement.kind else {
        panic!("expected typed INSERT")
    };
    assert_eq!(insert.returning.len(), 2);
    assert!(matches!(
        query.read_write_lock_manifest.mutation,
        Some(MutationManifest::Insert { .. })
    ));
    assert_eq!(query.read_write_lock_manifest.writes.len(), 1);
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .any(|reference| {
                reference.role == ReferenceRole::InsertTarget
                    && reference.access == ReferenceAccess::Write
                    && matches!(reference.target, ReferenceTarget::Table(_))
            })
    );
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .any(|reference| {
                reference.role == ReferenceRole::ConflictAction
                    && matches!(reference.target, ReferenceTarget::Column(_))
            })
    );

    let rendered = render_compiled_sql(query).expect("mutation SQL renders");
    assert_eq!(
        rendered.sql,
        "INSERT INTO \"public\".\"run\" (\"id\", \"owner\") VALUES ($1, $2) ON CONFLICT (\"id\") DO UPDATE SET \"owner\" = \"excluded\".\"owner\" RETURNING \"public\".\"run\".\"id\" AS \"id\", \"public\".\"run\".\"owner\" AS \"owner\""
    );
    let generated = generate_compiled_rust(query).expect("mutation Rust API generates");
    assert!(generated.source.contains("pub async fn upsert_run"));
    assert!(generated.source.contains("pub struct UpsertRunResult"));
}

#[test]
fn insert_assignment_cast_reaches_reference_index_and_sql() {
    let source = r#"query UpsertAmount(id: bigint, amount: bigint) -> one {
    insert into event(id, amount)
    values (:id, :amount)
    on conflict (id) do update
    set amount = :amount::numeric
    returning event.id as id
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(53),
        source,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("amount", PgType::Numeric, false),
            ],
        )]),
    )
    .expect("cast-bearing mutation compiles through artifact assembly");
    let query = compiled[0]
        .validate()
        .expect("cast mutation artifact validates");
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .any(|reference| {
                reference.role == ReferenceRole::CastUse
                    && matches!(reference.target, ReferenceTarget::Cast(_))
            })
    );
    let rendered = render_compiled_sql(query).expect("cast mutation SQL renders");
    assert!(
        rendered
            .sql
            .contains("SET \"amount\" = $2::\"pg_catalog\".\"numeric\"")
    );
}

#[test]
fn explicit_multi_hop_cast_preserves_every_catalog_edge() {
    let source = r#"query WidenAmount(id: bigint, amount: smallint) -> one {
    insert into event(id, amount)
    values (:id, :amount)
    on conflict (id) do update
    set amount = :amount::bigint
    returning event.id as id
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(54),
        source,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("amount", PgType::BigInt, false),
            ],
        )]),
    )
    .expect("multi-hop explicit cast compiles");
    let query = compiled[0]
        .validate()
        .expect("multi-hop artifact validates");
    let cast_references = query
        .resolved_references
        .references
        .iter()
        .filter(|reference| reference.role == ReferenceRole::CastUse)
        .collect::<Vec<_>>();
    assert_eq!(cast_references.len(), 2);
    let rendered = render_compiled_sql(query).expect("multi-hop cast SQL renders");
    assert!(
        rendered
            .sql
            .contains("SET \"amount\" = $2::\"pg_catalog\".\"integer\"::\"pg_catalog\".\"bigint\"")
    );
}

#[test]
fn explicit_identity_cast_is_a_semantic_no_op() {
    let source = r#"query KeepAmount(id: bigint, amount: bigint) -> one {
    insert into event(id, amount)
    values (:id, :amount)
    on conflict (id) do update
    set amount = :amount::bigint
    returning event.id as id
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(55),
        source,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("amount", PgType::BigInt, false),
            ],
        )]),
    )
    .expect("identity explicit cast compiles");
    let query = compiled[0].validate().expect("identity artifact validates");
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .all(|reference| reference.role != ReferenceRole::CastUse)
    );
    let rendered = render_compiled_sql(query).expect("identity cast SQL renders");
    assert!(rendered.sql.contains("SET \"amount\" = $2"));
    assert!(!rendered.sql.contains("SET \"amount\" = $2::"));
}

#[test]
fn explicit_cast_accepts_semantically_typed_compound_expression() {
    let source = r#"query IncreaseAmount(id: bigint, amount: bigint) -> one {
    insert into event(id, amount)
    values (:id, :amount)
    on conflict (id) do update
    set amount = (:amount + 1)::numeric
    returning event.id as id
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(56),
        source,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("amount", PgType::Numeric, false),
            ],
        )]),
    )
    .expect("compound explicit cast compiles after operator typing");
    let query = compiled[0]
        .validate()
        .expect("compound cast artifact validates");
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .any(|reference| {
                reference.role == ReferenceRole::OperatorUse
                    && matches!(reference.target, ReferenceTarget::Operator(_))
            })
    );
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .any(|reference| {
                reference.role == ReferenceRole::CastUse
                    && matches!(reference.target, ReferenceTarget::Cast(_))
            })
    );
    let rendered = render_compiled_sql(query).expect("compound cast SQL renders");
    assert!(
        rendered
            .sql
            .contains("SET \"amount\" = ($2 + 1)::\"pg_catalog\".\"numeric\"")
    );
}

#[test]
fn explicit_cast_preserves_string_and_null_target_syntax() {
    let source = r#"query CastLiterals(id: bigint) -> one {
    insert into event(id, label, fallback)
    values (:id, 'ready'::text, null::text)
    on conflict (id) do update
    set label = 'ready'::text,
        fallback = null::text
    returning event.id as id
}"#;

    let compiled = compile_query_source(
        parser(),
        SourceId::new(57),
        source,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("label", PgType::Text, false),
                column("fallback", PgType::Text, true),
            ],
        )]),
    )
    .expect("unknown literal explicit casts compile");
    let query = compiled[0]
        .validate()
        .expect("unknown literal cast artifact validates");
    let rendered = render_compiled_sql(query).expect("unknown literal casts render");
    assert!(rendered.sql.contains("'ready'::\"pg_catalog\".\"text\""));
    assert!(rendered.sql.contains("NULL::\"pg_catalog\".\"text\""));
}

#[test]
fn explicit_cast_typmod_changes_identity_and_sql() {
    fn compile_with_typmod(typmod: &str) -> CompiledQuery {
        let source = format!(
            "query NormalizeAmount(id: bigint, amount: numeric) -> one {{\n    insert into event(id, amount)\n    values (:id, :amount)\n    on conflict (id) do update\n    set amount = :amount::numeric({typmod})\n    returning event.id as id\n}}"
        );
        compile_query_source(
            parser(),
            SourceId::new(58),
            &source,
            &catalog(&[table(
                "event",
                &[
                    column("id", PgType::BigInt, false),
                    column("amount", PgType::Numeric, false),
                ],
            )]),
        )
        .expect("typmod cast compiles")
        .into_iter()
        .next()
        .expect("one compiled query")
    }

    let narrow = compile_with_typmod("10,2");
    let wide = compile_with_typmod("12,4");
    narrow.validate().expect("narrow typmod artifact validates");
    wide.validate().expect("wide typmod artifact validates");
    assert_ne!(narrow.execution_semantics_id, wide.execution_semantics_id);
    assert!(
        narrow
            .deterministic_sql
            .contains("::\"pg_catalog\".\"numeric\"(10,2)")
    );
    assert!(
        wide.deterministic_sql
            .contains("::\"pg_catalog\".\"numeric\"(12,4)")
    );
}
#[test]
fn insert_select_rejects_non_coercible_projection() {
    let diagnostics = compile_query_source(
        parser(),
        SourceId::new(88),
        r#"query InsertWrongType(value: text) -> one {
    insert into event(amount)
    select :value
    returning event.amount
}"#,
        &catalog(&[table("event", &[column("amount", PgType::BigInt, false)])]),
    )
    .expect_err("text cannot be implicitly assigned to bigint");
    assert_eq!(diagnostics[0].code, CompileDiagnosticCode::TypeMismatch);
}

#[test]
fn insert_select_preserves_implicit_projection_coercion() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(89),
        r#"query InsertWidened(value: smallint) -> one {
    insert into event(amount)
    select :value
    returning event.amount
}"#,
        &catalog(&[table("event", &[column("amount", PgType::BigInt, false)])]),
    )
    .expect("smallint implicitly coerces to bigint");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("coerced INSERT SELECT renders");
    assert!(
        rendered.sql.contains("$1::\"pg_catalog\".\"bigint\""),
        "{}",
        rendered.sql
    );
}

#[test]
fn insert_select_accepts_assignment_only_projection_coercion() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(90),
        r#"query InsertNarrowed(value: bigint) -> one {
    insert into event(amount)
    select :value
    returning event.amount
}"#,
        &catalog(&[table("event", &[column("amount", PgType::Integer, false)])]),
    )
    .expect("bigint uses PostgreSQL's assignment-only cast to integer");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("assignment-coerced INSERT SELECT renders");
    assert!(
        rendered.sql.contains("$1::\"pg_catalog\".\"integer\""),
        "{}",
        rendered.sql
    );
}

#[test]
fn insert_select_set_preserves_common_and_assignment_coercions() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(91),
        r#"query InsertSet(left_value: smallint, right_value: integer) -> many {
    insert into event(amount)
    select :left_value
    union all
    select :right_value
    returning event.amount
}"#,
        &catalog(&[table("event", &[column("amount", PgType::BigInt, false)])]),
    )
    .expect("set common type then INSERT assignment coercion compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("coerced set INSERT SELECT renders");
    assert!(
        rendered.sql.contains("$1::\"pg_catalog\".\"integer\""),
        "{}",
        rendered.sql
    );
    assert!(
        rendered.sql.contains("::\"pg_catalog\".\"bigint\""),
        "{}",
        rendered.sql
    );
}

#[test]
fn insert_select_cte_with_conflict_and_returning_compiles() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(87),
        r#"query PlaceAttempt(job_id: bigint, attempt_id: text) -> optional {
    with eligible as (
        select job.id
        from job
        where job.id = :job_id
    ), placed as (
        insert into attempt(job_id, attempt_id)
        select eligible.id, :attempt_id
        from eligible
        on conflict (attempt_id) do nothing
        returning attempt.id
    )
    select eligible.id,
        exists(select 1 from placed) as applied
    from eligible
}"#,
        &catalog(&[
            table("job", &[column("id", PgType::BigInt, false)]),
            table(
                "attempt",
                &[
                    column("id", PgType::BigInt, false),
                    column("job_id", PgType::BigInt, false),
                    unique_column("attempt_id", PgType::Text, false),
                ],
            ),
        ]),
    )
    .expect("INSERT SELECT inside a data-modifying CTE compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("INSERT SELECT renders");
    assert!(rendered.sql.contains("INSERT INTO \"public\".\"attempt\""));
    assert!(
        rendered.sql.contains("\"eligible\".\"id\""),
        "{}",
        rendered.sql
    );
    assert!(rendered.sql.contains("$2"), "{}", rendered.sql);
    assert!(
        rendered
            .sql
            .contains("ON CONFLICT (\"attempt_id\") DO NOTHING")
    );
    assert!(
        rendered
            .sql
            .contains("RETURNING \"public\".\"attempt\".\"id\"")
    );
}

#[test]
fn ordered_set_aggregate_extract_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(84),
        r#"query QueueWait() -> one {
    select percentile_cont(0.50) within group (
        order by extract(epoch from (coalesce(attempt.created_at, now()) - job.ready_at)) * 1000
    )::bigint as queue_wait_millis
    from job
    left join attempt on attempt.job_id = job.id
}"#,
        &catalog(&[
            table(
                "job",
                &[
                    column("id", PgType::BigInt, false),
                    column("ready_at", PgType::Timestamptz, false),
                ],
            ),
            table(
                "attempt",
                &[
                    column("id", PgType::BigInt, false),
                    column("job_id", PgType::BigInt, false),
                    column("created_at", PgType::Timestamptz, false),
                ],
            ),
        ]),
    )
    .expect("ordered-set aggregate with EXTRACT compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("ordered-set aggregate renders");
    assert!(rendered.sql.contains("percentile_cont"), "{}", rendered.sql);
    assert!(rendered.sql.contains("WITHIN GROUP"), "{}", rendered.sql);
    assert!(
        rendered.sql.contains("EXTRACT(EPOCH FROM"),
        "{}",
        rendered.sql
    );
}

#[test]
fn bytea_position_compiles_and_renders() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(85),
        r#"query Search(needle: bytea) -> many {
    select (position(:needle::bytea in chunk.bytes) - 1)::bigint as byte_offset
    from chunk
    where position(:needle::bytea in chunk.bytes) > 0
}"#,
        &catalog(&[table(
            "chunk",
            &[
                column("id", PgType::BigInt, false),
                column("bytes", PgType::Bytea, false),
            ],
        )]),
    )
    .expect("bytea POSITION compiles");
    let query = compiled[0].validate().expect("artifact is checked");
    let rendered = render_compiled_sql(query).expect("POSITION renders");
    assert!(
        rendered.sql.contains("POSITION($1 IN \"chunk\".\"bytes\")"),
        "{}",
        rendered.sql
    );
}

#[test]
fn jsonb_path_operators_contextualize_string_keys() {
    let compiled = compile_query_source(
        parser(),
        SourceId::new(82),
        r#"query RepositoryOwner(id: bigint) -> optional {
    select payload -> 'source' -> 'repository_coordinates' ->> 'owner' as owner
    from event
    where id = :id
}"#,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("payload", PgType::Jsonb, false),
            ],
        )]),
    )
    .expect("JSONB path operators compile");
    let query = compiled[0].validate().expect("artifact is checked");
    assert_eq!(
        query.ordered_output_fields[0].type_id.as_str(),
        "pg18:type:base:pg_catalog.text"
    );
    let rendered = render_compiled_sql(query).expect("JSONB path SQL renders");
    assert!(rendered.sql.contains("-> 'source'"), "{}", rendered.sql);
    assert!(rendered.sql.contains("->> 'owner'"), "{}", rendered.sql);
}

#[test]
fn text_to_jsonb_uses_registered_explicit_io_coercion() {
    let source = r#"query StorePayload(id: bigint, payload: text) -> one {
    insert into event(id, payload)
    values (:id, :payload::text::jsonb)
    on conflict (id) do update
    set payload = :payload::text::jsonb
    returning event.id as id
}"#;
    let compiled = compile_query_source(
        parser(),
        SourceId::new(60),
        source,
        &catalog(&[table(
            "event",
            &[
                column("id", PgType::BigInt, false),
                column("payload", PgType::Jsonb, false),
            ],
        )]),
    )
    .expect("registered text-to-jsonb I/O coercion compiles");
    let query = compiled[0]
        .validate()
        .expect("I/O coercion artifact validates");
    assert!(
        query
            .resolved_references
            .references
            .iter()
            .any(|reference| {
                matches!(reference.target, ReferenceTarget::IoCoercion(_))
                    && reference.role == ReferenceRole::CastUse
            })
    );
    assert!(
        query
            .deterministic_sql
            .contains("$2::\"pg_catalog\".\"jsonb\"")
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
fn unique_column(name: &str, pg_type: PgType, nullable: bool) -> Column {
    let mut column = column(name, pg_type, nullable);
    column.unique = true;
    column
}

fn primary_key_column(name: &str, pg_type: PgType, nullable: bool) -> Column {
    let mut column = column(name, pg_type, nullable);
    column.primary_key = true;
    column
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
fn is_null_and_distinct_from_use_structural_operators() {
    let is_null = compile_predicate("owner is null", "query Filter() -> many");
    assert_operator(
        select_predicate(&is_null),
        dibs_query_typing::SYNTAX_IS_NULL_OPERATOR_ID,
        |operands| assert_eq!(operands.len(), 1),
    );
    let rendered = render_compiled_sql(is_null.validate().expect("artifact is checked"))
        .expect("IS NULL renders");
    assert!(rendered.sql.contains("IS NULL"));

    let is_not_null = compile_predicate("owner is not null", "query Filter() -> many");
    assert_operator(
        select_predicate(&is_not_null),
        dibs_query_typing::SYNTAX_IS_NOT_NULL_OPERATOR_ID,
        |operands| assert_eq!(operands.len(), 1),
    );
    let rendered = render_compiled_sql(is_not_null.validate().expect("artifact is checked"))
        .expect("IS NOT NULL renders");
    assert!(rendered.sql.contains("IS NOT NULL"));

    let distinct = compile_predicate(
        "owner is distinct from :owner",
        "query Filter(owner: text) -> many",
    );
    assert_operator(
        select_predicate(&distinct),
        dibs_query_typing::SYNTAX_IS_DISTINCT_FROM_OPERATOR_ID,
        |operands| assert_eq!(operands.len(), 2),
    );
    let rendered = render_compiled_sql(distinct.validate().expect("artifact is checked"))
        .expect("IS DISTINCT FROM renders");
    assert!(rendered.sql.contains("IS DISTINCT FROM"));

    let not_distinct = compile_predicate(
        "owner is not distinct from :owner",
        "query Filter(owner: text) -> many",
    );
    assert_operator(
        select_predicate(&not_distinct),
        dibs_query_typing::SYNTAX_IS_NOT_DISTINCT_FROM_OPERATOR_ID,
        |operands| assert_eq!(operands.len(), 2),
    );
    let rendered = render_compiled_sql(not_distinct.validate().expect("artifact is checked"))
        .expect("IS NOT DISTINCT FROM renders");
    assert!(rendered.sql.contains("IS NOT DISTINCT FROM"));
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
