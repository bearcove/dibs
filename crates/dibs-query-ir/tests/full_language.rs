use dibs_pg_catalog::{
    CallableId, CollationId, ColumnId, ConstraintId, OperatorId, TableId, TypeId,
};
use dibs_query_ir::{
    Cardinality, CatalogRenderName, CatalogRenderNames, CteId, CteMaterialization, ExpressionId,
    FieldId, FrameBound, HirCall, HirCte, HirExpression, HirExpressionKind, HirLiteral, HirOrderBy,
    HirProjection, HirSelect, HirStatement, HirStatementKind, Nullability, NullabilityEvidence,
    NullsOrder, ParameterId, RelationId, SelectDistinct, SortDirection, SourceOrigin, SourceSpan,
    Span, StatementId, TypedArgument, TypedCall, TypedCte, TypedExpression, TypedExpressionKind,
    TypedOrderBy, TypedProjection, TypedSelect, TypedShapeError, TypedStatement,
    TypedStatementKind, TypedWithinGroupOrderBy, Volatility, WindowExclusion, WindowFrame,
    WindowFrameMode, WindowReference, WindowSpec,
};
use dibs_query_syntax::SourceId;

fn origin(start: u32, end: u32) -> SourceOrigin {
    SourceOrigin::authored(SourceSpan::new(SourceId::new(1), Span::new(start, end)))
}

fn type_id() -> TypeId {
    TypeId::new("pg18:type:pg_catalog.bigint:base")
}

fn hir_integer(id: u32, value: &str) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(id),
        origin: origin(id, id + 1),
        kind: HirExpressionKind::Literal(HirLiteral::Integer(value.to_string())),
    }
}

fn typed_integer(id: u32, value: &str) -> TypedExpression {
    TypedExpression {
        id: ExpressionId::new(id),
        origin: origin(id, id + 1),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::CallableContract {
            callable_id: CallableId::new("pg18:literal:integer"),
            proves_non_null: true,
        }),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Literal(HirLiteral::Integer(value.to_string())),
    }
}

fn typed_position() -> TypedExpression {
    let input_type = TypeId::new("pg18:type:base:pg_catalog.bytea");
    let argument = |id| TypedArgument {
        expression: TypedExpression {
            id: ExpressionId::new(id),
            origin: origin(id, id + 1),
            type_id: input_type.clone(),
            typmod: None,
            nullability: Nullability::not_null(NullabilityEvidence::CallableContract {
                callable_id: CallableId::new("pg18:syntax:position"),
                proves_non_null: true,
            }),
            volatility: Volatility::Immutable,
            kind: TypedExpressionKind::Parameter(ParameterId::new(id)),
        },
        coercion: None,
    };
    TypedExpression {
        id: ExpressionId::new(40),
        origin: origin(40, 50),
        type_id: TypeId::new("pg18:type:base:pg_catalog.integer"),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::CallableContract {
            callable_id: CallableId::new("pg18:syntax:position"),
            proves_non_null: true,
        }),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Position {
            substring: Box::new(argument(41)),
            string: Box::new(argument(42)),
            input_type,
        },
    }
}

#[test]
fn position_shape_rejects_wrong_result_input_and_typmod() {
    let mut wrong_result = typed_position();
    wrong_result.type_id = TypeId::new("pg18:type:base:pg_catalog.bigint");
    assert_eq!(wrong_result.validate(), Err(TypedShapeError::Expression));

    let mut wrong_typmod = typed_position();
    wrong_typmod.typmod = Some(dibs_query_ir::Typmod::new("1"));
    assert_eq!(wrong_typmod.validate(), Err(TypedShapeError::Expression));

    let mut wrong_input = typed_position();
    let TypedExpressionKind::Position { input_type, .. } = &mut wrong_input.kind else {
        unreachable!()
    };
    *input_type = TypeId::new("pg18:type:base:pg_catalog.integer");
    assert_eq!(wrong_input.validate(), Err(TypedShapeError::Expression));
}

fn hir_order(id: u32) -> HirOrderBy {
    HirOrderBy {
        expression: hir_integer(id, "1"),
        direction: SortDirection::Descending,
        nulls: NullsOrder::Last,
    }
}

fn typed_order(id: u32) -> TypedOrderBy {
    TypedOrderBy {
        expression: typed_integer(id, "1"),
        direction: SortDirection::Descending,
        nulls: NullsOrder::Last,
    }
}

fn typed_within_group_order(id: u32) -> TypedWithinGroupOrderBy {
    TypedWithinGroupOrderBy {
        expression: TypedArgument {
            expression: typed_integer(id, "1"),
            coercion: None,
        },
        direction: SortDirection::Descending,
        nulls: NullsOrder::Last,
    }
}

fn window_spec_hir() -> WindowSpec<HirExpression> {
    WindowSpec {
        existing: Some("job_order".to_string()),
        partition_by: vec![hir_integer(20, "1")],
        order_by: vec![hir_order(21)],
        frame: Some(WindowFrame {
            mode: WindowFrameMode::Groups,
            start: FrameBound::UnboundedPreceding,
            end: Some(FrameBound::Following(hir_integer(22, "1"))),
            exclusion: WindowExclusion::Ties,
        }),
    }
}

fn window_spec_typed() -> WindowSpec<TypedExpression> {
    WindowSpec {
        existing: Some("job_order".to_string()),
        partition_by: vec![typed_integer(20, "1")],
        order_by: vec![typed_order(21)],
        frame: Some(WindowFrame {
            mode: WindowFrameMode::Groups,
            start: FrameBound::UnboundedPreceding,
            end: Some(FrameBound::Following(typed_integer(22, "1"))),
            exclusion: WindowExclusion::Ties,
        }),
    }
}

#[test]
fn full_select_and_call_vocabulary_round_trips_with_facet_json() {
    let call = TypedExpression {
        id: ExpressionId::new(10),
        origin: origin(10, 30),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::CallableContract {
            callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            proves_non_null: true,
        }),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Call(Box::new(TypedCall {
            authored_callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            arguments: Vec::new(),
            argument_names: Vec::new(),
            distinct: true,
            star: false,
            order_by: Vec::new(),
            filter: Some(Box::new(typed_integer(13, "1"))),
            within_group: Vec::new(),
            over: Some(WindowReference::Inline(window_spec_typed())),
        })),
    };
    let select = TypedSelect {
        recursive: true,
        ctes: Vec::new(),
        distinct: SelectDistinct::On(vec![typed_integer(15, "1")]),
        projections: vec![TypedProjection {
            field_id: FieldId::new(1),
            sql_label: "total".to_string(),
            expression: call,
            coercion: None,
        }],
        from: Vec::new(),
        predicate: None,
        group_by: Vec::new(),
        having: None,
        windows: vec![dibs_query_ir::TypedNamedWindow {
            name: "job_order".to_string(),
            specification: window_spec_typed(),
        }],
        order_by: Vec::new(),
        limit: None,
        offset: None,
        locks: Vec::new(),
    };

    let json = facet_json::to_string(&select).unwrap();
    let decoded: TypedSelect = facet_json::from_str(&json).unwrap();
    assert_eq!(decoded, select);
    assert!(decoded.validate().is_ok());
}

#[test]
fn checked_typed_ir_rejects_full_language_hir_divergence() {
    let hir_call = HirExpression {
        id: ExpressionId::new(10),
        origin: origin(10, 30),
        kind: HirExpressionKind::Call(Box::new(HirCall {
            callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            arguments: Vec::new(),
            argument_names: Vec::new(),
            distinct: true,
            star: false,
            order_by: vec![hir_order(12)],
            filter: Some(Box::new(hir_integer(13, "1"))),
            within_group: vec![hir_order(14)],
            over: Some(WindowReference::Inline(window_spec_hir())),
        })),
    };
    let hir = HirStatement {
        id: StatementId::new(1),
        origin: origin(0, 40),
        kind: HirStatementKind::Select(Box::new(HirSelect {
            recursive: true,
            ctes: Vec::new(),
            distinct: SelectDistinct::On(vec![hir_integer(15, "1")]),
            projections: vec![HirProjection {
                field_id: FieldId::new(1),
                alias: "total".to_string(),
                alias_origin: origin(31, 36),
                expression: hir_call,
            }],
            from: Vec::new(),
            predicate: None,
            group_by: Vec::new(),
            having: None,
            windows: vec![dibs_query_ir::HirNamedWindow {
                name: "job_order".to_string(),
                origin: origin(20, 30),
                specification: window_spec_hir(),
            }],
            order_by: Vec::new(),
            limit: None,
            offset: None,
            locks: Vec::new(),
        })),
    };
    let mut typed = TypedStatement {
        id: StatementId::new(1),
        origin: origin(0, 40),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: true,
            ctes: Vec::new(),
            distinct: SelectDistinct::On(vec![typed_integer(15, "1")]),
            projections: vec![TypedProjection {
                field_id: FieldId::new(1),
                sql_label: "total".to_string(),
                expression: TypedExpression {
                    id: ExpressionId::new(10),
                    origin: origin(10, 30),
                    type_id: type_id(),
                    typmod: None,
                    nullability: Nullability::not_null(NullabilityEvidence::CallableContract {
                        callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
                        proves_non_null: true,
                    }),
                    volatility: Volatility::Immutable,
                    kind: TypedExpressionKind::Call(Box::new(TypedCall {
                        authored_callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
                        callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
                        arguments: Vec::new(),
                        argument_names: Vec::new(),
                        distinct: true,
                        star: false,
                        order_by: vec![typed_order(12)],
                        filter: Some(Box::new(typed_integer(13, "1"))),
                        within_group: vec![typed_within_group_order(14)],
                        over: Some(WindowReference::Inline(window_spec_typed())),
                    })),
                },
                coercion: None,
            }],
            from: Vec::new(),
            predicate: None,
            group_by: Vec::new(),
            having: None,
            windows: vec![dibs_query_ir::TypedNamedWindow {
                name: "job_order".to_string(),
                specification: window_spec_typed(),
            }],
            order_by: Vec::new(),
            limit: None,
            offset: None,
            locks: Vec::new(),
        })),
    };
    assert!(typed.corresponds_to_hir(&hir));

    let TypedStatementKind::Select(select) = &mut typed.kind else {
        unreachable!()
    };
    select.recursive = false;
    assert!(!typed.corresponds_to_hir(&hir));
}

#[test]
fn cte_materialization_recursive_flag_and_render_names_are_artifact_owned() {
    let cte_statement = Box::new(TypedStatement {
        id: StatementId::new(2),
        origin: origin(0, 1),
        cardinality: Cardinality::many(),
        kind: TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: SelectDistinct::AllRows,
            projections: vec![TypedProjection {
                field_id: FieldId::new(2),
                sql_label: "job_id".to_string(),
                expression: typed_integer(2, "1"),
                coercion: None,
            }],
            from: Vec::new(),
            predicate: None,
            group_by: Vec::new(),
            having: None,
            windows: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            locks: Vec::new(),
        })),
    });
    let cte = TypedCte::try_new(
        CteId::new(1),
        false,
        "dependency_path".to_string(),
        CteMaterialization::Materialized,
        cte_statement,
        vec![FieldId::new(2)],
        vec!["job_id".to_string()],
    )
    .unwrap();
    assert_eq!(cte.name(), "dependency_path");
    assert_eq!(cte.output_names(), ["job_id"]);

    let hir = HirCte {
        id: CteId::new(1),
        recursive: false,
        name: "dependency_path".to_string(),
        origin: origin(0, 10),
        materialization: CteMaterialization::Materialized,
        statement: Box::new(HirStatement {
            id: StatementId::new(2),
            origin: origin(0, 1),
            kind: HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: vec![HirProjection {
                    field_id: FieldId::new(2),
                    alias: "job_id".to_string(),
                    alias_origin: origin(0, 1),
                    expression: hir_integer(2, "1"),
                }],
                from: Vec::new(),
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: None,
                offset: None,
                locks: Vec::new(),
            })),
        }),
    };
    assert_eq!(cte.name(), hir.name);
}

#[test]
fn catalog_render_names_cover_every_rendered_catalog_identity_canonically() {
    let names = CatalogRenderNames::try_new(vec![
        CatalogRenderName::Type {
            id: type_id(),
            qualified_name: vec!["pg_catalog".to_string(), "int8".to_string()],
        },
        CatalogRenderName::Table {
            id: TableId::new("pg18:table:tenant.Job"),
            qualified_name: vec!["tenant".to_string(), "Job".to_string()],
        },
        CatalogRenderName::Column {
            id: ColumnId::new("pg18:column:tenant.Job.job ID"),
            name: "job ID".to_string(),
        },
        CatalogRenderName::Callable {
            id: CallableId::new("pg18:callable:tenant.do_work"),
            qualified_name: vec!["tenant".to_string(), "do_work".to_string()],
        },
        CatalogRenderName::Operator {
            id: OperatorId::new("pg18:operator:pg_catalog.+"),
            qualified_name: vec!["pg_catalog".to_string(), "+".to_string()],
        },
        CatalogRenderName::Collation {
            id: CollationId::new("pg18:collation:pg_catalog.C"),
            qualified_name: vec!["pg_catalog".to_string(), "C".to_string()],
        },
        CatalogRenderName::Constraint {
            id: ConstraintId::new("pg18:constraint:tenant.Job:Job_pkey"),
            name: "Job_pkey".to_string(),
        },
    ])
    .unwrap();

    assert_eq!(
        names.table(&TableId::new("pg18:table:tenant.Job")).unwrap(),
        ["tenant", "Job"]
    );
    assert_eq!(
        names
            .column(&ColumnId::new("pg18:column:tenant.Job.job ID"))
            .unwrap(),
        "job ID"
    );
    assert_eq!(
        names
            .operator(&OperatorId::new("pg18:operator:pg_catalog.+"))
            .unwrap(),
        ["pg_catalog", "+"]
    );

    let reversed =
        CatalogRenderNames::try_new(names.entries().iter().cloned().rev().collect()).unwrap();
    assert_eq!(
        facet_json::to_string(&names).unwrap(),
        facet_json::to_string(&reversed).unwrap()
    );
}

#[test]
fn catalog_render_names_reject_duplicates_and_invalid_components() {
    let table = CatalogRenderName::Table {
        id: TableId::new("pg18:table:public.job"),
        qualified_name: vec!["public".to_string(), "job".to_string()],
    };
    assert!(CatalogRenderNames::try_new(vec![table.clone(), table]).is_err());
    assert!(
        CatalogRenderNames::try_new(vec![CatalogRenderName::Callable {
            id: CallableId::new("pg18:callable:broken"),
            qualified_name: Vec::new(),
        }])
        .is_err()
    );
}

#[test]
fn typed_relations_keep_authored_aliases_for_rendering() {
    let relation = dibs_query_ir::TypedRelation {
        id: RelationId::new(1),
        origin: origin(0, 1),
        alias: Some(dibs_query_ir::RelationAlias {
            name: "retry".to_string(),
            column_names: vec!["intent_id".to_string(), "coalesced".to_string()],
        }),
        cardinality: Cardinality::many(),
        kind: dibs_query_ir::TypedRelationKind::Table {
            table_id: TableId::new("pg18:table:public.job"),
        },
    };
    assert_eq!(relation.alias.unwrap().column_names.len(), 2);
}

#[test]
fn typed_render_names_and_lock_targets_are_checked() {
    let invalid_alias = dibs_query_ir::TypedRelation {
        id: RelationId::new(1),
        origin: origin(0, 1),
        alias: Some(dibs_query_ir::RelationAlias {
            name: "bad\0alias".to_string(),
            column_names: Vec::new(),
        }),
        cardinality: Cardinality::many(),
        kind: dibs_query_ir::TypedRelationKind::Table {
            table_id: TableId::new("pg18:table:public.job"),
        },
    };
    let select = TypedSelect {
        recursive: false,
        ctes: Vec::new(),
        distinct: SelectDistinct::AllRows,
        projections: vec![TypedProjection {
            field_id: FieldId::new(1),
            sql_label: "bad\0label".to_string(),
            expression: typed_integer(1, "1"),
            coercion: None,
        }],
        from: vec![invalid_alias],
        predicate: None,
        group_by: Vec::new(),
        having: None,
        windows: Vec::new(),
        order_by: Vec::new(),
        limit: None,
        offset: None,
        locks: vec![dibs_query_ir::HirLockClause {
            strength: dibs_query_ir::LockStrength::Update,
            targets: vec![RelationId::new(99)],
            wait: dibs_query_ir::LockWaitPolicy::Wait,
        }],
    };
    assert!(select.validate().is_err());

    let mut valid_names = select;
    valid_names.projections[0].sql_label = "value".to_string();
    valid_names.from[0].alias.as_mut().unwrap().name = "job".to_string();
    assert!(valid_names.validate().is_err());
    valid_names.locks[0].targets = vec![RelationId::new(1)];
    assert!(valid_names.validate().is_ok());
}

#[test]
fn invalid_window_and_call_shapes_are_rejected() {
    let invalid_frame = WindowFrame {
        mode: WindowFrameMode::Rows,
        start: FrameBound::Following(typed_integer(1, "1")),
        end: Some(FrameBound::Preceding(typed_integer(2, "1"))),
        exclusion: WindowExclusion::None,
    };
    let call = TypedExpression {
        id: ExpressionId::new(10),
        origin: origin(10, 20),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::CallableContract {
            callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            proves_non_null: true,
        }),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Call(Box::new(TypedCall {
            authored_callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            callable_id: CallableId::new("pg18:callable:pg_catalog.count(*)"),
            arguments: vec![TypedArgument {
                expression: typed_integer(3, "1"),
                coercion: None,
            }],
            argument_names: vec![None],
            distinct: true,
            star: true,
            order_by: Vec::new(),
            filter: None,
            within_group: Vec::new(),
            over: Some(WindowReference::Inline(WindowSpec {
                existing: None,
                partition_by: Vec::new(),
                order_by: Vec::new(),
                frame: Some(invalid_frame),
            })),
        })),
    };
    let mut unsafe_name = call.clone();
    let TypedExpressionKind::Call(unsafe_call) = &mut unsafe_name.kind else {
        unreachable!()
    };
    unsafe_call.star = false;
    unsafe_call.argument_names = vec![Some("secs); DROP TABLE job; --".to_string())];
    assert!(unsafe_name.validate().is_err());

    let mut invalid_order = call.clone();
    let TypedExpressionKind::Call(invalid_call) = &mut invalid_order.kind else {
        unreachable!()
    };
    invalid_call.star = false;
    invalid_call.arguments.push(TypedArgument {
        expression: typed_integer(4, "2"),
        coercion: None,
    });
    invalid_call.argument_names = vec![Some("first".to_string()), None];
    assert!(invalid_order.validate().is_err());

    let mut duplicate_name = call.clone();
    let TypedExpressionKind::Call(duplicate_call) = &mut duplicate_name.kind else {
        unreachable!()
    };
    duplicate_call.star = false;
    duplicate_call.arguments.push(TypedArgument {
        expression: typed_integer(5, "2"),
        coercion: None,
    });
    duplicate_call.argument_names = vec![Some("value".to_string()), Some("value".to_string())];
    assert!(duplicate_name.validate().is_err());
    assert!(call.validate().is_err());
}

#[test]
fn named_window_references_remain_distinct_from_inline_specs() {
    assert_ne!(
        WindowReference::<TypedExpression>::Named("job_order".to_string()),
        WindowReference::Inline(WindowSpec {
            existing: Some("job_order".to_string()),
            partition_by: Vec::new(),
            order_by: Vec::new(),
            frame: None,
        })
    );
    assert_eq!(ParameterId::new(1).get(), 1);
}
