#[path = "../src/backend/sql.rs"]
mod sql_backend;

use dibs_pg_catalog::{
    CallableId, CastId, CollationId, ColumnId, ConstraintId, OperatorId, TableId, TypeId,
};
use dibs_query_ir::{
    Cardinality, CatalogRenderName, CatalogRenderNames, CoercionContext, CoercionEvidence,
    CompiledQuery, ConflictTarget, CteId, CteMaterialization, ExpressionId, FieldId, FrameBound,
    HirLiteral, HirLockClause, JoinKind, LockStrength, LockWaitPolicy, Nullability,
    NullabilityEvidence, NullsOrder, OrderedBind, ParameterId, RelationAlias, RelationId,
    SelectDistinct, SetOperationKind, SortDirection, SourceOrigin, SourceSpan, Span, StatementId,
    TypedArgument, TypedAssignment, TypedCall, TypedCaseBranch, TypedCastStep, TypedCoercion,
    TypedConflictAction, TypedConflictClause, TypedCte, TypedDelete, TypedExpression,
    TypedExpressionKind, TypedInsert, TypedInsertSource, TypedLimit, TypedNamedWindow,
    TypedOrderBy, TypedProjection, TypedRelation, TypedRelationKind, TypedSelect, TypedStatement,
    TypedStatementKind, TypedUpdate, TypedValues, TypedValuesColumn, Volatility, WindowExclusion,
    WindowFrame, WindowFrameMode, WindowReference, WindowSpec,
};
use dibs_query_syntax::SourceId;
use sql_backend::{RenderedSql, SqlRenderError, render_compiled_sql};

const BIGINT: &str = "pg18:type:pg_catalog.bigint:base";
const TEXT: &str = "pg18:type:pg_catalog.text:base";
const BOOL: &str = "pg18:type:pg_catalog.boolean:base";
const WIDGET: &str = "pg18:table:app.Widget";
const OTHER: &str = "pg18:table:app.other";
const ID: &str = "pg18:column:app.Widget.id";
const NAME: &str = "pg18:column:app.Widget.display name";
const OTHER_ID: &str = "pg18:column:app.other.widget_id";
const ADD: &str = "pg18:operator:pg_catalog.+";
const EQ: &str = "pg18:operator:pg_catalog.=";
const NOT: &str = "pg18:operator:pg_catalog.NOT";
const IS_NULL: &str = "pg18:operator:pg_catalog.IS NULL";
const COUNT: &str = "pg18:callable:pg_catalog.count";
const GENERATE_SERIES: &str = "pg18:callable:pg_catalog.generate_series";
const CAST_TEXT: &str = "pg18:cast:bigint:text";
const C_COLLATION: &str = "pg18:collation:pg_catalog.C";
const UNIQUE_NAME: &str = "pg18:constraint:app.Widget:Widget_name_key";

#[test]
fn renders_select_windows_relations_expressions_and_locks() {
    let cte = typed_cte(
        1,
        "seed values",
        CteMaterialization::Materialized,
        select_statement(2, vec![projection(20, "seed id", integer(20, "1"))], vec![]),
    );
    let left = table_relation(1, WIDGET, Some(alias("w", &[])));
    let right = TypedRelation {
        id: RelationId::new(2),
        origin: origin(),
        alias: Some(alias("series", &["n"])),
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::Function {
            callable_id: CallableId::new(GENERATE_SERIES),
            arguments: vec![integer(31, "1"), parameter(32, 2)],
        },
    };
    let join = TypedRelation {
        id: RelationId::new(3),
        origin: origin(),
        alias: None,
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::Join {
            kind: JoinKind::Left,
            left: Box::new(left),
            right: Box::new(right),
            predicate: Some(Box::new(operator(
                33,
                EQ,
                vec![column(34, RelationId::new(1), ID), parameter(35, 1)],
            ))),
            lateral: true,
        },
    };
    let values = TypedRelation {
        id: RelationId::new(4),
        origin: origin(),
        alias: Some(alias("v", &["x", "label"])),
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::Values {
            rows: typed_values(vec![
                vec![integer(40, "1"), string(41, "a")],
                vec![integer(42, "2"), string(43, "b")],
            ]),
        },
    };
    let cte_use = TypedRelation {
        id: RelationId::new(5),
        origin: origin(),
        alias: Some(alias("seed", &[])),
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::Cte {
            cte_id: CteId::new(1),
        },
    };
    let subquery = TypedRelation {
        id: RelationId::new(6),
        origin: origin(),
        alias: Some(alias("derived", &["value"])),
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::Subquery(Box::new(select_statement(
            7,
            vec![projection(70, "value", integer(70, "7"))],
            vec![],
        ))),
    };
    let set = TypedRelation {
        id: RelationId::new(7),
        origin: origin(),
        alias: Some(alias("set rows", &["value"])),
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::SetOperation {
            kind: SetOperationKind::Union,
            all: true,
            left: Box::new(select_statement(
                8,
                vec![projection(80, "value", integer(80, "8"))],
                vec![],
            )),
            right: Box::new(select_statement(
                9,
                vec![projection(90, "value", integer(90, "9"))],
                vec![],
            )),
        },
    };
    let call = expression(
        100,
        BIGINT,
        TypedExpressionKind::Call(Box::new(TypedCall {
            authored_callable_id: CallableId::new(COUNT),
            callable_id: CallableId::new(COUNT),
            arguments: vec![TypedArgument {
                expression: column(101, RelationId::new(1), ID),
                coercion: None,
            }],
            distinct: true,
            star: false,
            order_by: vec![order(
                column(102, RelationId::new(1), NAME),
                SortDirection::Descending,
                NullsOrder::Last,
            )],
            filter: Some(Box::new(operator(
                103,
                NOT,
                vec![operator(
                    104,
                    IS_NULL,
                    vec![column(105, RelationId::new(1), NAME)],
                )],
            ))),
            within_group: vec![],
            over: Some(WindowReference::Inline(WindowSpec {
                existing: Some("base window".to_string()),
                partition_by: vec![column(106, RelationId::new(1), ID)],
                order_by: vec![order(
                    parameter(107, 1),
                    SortDirection::Ascending,
                    NullsOrder::First,
                )],
                frame: Some(WindowFrame {
                    mode: WindowFrameMode::Groups,
                    start: FrameBound::UnboundedPreceding,
                    end: Some(FrameBound::Following(parameter(108, 2))),
                    exclusion: WindowExclusion::Ties,
                }),
            })),
        })),
    );
    let case = expression(
        110,
        TEXT,
        TypedExpressionKind::Case {
            operand: Some(Box::new(column(111, RelationId::new(1), ID))),
            branches: vec![TypedCaseBranch {
                when: integer(112, "1"),
                then: TypedArgument {
                    expression: collate(113, string(114, "one")),
                    coercion: None,
                },
            }],
            else_expression: Some(Box::new(TypedArgument {
                expression: cast_to_text(115, parameter(116, 1)),
                coercion: None,
            })),
            implicit_else_type: None,
            result_coercion: CoercionEvidence::CommonType {
                resolved: TypeId::new(TEXT),
                inputs: vec![TypeId::new(TEXT), TypeId::new(TEXT)],
            },
        },
    );
    let scalar = expression(
        120,
        BIGINT,
        TypedExpressionKind::ScalarSubquery(Box::new(select_statement(
            10,
            vec![projection(121, "scalar", integer(121, "3"))],
            vec![],
        ))),
    );
    let select = TypedStatement {
        id: StatementId::new(1),
        origin: origin(),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: true,
            ctes: vec![cte],
            distinct: SelectDistinct::On(vec![column(130, RelationId::new(1), ID)]),
            projections: vec![
                projection(1, "aggregate total", call),
                projection(2, "case result", case),
                projection(
                    3,
                    "row value",
                    expression(
                        131,
                        BIGINT,
                        TypedExpressionKind::Row(vec![parameter(132, 1), scalar]),
                    ),
                ),
                projection(
                    4,
                    "array value",
                    expression(
                        133,
                        BIGINT,
                        TypedExpressionKind::Array {
                            elements: vec![
                                TypedArgument {
                                    expression: integer(134, "1"),
                                    coercion: None,
                                },
                                TypedArgument {
                                    expression: integer(135, "2"),
                                    coercion: None,
                                },
                            ],
                            coercion: CoercionEvidence::CommonType {
                                resolved: TypeId::new(BIGINT),
                                inputs: vec![TypeId::new(BIGINT), TypeId::new(BIGINT)],
                            },
                        },
                    ),
                ),
                projection(
                    5,
                    "cte field",
                    expression(
                        136,
                        BIGINT,
                        TypedExpressionKind::CteColumn {
                            cte_id: CteId::new(1),
                            field_id: FieldId::new(20),
                        },
                    ),
                ),
                projection(6, "bytes", bytes(137, &[0, 10, 255])),
                projection(7, "null", null(138)),
                projection(8, "bool", boolean(139, true)),
                projection(9, "numeric", numeric(140, "12.50")),
            ],
            from: vec![join, values, cte_use, subquery, set],
            predicate: Some(operator(
                141,
                EQ,
                vec![parameter(142, 1), parameter(143, 2)],
            )),
            group_by: vec![column(144, RelationId::new(1), ID)],
            having: Some(boolean(145, true)),
            windows: vec![TypedNamedWindow {
                name: "base window".to_string(),
                specification: WindowSpec {
                    existing: None,
                    partition_by: vec![column(146, RelationId::new(1), ID)],
                    order_by: vec![order(
                        column(147, RelationId::new(1), NAME),
                        SortDirection::Ascending,
                        NullsOrder::Default,
                    )],
                    frame: Some(WindowFrame {
                        mode: WindowFrameMode::Rows,
                        start: FrameBound::Preceding(integer(148, "2")),
                        end: Some(FrameBound::CurrentRow),
                        exclusion: WindowExclusion::CurrentRow,
                    }),
                },
            }],
            order_by: vec![order(
                column(149, RelationId::new(1), NAME),
                SortDirection::Descending,
                NullsOrder::Last,
            )],
            limit: Some(TypedLimit::Parameter(ParameterId::new(2))),
            offset: Some(TypedLimit::Constant(4)),
            locks: vec![HirLockClause {
                strength: LockStrength::NoKeyUpdate,
                targets: vec![RelationId::new(1)],
                wait: LockWaitPolicy::SkipLocked,
            }],
        })),
    };

    let rendered = render(&select, &[ParameterId::new(2), ParameterId::new(1)]);
    assert_eq!(
        rendered.sql,
        "WITH RECURSIVE \"seed values\" (\"seed id\") AS MATERIALIZED (SELECT 1 AS \"seed id\") SELECT DISTINCT ON (\"w\".\"id\") \"pg_catalog\".\"count\"(DISTINCT \"w\".\"id\" ORDER BY \"w\".\"display name\" DESC NULLS LAST) FILTER (WHERE NOT (\"w\".\"display name\" IS NULL)) OVER (\"base window\" PARTITION BY \"w\".\"id\" ORDER BY $2 ASC NULLS FIRST GROUPS BETWEEN UNBOUNDED PRECEDING AND $1 FOLLOWING EXCLUDE TIES) AS \"aggregate total\", CASE \"w\".\"id\" WHEN 1 THEN 'one' COLLATE \"pg_catalog\".\"C\" ELSE $2::\"pg_catalog\".\"text\" END AS \"case result\", ROW($2, (SELECT 3 AS \"scalar\")) AS \"row value\", ARRAY[1, 2] AS \"array value\", \"seed values\".\"seed id\" AS \"cte field\", '\\x000aff'::bytea AS \"bytes\", NULL AS \"null\", TRUE AS \"bool\", 12.50 AS \"numeric\" FROM \"app\".\"Widget\" AS \"w\" LEFT JOIN LATERAL \"pg_catalog\".\"generate_series\"(1, $1) AS \"series\" (\"n\") ON \"w\".\"id\" = $2, (VALUES (1, 'a'), (2, 'b')) AS \"v\" (\"x\", \"label\"), \"seed values\" AS \"seed\", (SELECT 7 AS \"value\") AS \"derived\" (\"value\"), ((SELECT 8 AS \"value\") UNION ALL (SELECT 9 AS \"value\")) AS \"set rows\" (\"value\") WHERE $2 = $1 GROUP BY \"w\".\"id\" HAVING TRUE WINDOW \"base window\" AS (PARTITION BY \"w\".\"id\" ORDER BY \"w\".\"display name\" ASC ROWS BETWEEN 2 PRECEDING AND CURRENT ROW EXCLUDE CURRENT ROW) ORDER BY \"w\".\"display name\" DESC NULLS LAST LIMIT $1 OFFSET 4 FOR NO KEY UPDATE OF \"w\" SKIP LOCKED"
    );
    assert_eq!(
        rendered.ordered_binds,
        vec![
            OrderedBind {
                position: 1,
                parameter_id: ParameterId::new(2)
            },
            OrderedBind {
                position: 2,
                parameter_id: ParameterId::new(1)
            },
        ]
    );
}

#[test]
fn renders_within_group_named_over_and_every_frame_and_lock_spelling() {
    let fixtures = [
        (
            WindowFrameMode::Range,
            FrameBound::CurrentRow,
            None,
            WindowExclusion::Group,
            "RANGE CURRENT ROW EXCLUDE GROUP",
        ),
        (
            WindowFrameMode::Rows,
            FrameBound::Following(integer(1, "2")),
            Some(FrameBound::UnboundedFollowing),
            WindowExclusion::NoOthers,
            "ROWS BETWEEN 2 FOLLOWING AND UNBOUNDED FOLLOWING EXCLUDE NO OTHERS",
        ),
        (
            WindowFrameMode::Groups,
            FrameBound::UnboundedPreceding,
            Some(FrameBound::CurrentRow),
            WindowExclusion::None,
            "GROUPS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
        ),
    ];
    for (index, (mode, start, end, exclusion, frame_sql)) in fixtures.into_iter().enumerate() {
        let call = expression(
            200 + index as u32,
            BIGINT,
            TypedExpressionKind::Call(Box::new(TypedCall {
                authored_callable_id: CallableId::new(COUNT),
                callable_id: CallableId::new(COUNT),
                arguments: vec![TypedArgument {
                    expression: integer(210, "1"),
                    coercion: None,
                }],
                distinct: false,
                star: false,
                order_by: vec![],
                filter: None,
                within_group: vec![order(
                    integer(211, "1"),
                    SortDirection::Ascending,
                    NullsOrder::Default,
                )],
                over: Some(WindowReference::Named("win".to_string())),
            })),
        );
        let statement = TypedStatement {
            id: StatementId::new(30 + index as u32),
            origin: origin(),
            cardinality: Cardinality::many(),
            kind: TypedStatementKind::Select(Box::new(TypedSelect {
                recursive: false,
                ctes: vec![],
                distinct: SelectDistinct::Distinct,
                projections: vec![projection(1, "x", call)],
                from: vec![table_relation(1, WIDGET, Some(alias("w", &[])))],
                predicate: None,
                group_by: vec![],
                having: None,
                windows: vec![TypedNamedWindow {
                    name: "win".to_string(),
                    specification: WindowSpec {
                        existing: None,
                        partition_by: vec![],
                        order_by: vec![],
                        frame: Some(WindowFrame {
                            mode,
                            start,
                            end,
                            exclusion,
                        }),
                    },
                }],
                order_by: vec![],
                limit: None,
                offset: None,
                locks: vec![HirLockClause {
                    strength: match index {
                        0 => LockStrength::Update,
                        1 => LockStrength::Share,
                        _ => LockStrength::KeyShare,
                    },
                    targets: vec![],
                    wait: match index {
                        0 => LockWaitPolicy::NoWait,
                        1 => LockWaitPolicy::Wait,
                        _ => LockWaitPolicy::SkipLocked,
                    },
                }],
            })),
        };
        let rendered = render(&statement, &[]);
        assert!(
            rendered
                .sql
                .contains("WITHIN GROUP (ORDER BY 1 ASC) OVER \"win\" AS \"x\""),
            "{}",
            rendered.sql
        );
        assert!(
            rendered
                .sql
                .contains(&format!("WINDOW \"win\" AS ({frame_sql})")),
            "{}",
            rendered.sql
        );
    }
}

#[test]
fn renders_insert_conflicts_and_returning() {
    let statement = TypedStatement {
        id: StatementId::new(1),
        origin: origin(),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Insert(Box::new(TypedInsert {
            ctes: vec![typed_cte(
                1,
                "changed",
                CteMaterialization::NotMaterialized,
                update_statement(2, vec![projection(20, "id", integer(20, "1"))]),
            )],
            target: TableId::new(WIDGET),
            target_binding: RelationId::new(1),
            columns: vec![ColumnId::new(ID), ColumnId::new(NAME)],
            source: TypedInsertSource::Values(typed_values(vec![vec![
                parameter(1, 2),
                string(2, "O'Reilly"),
            ]])),
            conflict: Some(TypedConflictClause {
                target: ConflictTarget::Inference {
                    expressions: vec![column(3, RelationId::new(1), NAME)],
                    predicate: Some(Box::new(boolean(4, true))),
                },
                action: TypedConflictAction::Update {
                    assignments: vec![assignment(1, NAME, parameter(5, 1), Some(text_coercion()))],
                    predicate: Some(Box::new(boolean(6, true))),
                },
            }),
            returning: vec![projection(1, "new id", column(7, RelationId::new(1), ID))],
        })),
    };
    let rendered = render(&statement, &[ParameterId::new(2), ParameterId::new(1)]);
    assert_eq!(
        rendered.sql,
        "WITH \"changed\" (\"id\") AS NOT MATERIALIZED (UPDATE \"app\".\"Widget\" SET \"display name\" = 1 RETURNING 1 AS \"id\") INSERT INTO \"app\".\"Widget\" (\"id\", \"display name\") VALUES ($1, 'O''Reilly') ON CONFLICT (\"display name\") WHERE TRUE DO UPDATE SET \"display name\" = $2::\"pg_catalog\".\"text\" WHERE TRUE RETURNING \"app\".\"Widget\".\"id\" AS \"new id\""
    );

    let constraint = TypedStatement {
        id: StatementId::new(3),
        origin: origin(),
        cardinality: Cardinality::empty(),
        kind: TypedStatementKind::Insert(Box::new(TypedInsert {
            ctes: vec![],
            target: TableId::new(WIDGET),
            target_binding: RelationId::new(1),
            columns: vec![ColumnId::new(NAME)],
            source: TypedInsertSource::DefaultValues,
            conflict: Some(TypedConflictClause {
                target: ConflictTarget::Constraint(ConstraintId::new(UNIQUE_NAME)),
                action: TypedConflictAction::Nothing,
            }),
            returning: vec![],
        })),
    };
    assert_eq!(
        render(&constraint, &[]).sql,
        "INSERT INTO \"app\".\"Widget\" (\"display name\") DEFAULT VALUES ON CONFLICT ON CONSTRAINT \"Widget_name_key\" DO NOTHING"
    );

    let unspecified = TypedStatement {
        id: StatementId::new(4),
        origin: origin(),
        cardinality: Cardinality::empty(),
        kind: TypedStatementKind::Insert(Box::new(TypedInsert {
            ctes: vec![],
            target: TableId::new(WIDGET),
            target_binding: RelationId::new(1),
            columns: vec![],
            source: TypedInsertSource::Select(Box::new(select_statement(
                5,
                vec![projection(5, "x", integer(5, "1"))],
                vec![],
            ))),
            conflict: Some(TypedConflictClause {
                target: ConflictTarget::Unspecified,
                action: TypedConflictAction::Nothing,
            }),
            returning: vec![],
        })),
    };
    assert_eq!(
        render(&unspecified, &[]).sql,
        "INSERT INTO \"app\".\"Widget\" SELECT 1 AS \"x\" ON CONFLICT DO NOTHING"
    );
}

#[test]
fn renders_update_delete_and_join_kinds() {
    let join_kinds = [
        (JoinKind::Inner, "INNER JOIN"),
        (JoinKind::Right, "RIGHT JOIN"),
        (JoinKind::Full, "FULL JOIN"),
        (JoinKind::Cross, "CROSS JOIN"),
    ];
    for (index, (kind, spelling)) in join_kinds.into_iter().enumerate() {
        let relation = TypedRelation {
            id: RelationId::new(10 + index as u32),
            origin: origin(),
            alias: None,
            cardinality: Cardinality::many(),
            kind: TypedRelationKind::Join {
                kind,
                left: Box::new(table_relation(2, OTHER, Some(alias("o", &[])))),
                right: Box::new(table_relation(3, WIDGET, Some(alias("w2", &[])))),
                predicate: (kind != JoinKind::Cross).then(|| Box::new(boolean(20, true))),
                lateral: false,
            },
        };
        let update = TypedStatement {
            id: StatementId::new(1),
            origin: origin(),
            cardinality: Cardinality::many(),
            kind: TypedStatementKind::Update(Box::new(TypedUpdate {
                ctes: vec![],
                target: TableId::new(WIDGET),
                target_binding: RelationId::new(1),
                assignments: vec![assignment(1, NAME, parameter(1, 1), None)],
                from: vec![relation],
                predicate: Some(operator(
                    2,
                    EQ,
                    vec![
                        column(3, RelationId::new(1), ID),
                        column(4, RelationId::new(2), OTHER_ID),
                    ],
                )),
                returning: vec![projection(1, "id", column(5, RelationId::new(1), ID))],
            })),
        };
        let sql = render(&update, &[ParameterId::new(1)]).sql;
        assert!(sql.contains(spelling), "{sql}");
        if kind == JoinKind::Cross {
            assert!(
                !sql.contains("CROSS JOIN \"app\".\"Widget\" AS \"w2\" ON"),
                "{sql}"
            );
        }
    }

    let delete = TypedStatement {
        id: StatementId::new(10),
        origin: origin(),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Delete(Box::new(TypedDelete {
            ctes: vec![],
            target: TableId::new(WIDGET),
            target_binding: RelationId::new(1),
            using_relations: vec![table_relation(2, OTHER, Some(alias("o", &[])))],
            predicate: Some(operator(
                11,
                EQ,
                vec![
                    column(12, RelationId::new(1), ID),
                    column(13, RelationId::new(2), OTHER_ID),
                ],
            )),
            returning: vec![projection(2, "removed", column(14, RelationId::new(1), ID))],
        })),
    };
    assert_eq!(
        render(&delete, &[]).sql,
        "DELETE FROM \"app\".\"Widget\" USING \"app\".\"other\" AS \"o\" WHERE \"app\".\"Widget\".\"id\" = \"o\".\"widget_id\" RETURNING \"app\".\"Widget\".\"id\" AS \"removed\""
    );
}

#[test]
fn bind_positions_are_declaration_ordered_and_reused() {
    let statement = select_statement(
        1,
        vec![
            projection(1, "first", parameter(1, 9)),
            projection(2, "again", parameter(2, 9)),
            projection(3, "second", parameter(3, 4)),
            projection(4, "again second", parameter(4, 4)),
        ],
        vec![],
    );
    let rendered = render(&statement, &[ParameterId::new(9), ParameterId::new(4)]);
    assert_eq!(
        rendered.sql,
        "SELECT $1 AS \"first\", $1 AS \"again\", $2 AS \"second\", $2 AS \"again second\""
    );
    assert_eq!(rendered.ordered_binds.len(), 2);

    let gap = select_statement(
        2,
        vec![
            projection(5, "third", parameter(5, 4)),
            projection(6, "third again", parameter(6, 4)),
        ],
        vec![],
    );
    let rendered = render(
        &gap,
        &[
            ParameterId::new(9),
            ParameterId::new(7),
            ParameterId::new(4),
        ],
    );
    assert_eq!(
        rendered.sql,
        "SELECT $3 AS \"third\", $3 AS \"third again\""
    );
    assert_eq!(
        rendered.ordered_binds,
        vec![OrderedBind {
            position: 3,
            parameter_id: ParameterId::new(4),
        }]
    );
}

#[test]
fn invalid_artifact_facts_fail_closed() {
    let statement = select_statement(1, vec![projection(1, "x", parameter(1, 99))], vec![]);
    assert_eq!(
        render_result(&statement, &[ParameterId::new(1)]),
        Err(SqlRenderError::UnknownParameter(ParameterId::new(99)))
    );

    let missing_name = select_statement(
        2,
        vec![projection(1, "x", column(2, RelationId::new(1), ID))],
        vec![table_relation(1, WIDGET, None)],
    );
    let mut missing_name_query = query(missing_name, &[]);
    missing_name_query.catalog_render_names =
        CatalogRenderNames::try_new(vec![CatalogRenderName::Table {
            id: TableId::new(WIDGET),
            qualified_name: vec!["app".into(), "Widget".into()],
        }])
        .unwrap();
    assert_eq!(
        render_compiled_sql(&missing_name_query),
        Err(SqlRenderError::InvalidCompiledQuery(
            dibs_query_ir::CompiledQueryError::MissingCatalogRenderName,
        ))
    );

    let duplicate = select_statement(3, vec![projection(1, "x", parameter(3, 1))], vec![]);
    let query = query(duplicate, &[ParameterId::new(1), ParameterId::new(1)]);
    assert_eq!(
        render_compiled_sql(&query),
        Err(SqlRenderError::DuplicateDeclaredParameter(
            ParameterId::new(1)
        ))
    );

    let bad_literal = select_statement(
        4,
        vec![projection(1, "bad", integer(4, "1; DROP TABLE x"))],
        vec![],
    );
    assert_eq!(
        render_result(&bad_literal, &[]),
        Err(SqlRenderError::InvalidLiteral)
    );
}

fn render(statement: &TypedStatement, parameters: &[ParameterId]) -> RenderedSql {
    render_result(statement, parameters).expect("fixture renders")
}

fn render_result(
    statement: &TypedStatement,
    parameters: &[ParameterId],
) -> Result<RenderedSql, SqlRenderError> {
    render_compiled_sql(&query(statement.clone(), parameters))
}

fn query(statement: TypedStatement, parameters: &[ParameterId]) -> CompiledQuery {
    fixture_query(statement, parameters)
}

fn fixture_query(statement: TypedStatement, parameter_ids: &[ParameterId]) -> CompiledQuery {
    use dibs_query_ir::{
        ArtifactHashes, CompilerVersions, ExecutionIdentityInput, ExecutionParameter, HirParameter,
        HirQuery, ManifestIdentity, MutationManifest, OutputField, PublicIdentityInput, QueryId,
        QueryManifest, ReadWriteLockManifest, ReferenceIndex, ResultMode, RuntimeAssertion,
        Sensitivity, SourceMap, execution_identity, public_contract_identity,
    };

    let query_id = QueryId::new(1);
    let compiler_versions = CompilerVersions {
        artifact_schema_version: 1,
        compiler_semantic_version: "fixture".to_string(),
        query_language_version: 1,
        supported_postgres_major: 18,
        execution_identity_format_version: 1,
        public_identity_format_version: 1,
        manifest_format_version: 1,
    };
    let parameters = parameter_ids
        .iter()
        .enumerate()
        .map(|(ordinal, id)| dibs_query_ir::Parameter {
            id: *id,
            ordinal: u32::try_from(ordinal).unwrap(),
            source_name: format!("p{ordinal}"),
            origin: origin(),
            type_id: TypeId::new(BIGINT),
            typmod: None,
            nullable: false,
            pg_codec_id: dibs_pg_catalog::PgCodecId::new("test"),
            wire_codec_id: dibs_pg_catalog::WireCodecId::new("test"),
            bind_format: dibs_query_ir::BindFormat::Binary,
            api_contracts: vec![],
            sensitivity: dibs_query_ir::Sensitivity::Public,
        })
        .collect::<Vec<_>>();
    let hir_parameters = parameters
        .iter()
        .map(|parameter| HirParameter {
            id: parameter.id,
            ordinal: parameter.ordinal,
            name: parameter.source_name.clone(),
            origin: parameter.origin.clone(),
            type_id: parameter.type_id.clone(),
            typmod: parameter.typmod.clone(),
            nullable: parameter.nullable,
        })
        .collect();
    let hir_statement = hir_statement(&statement);
    let schema = dibs_pg_catalog::SchemaFingerprint::from_hex_for_artifact(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let output_fields = statement_projections(&statement)
        .iter()
        .enumerate()
        .map(|(ordinal, projection)| OutputField {
            id: projection.field_id,
            ordinal: u32::try_from(ordinal).unwrap(),
            sql_label: projection.sql_label.clone(),
            public_name: projection.sql_label.clone(),
            type_id: projection.output_type_id().clone(),
            typmod: projection.output_typmod().cloned(),
            nullability: projection.output_nullability().clone(),
            pg_codec_id: dibs_pg_catalog::PgCodecId::new("test"),
            wire_codec_id: dibs_pg_catalog::WireCodecId::new("test"),
            api_types: vec![],
            api_names: vec![],
            source_expression: projection.expression.id,
            lineage_root: dibs_query_ir::LineageNodeId::new(projection.field_id.get()),
            sensitivity: Sensitivity::Public,
        })
        .collect::<Vec<_>>();
    let read_write_lock_manifest = ReadWriteLockManifest {
        reads: vec![],
        writes: vec![],
        locks: vec![],
        volatility: Volatility::Immutable,
        mutation: match &statement.kind {
            TypedStatementKind::Select(_) => None,
            TypedStatementKind::Insert(insert) => Some(MutationManifest::Insert {
                target: insert.target.clone(),
            }),
            TypedStatementKind::Update(update) => Some(MutationManifest::Update {
                target: update.target.clone(),
                has_predicate: update.predicate.is_some(),
            }),
            TypedStatementKind::Delete(delete) => Some(MutationManifest::Delete {
                target: delete.target.clone(),
                has_predicate: delete.predicate.is_some(),
            }),
        },
    };
    let (result_mode, runtime_assertions) = match &statement.kind {
        TypedStatementKind::Select(_) => (ResultMode::Many, vec![]),
        TypedStatementKind::Insert(insert) if !insert.returning.is_empty() => {
            (ResultMode::Many, vec![])
        }
        TypedStatementKind::Update(update) if !update.returning.is_empty() => {
            (ResultMode::Many, vec![])
        }
        TypedStatementKind::Delete(delete) if !delete.returning.is_empty() => {
            (ResultMode::Many, vec![])
        }
        TypedStatementKind::Insert(_)
        | TypedStatementKind::Update(_)
        | TypedStatementKind::Delete(_) => (ResultMode::Exec, vec![RuntimeAssertion::Rowless]),
    };
    let execution_semantics_id = execution_identity(&ExecutionIdentityInput {
        version: 1,
        postgres_major: 18,
        statement: statement.clone(),
        parameters: parameters
            .iter()
            .map(|parameter| ExecutionParameter {
                id: parameter.id,
                type_id: parameter.type_id.clone(),
                typmod: parameter.typmod.clone(),
                nullable: parameter.nullable,
            })
            .collect(),
        result_mode,
        runtime_assertions: runtime_assertions.clone(),
        references: ReferenceIndex::new(vec![]),
        read_write_lock_manifest: read_write_lock_manifest.clone(),
        catalog_schema_fingerprint: schema.clone(),
    });
    let public_contract_id = public_contract_identity(&PublicIdentityInput {
        version: 1,
        query_name: "Fixture".to_string(),
        operation_names: vec![],
        result_type_names: vec![],
        parameters: parameters.clone(),
        output_fields: output_fields.clone(),
        result_mode,
        transport_envelope: None,
    });
    let source_map = SourceMap::new(vec![]);
    let lineage = dibs_query_ir::LineageGraph::new(vec![], vec![]);
    let manifest_seed = QueryManifest {
        manifest_format_version: 1,
        query_id,
        execution_semantics_id: execution_semantics_id.clone(),
        public_contract_id: public_contract_id.clone(),
        compiler_versions: compiler_versions.clone(),
        catalog_schema_fingerprint: schema.clone(),
        operation_names: vec![],
        result_type_names: vec![],
        normalized_sql_hash: dibs_query_ir::ContentHash::of_bytes(b""),
        source_hash: dibs_query_ir::ContentHash::of_bytes(b"fixture"),
        source_map_hash: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
        generated_output_hashes: vec![],
        parameters: parameters.clone(),
        output_fields: output_fields.clone(),
        inferred_cardinality: statement.cardinality.clone(),
        runtime_assertions: runtime_assertions.clone(),
        relation_edges: vec![],
        cte_dependencies: vec![],
        read_write_lock_manifest: read_write_lock_manifest.clone(),
        lineage: lineage.clone(),
        opaque_analysis_boundaries: vec![],
        plan_baseline_identity: None,
    };
    let manifest_identity = ManifestIdentity::from_manifest(&manifest_seed).unwrap();
    let manifest_hash = dibs_query_ir::ContentHash::of_json(&manifest_seed).unwrap();

    CompiledQuery {
        compiler_versions,
        catalog_schema_fingerprint: schema,
        query_id,
        execution_semantics_id,
        public_contract_id,
        manifest_identity,
        query_name: "Fixture".to_string(),
        query_origin: origin(),
        declared_result_mode: result_mode,
        inferred_cardinality: statement.cardinality.clone(),
        runtime_assertions,
        deterministic_sql: String::new(),
        ordered_bind_map: vec![],
        ordered_parameters: parameters,
        ordered_output_fields: output_fields,
        catalog_render_names: render_names(),
        resolved_hir: HirQuery {
            id: query_id,
            name: "Fixture".to_string(),
            origin: origin(),
            parameters: hir_parameters,
            statement: hir_statement,
        },
        typed_statement: statement,
        resolved_references: ReferenceIndex::new(vec![]),
        lineage,
        read_write_lock_manifest,
        source_map: source_map.clone(),
        manifest: manifest_seed,
        artifact_hashes: ArtifactHashes {
            normalized_sql: dibs_query_ir::ContentHash::of_bytes(b""),
            source: dibs_query_ir::ContentHash::of_bytes(b"fixture"),
            source_map: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
            manifest: manifest_hash,
            generated_outputs: vec![],
        },
    }
    .validate()
    .unwrap()
    .to_owned()
}

fn statement_projections(statement: &TypedStatement) -> &[TypedProjection] {
    match &statement.kind {
        TypedStatementKind::Select(select) => &select.projections,
        TypedStatementKind::Insert(insert) => &insert.returning,
        TypedStatementKind::Update(update) => &update.returning,
        TypedStatementKind::Delete(delete) => &delete.returning,
    }
}

fn hir_statement(statement: &TypedStatement) -> dibs_query_ir::HirStatement {
    use dibs_query_ir::{HirStatement, HirStatementKind};

    HirStatement {
        id: statement.id,
        origin: statement.origin.clone(),
        kind: match &statement.kind {
            TypedStatementKind::Select(select) => {
                HirStatementKind::Select(Box::new(hir_select(select)))
            }
            TypedStatementKind::Insert(insert) => {
                HirStatementKind::Insert(Box::new(hir_insert(insert)))
            }
            TypedStatementKind::Update(update) => {
                HirStatementKind::Update(Box::new(hir_update(update)))
            }
            TypedStatementKind::Delete(delete) => {
                HirStatementKind::Delete(Box::new(hir_delete(delete)))
            }
        },
    }
}

fn hir_select(select: &TypedSelect) -> dibs_query_ir::HirSelect {
    dibs_query_ir::HirSelect {
        recursive: select.recursive,
        ctes: select.ctes.iter().map(hir_cte).collect(),
        distinct: match &select.distinct {
            SelectDistinct::AllRows => SelectDistinct::AllRows,
            SelectDistinct::Distinct => SelectDistinct::Distinct,
            SelectDistinct::On(expressions) => {
                SelectDistinct::On(expressions.iter().map(hir_expression).collect())
            }
        },
        projections: select.projections.iter().map(hir_projection).collect(),
        from: select.from.iter().map(hir_relation).collect(),
        predicate: select.predicate.as_ref().map(hir_expression),
        group_by: select.group_by.iter().map(hir_expression).collect(),
        having: select.having.as_ref().map(hir_expression),
        windows: select
            .windows
            .iter()
            .map(|window| dibs_query_ir::HirNamedWindow {
                name: window.name.clone(),
                origin: origin(),
                specification: hir_window_spec(&window.specification),
            })
            .collect(),
        order_by: select.order_by.iter().map(hir_order).collect(),
        limit: select.limit.as_ref().map(hir_limit),
        offset: select.offset.as_ref().map(hir_limit),
        locks: select.locks.clone(),
    }
}

fn hir_cte(cte: &TypedCte) -> dibs_query_ir::HirCte {
    dibs_query_ir::HirCte {
        id: cte.id,
        name: cte.name().to_string(),
        origin: origin(),
        materialization: cte.materialization,
        statement: Box::new(hir_statement(&cte.statement)),
    }
}

fn hir_projection(projection: &TypedProjection) -> dibs_query_ir::HirProjection {
    dibs_query_ir::HirProjection {
        field_id: projection.field_id,
        alias: projection.sql_label.clone(),
        alias_origin: origin(),
        expression: hir_expression(&projection.expression),
    }
}

fn hir_relation(relation: &TypedRelation) -> dibs_query_ir::HirRelation {
    use dibs_query_ir::HirRelationKind;

    dibs_query_ir::HirRelation {
        id: relation.id,
        origin: relation.origin.clone(),
        alias: relation.alias.clone(),
        kind: match &relation.kind {
            TypedRelationKind::Table { table_id } => HirRelationKind::Table {
                table_id: table_id.clone(),
            },
            TypedRelationKind::Cte { cte_id } => HirRelationKind::Cte { cte_id: *cte_id },
            TypedRelationKind::Subquery(statement) => {
                HirRelationKind::Subquery(Box::new(hir_statement(statement)))
            }
            TypedRelationKind::Function {
                callable_id,
                arguments,
            } => HirRelationKind::Function {
                callable_id: callable_id.clone(),
                arguments: arguments.iter().map(hir_expression).collect(),
            },
            TypedRelationKind::Join {
                kind,
                left,
                right,
                predicate,
                lateral,
            } => HirRelationKind::Join {
                kind: *kind,
                left: Box::new(hir_relation(left)),
                right: Box::new(hir_relation(right)),
                predicate: predicate.as_deref().map(hir_expression).map(Box::new),
                lateral: *lateral,
            },
            TypedRelationKind::Values { rows } => HirRelationKind::Values {
                rows: dibs_query_ir::HirValues::try_new(
                    rows.rows()
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|argument| hir_expression(&argument.expression))
                                .collect()
                        })
                        .collect(),
                )
                .unwrap(),
            },
            TypedRelationKind::SetOperation {
                kind,
                all,
                left,
                right,
            } => HirRelationKind::SetOperation {
                kind: *kind,
                all: *all,
                left: Box::new(hir_statement(left)),
                right: Box::new(hir_statement(right)),
            },
        },
    }
}

fn hir_expression(expression: &TypedExpression) -> dibs_query_ir::HirExpression {
    use dibs_query_ir::{HirCaseBranch, HirExpressionKind};

    dibs_query_ir::HirExpression {
        id: expression.id,
        origin: expression.origin.clone(),
        kind: match &expression.kind {
            TypedExpressionKind::Literal(literal) => HirExpressionKind::Literal(literal.clone()),
            TypedExpressionKind::Parameter(parameter) => HirExpressionKind::Parameter(*parameter),
            TypedExpressionKind::Column { binding, column_id } => HirExpressionKind::Column {
                binding: *binding,
                column_id: column_id.clone(),
            },
            TypedExpressionKind::Call(call) => {
                HirExpressionKind::Call(Box::new(dibs_query_ir::HirCall {
                    callable_id: call.callable_id.clone(),
                    arguments: call
                        .arguments
                        .iter()
                        .map(|argument| hir_expression(&argument.expression))
                        .collect(),
                    distinct: call.distinct,
                    star: call.star,
                    order_by: call.order_by.iter().map(hir_order).collect(),
                    filter: call.filter.as_deref().map(hir_expression).map(Box::new),
                    within_group: call.within_group.iter().map(hir_order).collect(),
                    over: call.over.as_ref().map(hir_window_reference),
                }))
            }
            TypedExpressionKind::Operator {
                authored_operator_id,
                operands,
                ..
            } => HirExpressionKind::Operator {
                operator_id: authored_operator_id.clone(),
                operands: operands
                    .iter()
                    .map(|argument| hir_expression(&argument.expression))
                    .collect(),
            },
            TypedExpressionKind::Cast {
                cast_id,
                expression,
                ..
            } => HirExpressionKind::Cast {
                cast_id: cast_id.clone(),
                expression: Box::new(hir_expression(expression)),
            },
            TypedExpressionKind::Collate {
                collation_id,
                expression,
            } => HirExpressionKind::Collate {
                collation_id: collation_id.clone(),
                expression: Box::new(hir_expression(expression)),
            },
            TypedExpressionKind::Case {
                operand,
                branches,
                else_expression,
                ..
            } => HirExpressionKind::Case {
                operand: operand.as_deref().map(hir_expression).map(Box::new),
                branches: branches
                    .iter()
                    .map(|branch| HirCaseBranch {
                        when: hir_expression(&branch.when),
                        then: hir_expression(&branch.then.expression),
                    })
                    .collect(),
                else_expression: else_expression
                    .as_deref()
                    .map(|argument| hir_expression(&argument.expression))
                    .map(Box::new),
            },
            TypedExpressionKind::ScalarSubquery(statement) => {
                HirExpressionKind::ScalarSubquery(Box::new(hir_statement(statement)))
            }
            TypedExpressionKind::Row(values) => {
                HirExpressionKind::Row(values.iter().map(hir_expression).collect())
            }
            TypedExpressionKind::Array { elements, .. } => HirExpressionKind::Array(
                elements
                    .iter()
                    .map(|argument| hir_expression(&argument.expression))
                    .collect(),
            ),
            TypedExpressionKind::CteColumn { cte_id, field_id } => HirExpressionKind::CteColumn {
                cte_id: *cte_id,
                field_id: *field_id,
            },
        },
    }
}

fn hir_order(order: &TypedOrderBy) -> dibs_query_ir::HirOrderBy {
    dibs_query_ir::HirOrderBy {
        expression: hir_expression(&order.expression),
        direction: order.direction,
        nulls: order.nulls,
    }
}

fn hir_window_reference(
    window: &WindowReference<TypedExpression>,
) -> WindowReference<dibs_query_ir::HirExpression> {
    match window {
        WindowReference::Named(name) => WindowReference::Named(name.clone()),
        WindowReference::Inline(specification) => {
            WindowReference::Inline(hir_window_spec(specification))
        }
    }
}

fn hir_window_spec(
    specification: &WindowSpec<TypedExpression>,
) -> WindowSpec<dibs_query_ir::HirExpression> {
    WindowSpec {
        existing: specification.existing.clone(),
        partition_by: specification
            .partition_by
            .iter()
            .map(hir_expression)
            .collect(),
        order_by: specification.order_by.iter().map(hir_order).collect(),
        frame: specification.frame.as_ref().map(hir_window_frame),
    }
}

fn hir_window_frame(
    frame: &WindowFrame<TypedExpression>,
) -> WindowFrame<dibs_query_ir::HirExpression> {
    WindowFrame {
        mode: frame.mode,
        start: hir_frame_bound(&frame.start),
        end: frame.end.as_ref().map(hir_frame_bound),
        exclusion: frame.exclusion,
    }
}

fn hir_frame_bound(
    bound: &FrameBound<TypedExpression>,
) -> FrameBound<dibs_query_ir::HirExpression> {
    match bound {
        FrameBound::UnboundedPreceding => FrameBound::UnboundedPreceding,
        FrameBound::Preceding(expression) => FrameBound::Preceding(hir_expression(expression)),
        FrameBound::CurrentRow => FrameBound::CurrentRow,
        FrameBound::Following(expression) => FrameBound::Following(hir_expression(expression)),
        FrameBound::UnboundedFollowing => FrameBound::UnboundedFollowing,
    }
}

fn hir_limit(limit: &TypedLimit) -> dibs_query_ir::HirExpression {
    match limit {
        TypedLimit::Constant(value) => dibs_query_ir::HirExpression {
            id: ExpressionId::new(900_000 + u32::try_from(*value).unwrap_or(u32::MAX)),
            origin: origin(),
            kind: dibs_query_ir::HirExpressionKind::Literal(HirLiteral::Integer(value.to_string())),
        },
        TypedLimit::Parameter(parameter) => dibs_query_ir::HirExpression {
            id: ExpressionId::new(900_001 + parameter.get()),
            origin: origin(),
            kind: dibs_query_ir::HirExpressionKind::Parameter(*parameter),
        },
    }
}

fn hir_insert(insert: &TypedInsert) -> dibs_query_ir::HirInsert {
    use dibs_query_ir::{HirConflictAction, HirConflictClause, HirConflictTarget, HirInsertSource};

    dibs_query_ir::HirInsert {
        ctes: insert.ctes.iter().map(hir_cte).collect(),
        target: insert.target.clone(),
        target_binding: insert.target_binding,
        columns: insert.columns.clone(),
        source: match &insert.source {
            TypedInsertSource::Values(values) => HirInsertSource::Values(
                dibs_query_ir::HirValues::try_new(
                    values
                        .rows()
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|argument| hir_expression(&argument.expression))
                                .collect()
                        })
                        .collect(),
                )
                .unwrap(),
            ),
            TypedInsertSource::Select(statement) => {
                HirInsertSource::Select(Box::new(hir_statement(statement)))
            }
            TypedInsertSource::DefaultValues => HirInsertSource::DefaultValues,
        },
        conflict: insert.conflict.as_ref().map(|conflict| HirConflictClause {
            target: match &conflict.target {
                ConflictTarget::Constraint(constraint) => {
                    HirConflictTarget::Constraint(constraint.clone())
                }
                ConflictTarget::Inference {
                    expressions,
                    predicate,
                } => HirConflictTarget::Inference {
                    expressions: expressions.iter().map(hir_expression).collect(),
                    predicate: predicate.as_deref().map(hir_expression).map(Box::new),
                },
                ConflictTarget::Unspecified => HirConflictTarget::Unspecified,
            },
            action: match &conflict.action {
                TypedConflictAction::Nothing => HirConflictAction::Nothing,
                TypedConflictAction::Update {
                    assignments,
                    predicate,
                } => HirConflictAction::Update {
                    assignments: assignments.iter().map(hir_assignment).collect(),
                    predicate: predicate.as_deref().map(hir_expression),
                },
            },
        }),
        returning: insert.returning.iter().map(hir_projection).collect(),
    }
}

fn hir_update(update: &TypedUpdate) -> dibs_query_ir::HirUpdate {
    dibs_query_ir::HirUpdate {
        ctes: update.ctes.iter().map(hir_cte).collect(),
        target: update.target.clone(),
        target_binding: update.target_binding,
        assignments: update.assignments.iter().map(hir_assignment).collect(),
        from: update.from.iter().map(hir_relation).collect(),
        predicate: update.predicate.as_ref().map(hir_expression),
        returning: update.returning.iter().map(hir_projection).collect(),
    }
}

fn hir_delete(delete: &TypedDelete) -> dibs_query_ir::HirDelete {
    dibs_query_ir::HirDelete {
        ctes: delete.ctes.iter().map(hir_cte).collect(),
        target: delete.target.clone(),
        target_binding: delete.target_binding,
        using_relations: delete.using_relations.iter().map(hir_relation).collect(),
        predicate: delete.predicate.as_ref().map(hir_expression),
        returning: delete.returning.iter().map(hir_projection).collect(),
    }
}

fn hir_assignment(assignment: &TypedAssignment) -> dibs_query_ir::HirAssignment {
    dibs_query_ir::HirAssignment {
        id: assignment.id,
        target: assignment.target.clone(),
        value: hir_expression(&assignment.value),
    }
}

fn render_names() -> CatalogRenderNames {
    CatalogRenderNames::try_new(vec![
        CatalogRenderName::Table {
            id: TableId::new(WIDGET),
            qualified_name: vec!["app".into(), "Widget".into()],
        },
        CatalogRenderName::Table {
            id: TableId::new(OTHER),
            qualified_name: vec!["app".into(), "other".into()],
        },
        CatalogRenderName::Column {
            id: ColumnId::new(ID),
            name: "id".into(),
        },
        CatalogRenderName::Column {
            id: ColumnId::new(NAME),
            name: "display name".into(),
        },
        CatalogRenderName::Column {
            id: ColumnId::new(OTHER_ID),
            name: "widget_id".into(),
        },
        CatalogRenderName::Callable {
            id: CallableId::new(COUNT),
            qualified_name: vec!["pg_catalog".into(), "count".into()],
        },
        CatalogRenderName::Callable {
            id: CallableId::new(GENERATE_SERIES),
            qualified_name: vec!["pg_catalog".into(), "generate_series".into()],
        },
        CatalogRenderName::Operator {
            id: OperatorId::new(ADD),
            qualified_name: vec!["pg_catalog".into(), "+".into()],
        },
        CatalogRenderName::Operator {
            id: OperatorId::new(EQ),
            qualified_name: vec!["pg_catalog".into(), "=".into()],
        },
        CatalogRenderName::Operator {
            id: OperatorId::new(NOT),
            qualified_name: vec!["pg_catalog".into(), "NOT".into()],
        },
        CatalogRenderName::Operator {
            id: OperatorId::new(IS_NULL),
            qualified_name: vec!["pg_catalog".into(), "IS NULL".into()],
        },
        CatalogRenderName::Type {
            id: TypeId::new(BIGINT),
            qualified_name: vec!["pg_catalog".into(), "int8".into()],
        },
        CatalogRenderName::Type {
            id: TypeId::new(TEXT),
            qualified_name: vec!["pg_catalog".into(), "text".into()],
        },
        CatalogRenderName::Type {
            id: TypeId::new(BOOL),
            qualified_name: vec!["pg_catalog".into(), "bool".into()],
        },
        CatalogRenderName::Collation {
            id: CollationId::new(C_COLLATION),
            qualified_name: vec!["pg_catalog".into(), "C".into()],
        },
        CatalogRenderName::Constraint {
            id: ConstraintId::new(UNIQUE_NAME),
            name: "Widget_name_key".into(),
        },
    ])
    .unwrap()
}

fn select_statement(
    id: u32,
    projections: Vec<TypedProjection>,
    from: Vec<TypedRelation>,
) -> TypedStatement {
    TypedStatement {
        id: StatementId::new(id),
        origin: origin(),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: false,
            ctes: vec![],
            distinct: SelectDistinct::AllRows,
            projections,
            from,
            predicate: None,
            group_by: vec![],
            having: None,
            windows: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            locks: vec![],
        })),
    }
}

fn update_statement(id: u32, returning: Vec<TypedProjection>) -> TypedStatement {
    TypedStatement {
        id: StatementId::new(id),
        origin: origin(),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Update(Box::new(TypedUpdate {
            ctes: vec![],
            target: TableId::new(WIDGET),
            target_binding: RelationId::new(1),
            assignments: vec![assignment(1, NAME, integer(1, "1"), None)],
            from: vec![],
            predicate: None,
            returning,
        })),
    }
}

fn typed_cte(
    id: u32,
    name: &str,
    materialization: CteMaterialization,
    statement: TypedStatement,
) -> TypedCte {
    let (output_fields, output_names) = {
        let projections = match &statement.kind {
            TypedStatementKind::Select(select) => &select.projections,
            TypedStatementKind::Insert(insert) => &insert.returning,
            TypedStatementKind::Update(update) => &update.returning,
            TypedStatementKind::Delete(delete) => &delete.returning,
        };
        (
            projections
                .iter()
                .map(|projection| projection.field_id)
                .collect(),
            projections
                .iter()
                .map(|projection| projection.sql_label.clone())
                .collect(),
        )
    };
    TypedCte::try_new(
        CteId::new(id),
        name.to_string(),
        materialization,
        Box::new(statement),
        output_fields,
        output_names,
    )
    .unwrap()
}

fn projection(id: u32, label: &str, expression: TypedExpression) -> TypedProjection {
    TypedProjection {
        field_id: FieldId::new(id),
        sql_label: label.to_string(),
        expression,
        coercion: None,
    }
}

fn typed_values(rows: Vec<Vec<TypedExpression>>) -> TypedValues {
    let arguments = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|expression| TypedArgument {
                    expression,
                    coercion: None,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let width = arguments[0].len();
    let columns = (0..width)
        .map(|column| {
            let first = &arguments[0][column].expression;
            let nullable = arguments
                .iter()
                .any(|row| row[column].expression.nullability.is_nullable());
            TypedValuesColumn {
                type_id: first.type_id.clone(),
                typmod: first.typmod.clone(),
                nullability: if nullable {
                    Nullability::nullable(NullabilityEvidence::ValuesPropagation)
                } else {
                    Nullability::not_null(NullabilityEvidence::ValuesPropagation)
                },
                common_type: CoercionEvidence::CommonType {
                    resolved: first.type_id.clone(),
                    inputs: arguments
                        .iter()
                        .map(|row| row[column].expression.type_id.clone())
                        .collect(),
                },
            }
        })
        .collect();
    TypedValues::try_new(arguments, columns).unwrap()
}

fn table_relation(id: u32, table: &str, alias: Option<RelationAlias>) -> TypedRelation {
    TypedRelation {
        id: RelationId::new(id),
        origin: origin(),
        alias,
        cardinality: Cardinality::many(),
        kind: TypedRelationKind::Table {
            table_id: TableId::new(table),
        },
    }
}

fn alias(name: &str, columns: &[&str]) -> RelationAlias {
    RelationAlias {
        name: name.to_string(),
        column_names: columns.iter().map(|name| (*name).to_string()).collect(),
    }
}

fn expression(id: u32, ty: &str, kind: TypedExpressionKind) -> TypedExpression {
    TypedExpression {
        id: ExpressionId::new(id),
        origin: origin(),
        type_id: TypeId::new(ty),
        typmod: None,
        nullability: Nullability::nullable(NullabilityEvidence::Conservative),
        volatility: Volatility::Immutable,
        kind,
    }
}

fn integer(id: u32, value: &str) -> TypedExpression {
    expression(
        id,
        BIGINT,
        TypedExpressionKind::Literal(HirLiteral::Integer(value.to_string())),
    )
}
fn numeric(id: u32, value: &str) -> TypedExpression {
    expression(
        id,
        BIGINT,
        TypedExpressionKind::Literal(HirLiteral::Numeric(value.to_string())),
    )
}
fn string(id: u32, value: &str) -> TypedExpression {
    expression(
        id,
        TEXT,
        TypedExpressionKind::Literal(HirLiteral::String(value.to_string())),
    )
}
fn bytes(id: u32, value: &[u8]) -> TypedExpression {
    expression(
        id,
        TEXT,
        TypedExpressionKind::Literal(HirLiteral::Bytes(value.to_vec())),
    )
}
fn null(id: u32) -> TypedExpression {
    expression(id, TEXT, TypedExpressionKind::Literal(HirLiteral::Null))
}
fn boolean(id: u32, value: bool) -> TypedExpression {
    expression(
        id,
        BOOL,
        TypedExpressionKind::Literal(HirLiteral::Boolean(value)),
    )
}
fn parameter(id: u32, parameter: u32) -> TypedExpression {
    expression(
        id,
        BIGINT,
        TypedExpressionKind::Parameter(ParameterId::new(parameter)),
    )
}
fn column(id: u32, binding: RelationId, column: &str) -> TypedExpression {
    expression(
        id,
        BIGINT,
        TypedExpressionKind::Column {
            binding,
            column_id: ColumnId::new(column),
        },
    )
}

fn operator(id: u32, operator_id: &str, operands: Vec<TypedExpression>) -> TypedExpression {
    expression(
        id,
        BOOL,
        TypedExpressionKind::Operator {
            authored_operator_id: OperatorId::new(operator_id),
            operator_id: OperatorId::new(operator_id),
            operands: operands
                .into_iter()
                .map(|expression| TypedArgument {
                    expression,
                    coercion: None,
                })
                .collect(),
        },
    )
}

fn collate(id: u32, value: TypedExpression) -> TypedExpression {
    expression(
        id,
        TEXT,
        TypedExpressionKind::Collate {
            collation_id: CollationId::new(C_COLLATION),
            expression: Box::new(value),
        },
    )
}

fn cast_to_text(id: u32, value: TypedExpression) -> TypedExpression {
    expression(
        id,
        TEXT,
        TypedExpressionKind::Cast {
            cast_id: CastId::new(CAST_TEXT),
            expression: Box::new(value),
            coercion: text_coercion(),
        },
    )
}

fn text_coercion() -> TypedCoercion {
    TypedCoercion {
        source_type: TypeId::new(BIGINT),
        target_type: TypeId::new(TEXT),
        target_typmod: None,
        result_nullability: Nullability::nullable(NullabilityEvidence::CastPropagation),
        evidence: CoercionEvidence::CatalogCastPath {
            steps: vec![TypedCastStep {
                cast_id: CastId::new(CAST_TEXT),
                source_type: TypeId::new(BIGINT),
                target_type: TypeId::new(TEXT),
                context: CoercionContext::Explicit,
            }],
        },
    }
}

fn assignment(
    id: u32,
    column: &str,
    value: TypedExpression,
    coercion: Option<TypedCoercion>,
) -> TypedAssignment {
    TypedAssignment {
        id: dibs_query_ir::AssignmentId::new(id),
        target: ColumnId::new(column),
        value,
        coercion,
    }
}

fn order(expression: TypedExpression, direction: SortDirection, nulls: NullsOrder) -> TypedOrderBy {
    TypedOrderBy {
        expression,
        direction,
        nulls,
    }
}
fn origin() -> SourceOrigin {
    SourceOrigin::authored(SourceSpan::new(SourceId::new(1), Span::new(0, 1)))
}
