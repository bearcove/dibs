use dibs_db_schema::{Column, PgType, Schema, SourceLocation, Table};
use dibs_pg_catalog::{
    CallableCardinality, CallableId, CallableKind, CatalogCallable, CatalogSnapshot,
    DomainCollation, OperatorId, TypeRegistration, TypeRegistrationKind,
};
use dibs_query_ir::{
    CardinalityEvidence, CoercionEvidence, CteId, CteMaterialization, ExpressionId, FieldId,
    HirCall, HirCaseBranch, HirCte, HirExpression, HirExpressionKind, HirLiteral, HirParameter,
    HirProjection, HirQuery, HirRelation, HirRelationKind, HirSelect, HirStatement,
    HirStatementKind, LowerBound, NullabilityEvidence, ParameterId, QueryId, RelationId,
    ResultMode, SelectDistinct, SourceOrigin, StatementId, TypedExpressionKind, TypedRelationKind,
    TypedStatementKind, UpperBound,
};
use dibs_query_typing::{
    CardinalityModeError, CheckError, CheckedOutput, SemanticChecker, TypeResolutionError,
};

fn origin() -> SourceOrigin {
    SourceOrigin {
        primary: None,
        related: Vec::new(),
        generated: Some(dibs_query_ir::GeneratedOrigin::Structural),
    }
}

fn column(name: &str, pg_type: PgType, nullable: bool) -> Column {
    Column {
        name: name.to_string(),
        pg_type,
        rust_type: Some(pg_type.to_rust_type().to_string()),
        nullable,
        default: None,
        primary_key: false,
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

fn catalog() -> CatalogSnapshot {
    let mut id = column("id", PgType::BigInt, false);
    id.primary_key = true;
    let item = Table {
        name: "item".to_string(),
        columns: vec![
            id,
            column("name", PgType::Text, true),
            column("enabled", PgType::Boolean, false),
        ],
        check_constraints: Vec::new(),
        trigger_checks: Vec::new(),
        foreign_keys: Vec::new(),
        indices: Vec::new(),
        source: SourceLocation::default(),
        doc: None,
        icon: None,
    };
    CatalogSnapshot::from_schema_postgres_18(&Schema {
        tables: [(item.name.clone(), item)].into_iter().collect(),
    })
    .unwrap()
}

fn literal(id: u32, value: HirLiteral) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(id),
        origin: origin(),
        kind: HirExpressionKind::Literal(value),
    }
}

fn parameter(id: u32, parameter_id: u32) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(id),
        origin: origin(),
        kind: HirExpressionKind::Parameter(ParameterId::new(parameter_id)),
    }
}

fn column_ref(id: u32, binding: u32, column_id: dibs_pg_catalog::ColumnId) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(id),
        origin: origin(),
        kind: HirExpressionKind::Column {
            binding: RelationId::new(binding),
            column_id,
        },
    }
}

fn operator(id: u32, operator_id: OperatorId, operands: Vec<HirExpression>) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(id),
        origin: origin(),
        kind: HirExpressionKind::Operator {
            operator_id,
            operands,
        },
    }
}

fn call(id: u32, callable_id: CallableId, arguments: Vec<HirExpression>) -> HirExpression {
    let argument_count = arguments.len();
    HirExpression {
        id: ExpressionId::new(id),
        origin: origin(),
        kind: HirExpressionKind::Call(Box::new(HirCall {
            callable_id,
            arguments,
            argument_names: vec![None; argument_count],
            distinct: false,
            star: false,
            order_by: Vec::new(),
            filter: None,
            within_group: Vec::new(),
            over: None,
        })),
    }
}

fn set_query_with_parameters(
    left: HirStatement,
    right: HirStatement,
    parameters: Vec<HirParameter>,
) -> HirQuery {
    HirQuery {
        id: QueryId::new(90),
        name: "SetQuery".to_string(),
        origin: origin(),
        parameters,
        statement: HirStatement {
            id: StatementId::new(90),
            origin: origin(),
            kind: HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: vec![HirProjection {
                    field_id: FieldId::new(90),
                    alias: "value".to_string(),
                    alias_origin: origin(),
                    expression: literal(90, HirLiteral::Integer("0".to_string())),
                }],
                from: vec![HirRelation {
                    id: RelationId::new(90),
                    origin: origin(),
                    alias: Some(dibs_query_ir::RelationAlias {
                        name: "set_rows".to_string(),
                        column_names: vec!["value".to_string()],
                    }),
                    kind: HirRelationKind::SetOperation {
                        kind: dibs_query_ir::SetOperationKind::Union,
                        all: true,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                }],
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: None,
                offset: None,
                locks: Vec::new(),
            })),
        },
    }
}

fn query(
    catalog: &CatalogSnapshot,
    projection: HirExpression,
    predicate: Option<HirExpression>,
    group_by: Vec<HirExpression>,
    having: Option<HirExpression>,
    limit: Option<HirExpression>,
    parameters: Vec<HirParameter>,
) -> HirQuery {
    let table = catalog.resolve_table("public.item").unwrap();
    HirQuery {
        id: QueryId::new(1),
        name: "CheckItem".to_string(),
        origin: origin(),
        parameters,
        statement: HirStatement {
            id: StatementId::new(1),
            origin: origin(),
            kind: HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: vec![HirProjection {
                    field_id: FieldId::new(1),
                    alias: "value".to_string(),
                    alias_origin: origin(),
                    expression: projection,
                }],
                from: vec![HirRelation {
                    id: RelationId::new(1),
                    origin: origin(),
                    alias: None,
                    kind: HirRelationKind::Table {
                        table_id: table.id.clone(),
                    },
                }],
                predicate,
                group_by,
                having,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit,
                offset: None,
                locks: Vec::new(),
            })),
        },
    }
}

fn projection(output: &CheckedOutput) -> &dibs_query_ir::TypedExpression {
    let dibs_query_ir::TypedStatementKind::Select(select) = &output.statement.kind else {
        panic!("expected select")
    };
    &select.projections[0].expression
}

#[test]
fn scalar_literals_parameters_columns_and_nullability_are_typed() {
    let catalog = catalog();
    let table = catalog.resolve_table("public.item").unwrap();
    let name = table.column("name").unwrap();
    let bigint = catalog.resolve_type("pg_catalog.bigint").unwrap();
    let hir = query(
        &catalog,
        column_ref(1, 1, name.id.clone()),
        None,
        Vec::new(),
        None,
        None,
        vec![HirParameter {
            id: ParameterId::new(1),
            ordinal: 0,
            name: "id".to_string(),
            origin: origin(),
            type_id: bigint.id.clone(),
            typmod: None,
            nullable: false,
        }],
    );

    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert_eq!(projection(&checked).type_id, name.type_id);
    assert!(projection(&checked).nullability.is_nullable());
    assert_eq!(checked.parameters[0].id, ParameterId::new(1));
    assert_eq!(checked.parameters[0].type_id, bigint.id);
}

#[test]
fn incompatible_operator_is_structured_error() {
    let catalog = catalog();
    let plus = catalog
        .operators
        .iter()
        .find(|operator| operator.qualified_name == "pg_catalog.+")
        .unwrap();
    let hir = query(
        &catalog,
        operator(
            3,
            plus.id.clone(),
            vec![
                literal(1, HirLiteral::String("x".to_string())),
                literal(2, HirLiteral::Boolean(true)),
            ],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );

    let error = SemanticChecker::new(&catalog)
        .check_query(&hir)
        .unwrap_err();
    assert!(matches!(
        error,
        CheckError::Type(TypeResolutionError::IncompatibleOperator { .. })
    ));
}

#[test]
fn unknown_overloads_use_pg18_category_and_preferred_type_rules() {
    let mut catalog = catalog();
    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    catalog
        .register_scalar(
            dibs_pg_catalog::ScalarSignature {
                qualified_name: "app.pick".to_string(),
                arguments: vec![text.clone()],
                result: text.clone(),
            },
            dibs_pg_catalog::ScalarCallableFacts {
                volatility: dibs_pg_catalog::Volatility::Immutable,
                strict: true,
                result_nullability: dibs_pg_catalog::Nullability::Nullable,
            },
        )
        .unwrap();
    catalog
        .register_scalar(
            dibs_pg_catalog::ScalarSignature {
                qualified_name: "app.pick".to_string(),
                arguments: vec![bigint.clone()],
                result: bigint,
            },
            dibs_pg_catalog::ScalarCallableFacts {
                volatility: dibs_pg_catalog::Volatility::Immutable,
                strict: true,
                result_nullability: dibs_pg_catalog::Nullability::Nullable,
            },
        )
        .unwrap();
    let hir = query(
        &catalog,
        call(
            2,
            CallableId::new("unresolved:function:app.pick"),
            vec![literal(1, HirLiteral::Null)],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );

    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert_eq!(projection(&checked).type_id, text);

    let integer = catalog
        .resolve_type("pg_catalog.integer")
        .unwrap()
        .id
        .clone();
    let smallint = catalog
        .resolve_type("pg_catalog.smallint")
        .unwrap()
        .id
        .clone();
    for argument in [integer, smallint] {
        catalog
            .register_scalar(
                dibs_pg_catalog::ScalarSignature {
                    qualified_name: "app.numeric_pick".to_string(),
                    arguments: vec![argument.clone()],
                    result: argument,
                },
                dibs_pg_catalog::ScalarCallableFacts {
                    volatility: dibs_pg_catalog::Volatility::Immutable,
                    strict: true,
                    result_nullability: dibs_pg_catalog::Nullability::Nullable,
                },
            )
            .unwrap();
    }
    let ambiguous = query(
        &catalog,
        call(
            3,
            CallableId::new("unresolved:function:app.numeric_pick"),
            vec![literal(2, HirLiteral::Null)],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&ambiguous)
            .unwrap_err(),
        CheckError::Type(TypeResolutionError::AmbiguousCallable { .. })
    ));
}

#[test]
fn polymorphic_arguments_are_unified_across_the_whole_call() {
    let mut catalog = catalog();
    let anyelement = catalog
        .resolve_type("pg_catalog.anyelement")
        .unwrap()
        .id
        .clone();
    let anyarray = catalog
        .resolve_type("pg_catalog.anyarray")
        .unwrap()
        .id
        .clone();
    let integer = catalog
        .resolve_type("pg_catalog.integer")
        .unwrap()
        .id
        .clone();
    let integer_array = catalog
        .resolve_type("pg_catalog.integer[]")
        .unwrap()
        .id
        .clone();
    let text_array = catalog
        .resolve_type("pg_catalog.text[]")
        .unwrap()
        .id
        .clone();
    catalog.callables.push(CatalogCallable {
        id: CallableId::new("pg18:callable:test:app.same_element(anyelement,anyelement)"),
        qualified_name: "app.same_element".to_string(),
        kind: CallableKind::Scalar,
        arguments: vec![anyelement.clone(), anyelement.clone()],
        aggregated_arguments: Vec::new(),
        parameter_names: vec![None, None],
        required_arguments: 2,
        scalar_result: Some(anyelement.clone()),
        table_columns: Vec::new(),
        volatility: dibs_pg_catalog::Volatility::Immutable,
        strict: true,
        scalar_result_nullability: Some(dibs_pg_catalog::Nullability::Nullable),
        cardinality: CallableCardinality::ExactlyOne,
        aggregate_empty: None,
        postgres_identity_arguments: "anyelement, anyelement".to_string(),
        postgres_result_type: "anyelement".to_string(),
        builtin: false,
    });
    catalog.callables.push(CatalogCallable {
        id: CallableId::new("pg18:callable:test:app.array_element(anyarray,anyelement)"),
        qualified_name: "app.array_element".to_string(),
        kind: CallableKind::Scalar,
        arguments: vec![anyarray, anyelement.clone()],
        aggregated_arguments: Vec::new(),
        parameter_names: vec![None, None],
        required_arguments: 2,
        scalar_result: Some(anyelement),
        table_columns: Vec::new(),
        volatility: dibs_pg_catalog::Volatility::Immutable,
        strict: true,
        scalar_result_nullability: Some(dibs_pg_catalog::Nullability::Nullable),
        cardinality: CallableCardinality::ExactlyOne,
        aggregate_empty: None,
        postgres_identity_arguments: "anyarray, anyelement".to_string(),
        postgres_result_type: "anyelement".to_string(),
        builtin: false,
    });

    let mismatch = query(
        &catalog,
        call(
            40,
            CallableId::new("unresolved:function:app.same_element"),
            vec![
                literal(41, HirLiteral::Integer("1".to_string())),
                literal(42, HirLiteral::String("x".to_string())),
            ],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&mismatch)
            .unwrap_err(),
        CheckError::Type(TypeResolutionError::IncompatibleCallable { .. })
    ));

    let consistent = query(
        &catalog,
        call(
            43,
            CallableId::new("unresolved:function:app.same_element"),
            vec![
                literal(44, HirLiteral::Integer("1".to_string())),
                literal(45, HirLiteral::Integer("2".to_string())),
            ],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog)
        .check_query(&consistent)
        .unwrap();
    assert_eq!(projection(&checked).type_id, integer);

    let parameters = vec![
        HirParameter {
            id: ParameterId::new(1),
            ordinal: 0,
            name: "values".to_string(),
            origin: origin(),
            type_id: integer_array,
            typmod: None,
            nullable: false,
        },
        HirParameter {
            id: ParameterId::new(2),
            ordinal: 1,
            name: "value".to_string(),
            origin: origin(),
            type_id: integer.clone(),
            typmod: None,
            nullable: false,
        },
    ];
    let coupled = query(
        &catalog,
        call(
            46,
            CallableId::new("unresolved:function:app.array_element"),
            vec![parameter(47, 1), parameter(48, 2)],
        ),
        None,
        Vec::new(),
        None,
        None,
        parameters,
    );
    let checked = SemanticChecker::new(&catalog)
        .check_query(&coupled)
        .unwrap();
    assert_eq!(projection(&checked).type_id, integer);

    let mismatched_coupling = query(
        &catalog,
        call(
            49,
            CallableId::new("unresolved:function:app.array_element"),
            vec![parameter(50, 1), parameter(51, 2)],
        ),
        None,
        Vec::new(),
        None,
        None,
        vec![
            HirParameter {
                id: ParameterId::new(1),
                ordinal: 0,
                name: "values".to_string(),
                origin: origin(),
                type_id: text_array,
                typmod: None,
                nullable: false,
            },
            HirParameter {
                id: ParameterId::new(2),
                ordinal: 1,
                name: "value".to_string(),
                origin: origin(),
                type_id: integer,
                typmod: None,
                nullable: false,
            },
        ],
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&mismatched_coupling)
            .unwrap_err(),
        CheckError::Type(TypeResolutionError::IncompatibleCallable { .. })
    ));
}

#[test]
fn anycompatible_uses_one_common_family_type_for_arguments_and_result() {
    let mut catalog = catalog();
    let anycompatible = catalog
        .resolve_type("pg_catalog.anycompatible")
        .unwrap()
        .id
        .clone();
    let numeric = catalog
        .resolve_type("pg_catalog.numeric")
        .unwrap()
        .id
        .clone();
    catalog.callables.push(CatalogCallable {
        id: CallableId::new("pg18:callable:test:app.compatible_pair(anycompatible,anycompatible)"),
        qualified_name: "app.compatible_pair".to_string(),
        kind: CallableKind::Scalar,
        arguments: vec![anycompatible.clone(), anycompatible.clone()],
        aggregated_arguments: Vec::new(),
        parameter_names: vec![None, None],
        required_arguments: 2,
        scalar_result: Some(anycompatible),
        table_columns: Vec::new(),
        volatility: dibs_pg_catalog::Volatility::Immutable,
        strict: true,
        scalar_result_nullability: Some(dibs_pg_catalog::Nullability::Nullable),
        cardinality: CallableCardinality::ExactlyOne,
        aggregate_empty: None,
        postgres_identity_arguments: "anycompatible, anycompatible".to_string(),
        postgres_result_type: "anycompatible".to_string(),
        builtin: false,
    });
    let compatible = query(
        &catalog,
        call(
            52,
            CallableId::new("unresolved:function:app.compatible_pair"),
            vec![
                literal(53, HirLiteral::Integer("1".to_string())),
                literal(54, HirLiteral::Numeric("1.5".to_string())),
            ],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog)
        .check_query(&compatible)
        .unwrap();
    assert_eq!(projection(&checked).type_id, numeric);
    let TypedExpressionKind::Call(call) = &projection(&checked).kind else {
        panic!("expected callable")
    };
    assert_eq!(
        call.arguments[0]
            .coercion
            .as_ref()
            .expect("integer compatible argument must coerce")
            .target_type,
        numeric
    );
    assert!(call.arguments[1].coercion.is_none());
}

#[test]
fn where_and_having_require_boolean() {
    let catalog = catalog();
    let table = catalog.resolve_table("public.item").unwrap();
    let id = table.column("id").unwrap();
    let non_boolean = column_ref(2, 1, id.id.clone());
    let where_hir = query(
        &catalog,
        literal(1, HirLiteral::Integer("1".to_string())),
        Some(non_boolean.clone()),
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&where_hir)
            .unwrap_err(),
        CheckError::NonBooleanPredicate {
            clause: "WHERE",
            ..
        }
    ));

    let having_hir = query(
        &catalog,
        literal(1, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        Some(non_boolean),
        None,
        Vec::new(),
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&having_hir)
            .unwrap_err(),
        CheckError::NonBooleanPredicate {
            clause: "HAVING",
            ..
        }
    ));
}

#[test]
fn set_operations_require_equal_arity_and_compatible_common_types() {
    let catalog = catalog();
    let left = query(
        &catalog,
        literal(1, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .statement;
    let mut right = query(
        &catalog,
        literal(2, HirLiteral::Integer("2".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .statement;
    let HirStatementKind::Select(select) = &mut right.kind else {
        unreachable!()
    };
    select.projections.push(HirProjection {
        field_id: FieldId::new(2),
        alias: "other".to_string(),
        alias_origin: origin(),
        expression: literal(3, HirLiteral::Integer("3".to_string())),
    });
    let set = HirQuery {
        id: QueryId::new(2),
        name: "SetMismatch".to_string(),
        origin: origin(),
        parameters: Vec::new(),
        statement: HirStatement {
            id: StatementId::new(2),
            origin: origin(),
            kind: HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: vec![HirProjection {
                    field_id: FieldId::new(10),
                    alias: "value".to_string(),
                    alias_origin: origin(),
                    expression: literal(10, HirLiteral::Integer("0".to_string())),
                }],
                from: vec![HirRelation {
                    id: RelationId::new(9),
                    origin: origin(),
                    alias: Some(dibs_query_ir::RelationAlias {
                        name: "set_rows".to_string(),
                        column_names: vec!["value".to_string()],
                    }),
                    kind: HirRelationKind::SetOperation {
                        kind: dibs_query_ir::SetOperationKind::Union,
                        all: true,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                }],
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: None,
                offset: None,
                locks: Vec::new(),
            })),
        },
    };

    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&set)
            .unwrap_err(),
        CheckError::SetColumnCountMismatch { left: 1, right: 2 }
    ));
}

#[test]
fn scalar_aggregate_produces_exactly_one_row() {
    let catalog = catalog();
    let count_id = catalog
        .callable_candidates("count", 0)
        .next()
        .unwrap()
        .id
        .clone();
    let hir = query(
        &catalog,
        HirExpression {
            id: ExpressionId::new(1),
            origin: origin(),
            kind: HirExpressionKind::Call(Box::new(HirCall {
                callable_id: count_id,
                arguments: Vec::new(),
                argument_names: Vec::new(),
                distinct: false,
                star: true,
                order_by: Vec::new(),
                filter: None,
                within_group: Vec::new(),
                over: None,
            })),
        },
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );

    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert_eq!(checked.cardinality.lower(), LowerBound::One);
    assert_eq!(checked.cardinality.upper(), UpperBound::One);
    assert!(matches!(
        checked.cardinality.proof(),
        [CardinalityEvidence::ScalarAggregate { .. }]
    ));
}

#[test]
fn aggregate_and_window_nullability_come_from_catalog_facts() {
    let catalog = catalog();
    let table = catalog.resolve_table("public.item").unwrap();
    let id = table.column("id").unwrap();
    let sum = catalog
        .callable_candidates("sum", 1)
        .find(|callable| callable.postgres_identity_arguments == "bigint")
        .unwrap();
    let sum_hir = query(
        &catalog,
        call(2, sum.id.clone(), vec![column_ref(1, 1, id.id.clone())]),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let sum_checked = SemanticChecker::new(&catalog)
        .check_query(&sum_hir)
        .unwrap();
    assert!(projection(&sum_checked).nullability.is_nullable());
    assert_eq!(sum_checked.cardinality.lower(), LowerBound::One);
    assert_eq!(sum_checked.cardinality.upper(), UpperBound::One);

    let row_number = catalog.callable_candidates("row_number", 0).next().unwrap();
    let mut row_number_expression = call(3, row_number.id.clone(), Vec::new());
    let HirExpressionKind::Call(call) = &mut row_number_expression.kind else {
        unreachable!()
    };
    call.over = Some(dibs_query_ir::WindowReference::Inline(
        dibs_query_ir::WindowSpec {
            existing: None,
            partition_by: Vec::new(),
            order_by: Vec::new(),
            frame: None,
        },
    ));
    let window_hir = query(
        &catalog,
        row_number_expression,
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let window_checked = SemanticChecker::new(&catalog)
        .check_query(&window_hir)
        .unwrap();
    assert!(!projection(&window_checked).nullability.is_nullable());
    assert_eq!(window_checked.cardinality.lower(), LowerBound::Zero);
    assert_eq!(window_checked.cardinality.upper(), UpperBound::Unbounded);
}

#[test]
fn structural_null_tests_are_boolean_and_non_null() {
    let catalog = catalog();
    let expression = operator(
        2,
        OperatorId::new(dibs_query_typing::SYNTAX_IS_NULL_OPERATOR_ID),
        vec![literal(1, HirLiteral::Null)],
    );
    let hir = query(
        &catalog,
        expression,
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert_eq!(
        projection(&checked).type_id,
        catalog.resolve_type("pg_catalog.boolean").unwrap().id
    );
    assert!(!projection(&checked).nullability.is_nullable());
}

#[test]
fn set_operations_reject_incompatible_column_types() {
    let catalog = catalog();
    let left = query(
        &catalog,
        literal(1, HirLiteral::Boolean(true)),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .statement;
    let right = query(
        &catalog,
        literal(2, HirLiteral::Integer("2".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .statement;
    let set = HirQuery {
        id: QueryId::new(2),
        name: "SetTypeMismatch".to_string(),
        origin: origin(),
        parameters: Vec::new(),
        statement: HirStatement {
            id: StatementId::new(2),
            origin: origin(),
            kind: HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: vec![HirProjection {
                    field_id: FieldId::new(10),
                    alias: "value".to_string(),
                    alias_origin: origin(),
                    expression: literal(10, HirLiteral::Integer("0".to_string())),
                }],
                from: vec![HirRelation {
                    id: RelationId::new(9),
                    origin: origin(),
                    alias: Some(dibs_query_ir::RelationAlias {
                        name: "set_rows".to_string(),
                        column_names: vec!["value".to_string()],
                    }),
                    kind: HirRelationKind::SetOperation {
                        kind: dibs_query_ir::SetOperationKind::Union,
                        all: true,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                }],
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: None,
                offset: None,
                locks: Vec::new(),
            })),
        },
    };
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&set)
            .unwrap_err(),
        CheckError::Type(TypeResolutionError::IncompatibleCommonType { .. })
    ));
}

#[test]
fn limit_changes_only_the_upper_bound_and_does_not_select_a_declared_mode() {
    let catalog = catalog();
    let hir = query(
        &catalog,
        literal(1, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        None,
        Some(literal(2, HirLiteral::Integer("1".to_string()))),
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();

    assert_eq!(checked.cardinality.lower(), LowerBound::Zero);
    assert_eq!(checked.cardinality.upper(), UpperBound::One);
    assert!(checked.validate_mode(ResultMode::Many).is_ok());
    assert!(checked.validate_mode(ResultMode::Optional).is_ok());
    assert!(matches!(
        checked.validate_mode(ResultMode::One),
        Err(CardinalityModeError::Incompatible {
            mode: ResultMode::One,
            ..
        })
    ));
}

#[test]
fn declared_modes_fail_closed_against_inferred_cardinality_and_row_shape() {
    let catalog = catalog();
    let hir = query(
        &catalog,
        literal(1, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();

    assert!(matches!(
        checked.validate_mode(ResultMode::Optional),
        Err(CardinalityModeError::Incompatible {
            mode: ResultMode::Optional,
            ..
        })
    ));
    assert!(matches!(
        checked.validate_mode(ResultMode::Exec),
        Err(CardinalityModeError::Incompatible {
            mode: ResultMode::Exec,
            ..
        })
    ));
}

#[test]
fn nullable_parameter_evidence_is_preserved() {
    let catalog = catalog();
    let text = catalog.resolve_type("pg_catalog.text").unwrap();
    let hir = query(
        &catalog,
        parameter(1, 1),
        None,
        Vec::new(),
        None,
        None,
        vec![HirParameter {
            id: ParameterId::new(1),
            ordinal: 0,
            name: "needle".to_string(),
            origin: origin(),
            type_id: text.id.clone(),
            typmod: None,
            nullable: true,
        }],
    );
    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert!(projection(&checked).nullability.is_nullable());
}

#[test]
fn having_can_remove_scalar_aggregate_row() {
    let catalog = catalog();
    let count = catalog.callable_candidates("count", 0).next().unwrap();
    let hir = query(
        &catalog,
        HirExpression {
            id: ExpressionId::new(101),
            origin: origin(),
            kind: HirExpressionKind::Call(Box::new(HirCall {
                callable_id: count.id.clone(),
                arguments: Vec::new(),
                argument_names: Vec::new(),
                distinct: false,
                star: true,
                order_by: Vec::new(),
                filter: None,
                within_group: Vec::new(),
                over: None,
            })),
        },
        None,
        Vec::new(),
        Some(literal(102, HirLiteral::Boolean(false))),
        None,
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert_eq!(checked.cardinality.lower(), LowerBound::Zero);
    assert_eq!(checked.cardinality.upper(), UpperBound::One);
    assert!(matches!(
        checked.validate_mode(ResultMode::One),
        Err(CardinalityModeError::Incompatible { .. })
    ));
}

#[test]
fn ungrouped_column_alongside_aggregate_is_rejected() {
    let catalog = catalog();
    let table = catalog.resolve_table("public.item").unwrap();
    let id = table.column("id").unwrap();
    let count = catalog.callable_candidates("count", 0).next().unwrap();
    let mut hir = query(
        &catalog,
        call(103, count.id.clone(), Vec::new()),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let HirStatementKind::Select(select) = &mut hir.statement.kind else {
        unreachable!()
    };
    select.projections.push(HirProjection {
        field_id: FieldId::new(104),
        alias: "id".to_string(),
        alias_origin: origin(),
        expression: column_ref(104, 1, id.id.clone()),
    });
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&hir)
            .unwrap_err(),
        CheckError::UngroupedAggregateProjection { .. }
    ));
}

#[test]
fn grouping_by_primary_key_allows_same_binding_columns() {
    let catalog = catalog();
    let table = catalog.resolve_table("public.item").unwrap();
    let id = table.column("id").unwrap();
    let name = table.column("name").unwrap();
    let count = catalog.callable_candidates("count", 0).next().unwrap();
    let mut hir = query(
        &catalog,
        call(105, count.id.clone(), Vec::new()),
        None,
        vec![column_ref(106, 1, id.id.clone())],
        None,
        None,
        Vec::new(),
    );
    let HirStatementKind::Select(select) = &mut hir.statement.kind else {
        unreachable!()
    };
    select.projections.push(HirProjection {
        field_id: FieldId::new(107),
        alias: "name".to_string(),
        alias_origin: origin(),
        expression: column_ref(107, 1, name.id.clone()),
    });

    SemanticChecker::new(&catalog).check_query(&hir).unwrap();
}

#[test]
fn contextual_integer_range_is_checked_before_coercion() {
    let catalog = catalog();
    let hir = query(
        &catalog,
        call(
            106,
            CallableId::new("unresolved:function:abs"),
            vec![literal(
                105,
                HirLiteral::Integer("999999999999999999999999".to_string()),
            )],
        ),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&hir)
            .unwrap_err(),
        CheckError::NumericLiteralOutOfRange { .. }
    ));
}

#[test]
fn common_type_flattens_domains_and_emits_proof_bearing_set_coercion() {
    let mut catalog = catalog();
    let domain = catalog
        .register_type(TypeRegistration {
            qualified_name: "app.positive_bigint".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: Vec::new(),
            },
        })
        .unwrap();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let left = query(
        &catalog,
        parameter(107, 1),
        None,
        Vec::new(),
        None,
        None,
        vec![HirParameter {
            id: ParameterId::new(1),
            ordinal: 0,
            name: "domain_value".to_string(),
            origin: origin(),
            type_id: domain.clone(),
            typmod: None,
            nullable: false,
        }],
    )
    .statement;
    let right = query(
        &catalog,
        parameter(108, 2),
        None,
        Vec::new(),
        None,
        None,
        vec![HirParameter {
            id: ParameterId::new(2),
            ordinal: 0,
            name: "base_value".to_string(),
            origin: origin(),
            type_id: bigint.clone(),
            typmod: None,
            nullable: false,
        }],
    )
    .statement;
    let checked = SemanticChecker::new(&catalog)
        .check_query(&set_query_with_parameters(
            left,
            right,
            vec![
                HirParameter {
                    id: ParameterId::new(1),
                    ordinal: 0,
                    name: "domain_value".to_string(),
                    origin: origin(),
                    type_id: domain.clone(),
                    typmod: None,
                    nullable: false,
                },
                HirParameter {
                    id: ParameterId::new(2),
                    ordinal: 1,
                    name: "base_value".to_string(),
                    origin: origin(),
                    type_id: bigint.clone(),
                    typmod: None,
                    nullable: false,
                },
            ],
        ))
        .unwrap();
    let TypedStatementKind::Select(select) = &checked.statement.kind else {
        panic!("expected outer SELECT")
    };
    let TypedRelationKind::SetOperation { left, right, .. } = &select.from[0].kind else {
        panic!("expected set relation")
    };
    let TypedStatementKind::Select(left) = &left.kind else {
        panic!("expected left SELECT")
    };
    let TypedStatementKind::Select(right) = &right.kind else {
        panic!("expected right SELECT")
    };
    let coercion = left.projections[0]
        .coercion
        .as_ref()
        .expect("domain branch must be flattened at the projection use site");
    assert_eq!(coercion.source_type, domain);
    assert_eq!(coercion.target_type, bigint);
    assert!(matches!(
        coercion.evidence,
        CoercionEvidence::DomainBase { .. }
    ));
    assert!(right.projections[0].coercion.is_none());
}

#[test]
fn set_projection_coercion_drives_checked_output_type_and_codec() {
    let mut catalog = catalog();
    let domain = catalog
        .register_type(TypeRegistration {
            qualified_name: "app.positive_bigint_output".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: Vec::new(),
            },
        })
        .unwrap();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let left = query(
        &catalog,
        parameter(130, 1),
        None,
        Vec::new(),
        None,
        None,
        vec![HirParameter {
            id: ParameterId::new(1),
            ordinal: 0,
            name: "domain_value".to_string(),
            origin: origin(),
            type_id: domain.clone(),
            typmod: None,
            nullable: false,
        }],
    )
    .statement;
    let right = query(
        &catalog,
        parameter(131, 2),
        None,
        Vec::new(),
        None,
        None,
        vec![HirParameter {
            id: ParameterId::new(2),
            ordinal: 0,
            name: "base_value".to_string(),
            origin: origin(),
            type_id: bigint.clone(),
            typmod: None,
            nullable: false,
        }],
    )
    .statement;
    let mut set = set_query_with_parameters(
        left,
        right,
        vec![
            HirParameter {
                id: ParameterId::new(1),
                ordinal: 0,
                name: "domain_value".to_string(),
                origin: origin(),
                type_id: domain,
                typmod: None,
                nullable: false,
            },
            HirParameter {
                id: ParameterId::new(2),
                ordinal: 1,
                name: "base_value".to_string(),
                origin: origin(),
                type_id: bigint.clone(),
                typmod: None,
                nullable: false,
            },
        ],
    );
    let HirStatementKind::Select(select) = &mut set.statement.kind else {
        unreachable!()
    };
    select.projections[0].expression = HirExpression {
        id: ExpressionId::new(132),
        origin: origin(),
        kind: HirExpressionKind::DerivedColumn {
            binding: RelationId::new(90),
            field_id: FieldId::new(1),
        },
    };
    let checked = SemanticChecker::new(&catalog).check_query(&set).unwrap();
    let bigint_facts = catalog.type_by_id(&bigint).unwrap();
    assert_eq!(checked.output_fields[0].type_id, bigint);
    assert_eq!(
        checked.output_fields[0].pg_codec_id,
        bigint_facts.pg_codec_id
    );
    assert_eq!(
        checked.output_fields[0].wire_codec_id,
        bigint_facts.wire_codec_id
    );
}

#[test]
fn cte_and_subquery_bindings_propagate_post_coercion_projection_facts() {
    let mut catalog = catalog();
    let domain = catalog
        .register_type(TypeRegistration {
            qualified_name: "app.derived_positive_bigint".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: Vec::new(),
            },
        })
        .unwrap();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let set_statement = || {
        let left = query(
            &catalog,
            parameter(133, 1),
            None,
            Vec::new(),
            None,
            None,
            vec![HirParameter {
                id: ParameterId::new(1),
                ordinal: 0,
                name: "domain_value".to_string(),
                origin: origin(),
                type_id: domain.clone(),
                typmod: None,
                nullable: false,
            }],
        )
        .statement;
        let right = query(
            &catalog,
            parameter(134, 2),
            None,
            Vec::new(),
            None,
            None,
            vec![HirParameter {
                id: ParameterId::new(2),
                ordinal: 1,
                name: "base_value".to_string(),
                origin: origin(),
                type_id: bigint.clone(),
                typmod: None,
                nullable: false,
            }],
        )
        .statement;
        let mut set = set_query_with_parameters(
            left,
            right,
            vec![
                HirParameter {
                    id: ParameterId::new(1),
                    ordinal: 0,
                    name: "domain_value".to_string(),
                    origin: origin(),
                    type_id: domain.clone(),
                    typmod: None,
                    nullable: false,
                },
                HirParameter {
                    id: ParameterId::new(2),
                    ordinal: 1,
                    name: "base_value".to_string(),
                    origin: origin(),
                    type_id: bigint.clone(),
                    typmod: None,
                    nullable: false,
                },
            ],
        );
        let HirStatementKind::Select(select) = &mut set.statement.kind else {
            unreachable!()
        };
        select.projections[0].expression = HirExpression {
            id: ExpressionId::new(139),
            origin: origin(),
            kind: HirExpressionKind::DerivedColumn {
                binding: RelationId::new(90),
                field_id: FieldId::new(1),
            },
        };
        set.statement
    };
    let parameters = vec![
        HirParameter {
            id: ParameterId::new(1),
            ordinal: 0,
            name: "domain_value".to_string(),
            origin: origin(),
            type_id: domain.clone(),
            typmod: None,
            nullable: false,
        },
        HirParameter {
            id: ParameterId::new(2),
            ordinal: 1,
            name: "base_value".to_string(),
            origin: origin(),
            type_id: bigint.clone(),
            typmod: None,
            nullable: false,
        },
    ];

    let mut subquery = query(
        &catalog,
        literal(135, HirLiteral::Integer("0".to_string())),
        None,
        Vec::new(),
        None,
        None,
        parameters.clone(),
    );
    let HirStatementKind::Select(select) = &mut subquery.statement.kind else {
        unreachable!()
    };
    select.from = vec![HirRelation {
        id: RelationId::new(91),
        origin: origin(),
        alias: Some(dibs_query_ir::RelationAlias {
            name: "derived_rows".to_string(),
            column_names: vec!["value".to_string()],
        }),
        kind: HirRelationKind::Subquery(Box::new(set_statement())),
    }];
    select.projections[0].expression = HirExpression {
        id: ExpressionId::new(136),
        origin: origin(),
        kind: HirExpressionKind::DerivedColumn {
            binding: RelationId::new(91),
            field_id: FieldId::new(90),
        },
    };
    let checked = SemanticChecker::new(&catalog)
        .check_query(&subquery)
        .unwrap();
    assert_eq!(projection(&checked).type_id, bigint);

    let cte_id = CteId::new(7);
    let mut cte_query = query(
        &catalog,
        literal(137, HirLiteral::Integer("0".to_string())),
        None,
        Vec::new(),
        None,
        None,
        parameters,
    );
    let HirStatementKind::Select(select) = &mut cte_query.statement.kind else {
        unreachable!()
    };
    select.ctes = vec![HirCte {
        id: cte_id,
        recursive: false,
        name: "typed_rows".to_string(),
        origin: origin(),
        materialization: CteMaterialization::Default,
        statement: Box::new(set_statement()),
    }];
    select.from = vec![HirRelation {
        id: RelationId::new(7),
        origin: origin(),
        alias: None,
        kind: HirRelationKind::Cte { cte_id },
    }];
    select.projections[0].expression = HirExpression {
        id: ExpressionId::new(138),
        origin: origin(),
        kind: HirExpressionKind::CteColumn {
            cte_id,
            binding: RelationId::new(7),
            field_id: FieldId::new(90),
        },
    };
    let checked = SemanticChecker::new(&catalog)
        .check_query(&cte_query)
        .unwrap();
    assert_eq!(projection(&checked).type_id, bigint);
}

#[test]
fn unsupported_recursive_cte_body_fails_closed() {
    let catalog = catalog();
    let mut recursive_query = query(
        &catalog,
        literal(109, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let HirStatementKind::Select(select) = &mut recursive_query.statement.kind else {
        unreachable!()
    };
    select.recursive = true;
    select.ctes.push(HirCte {
        id: CteId::new(9),
        recursive: true,
        name: "recursive_rows".to_string(),
        origin: origin(),
        materialization: CteMaterialization::Default,
        statement: Box::new(
            query(
                &catalog,
                literal(110, HirLiteral::Integer("1".to_string())),
                None,
                Vec::new(),
                None,
                None,
                Vec::new(),
            )
            .statement,
        ),
    });
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&recursive_query)
            .unwrap_err(),
        CheckError::UnsupportedRecursiveCte { .. }
    ));
}

#[test]
fn unbounded_scalar_subqueries_fail_closed() {
    let catalog = catalog();
    let subquery = query(
        &catalog,
        literal(110, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .statement;
    let scalar = query(
        &catalog,
        HirExpression {
            id: ExpressionId::new(111),
            origin: origin(),
            kind: HirExpressionKind::ScalarSubquery(Box::new(subquery)),
        },
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    assert!(matches!(
        SemanticChecker::new(&catalog)
            .check_query(&scalar)
            .unwrap_err(),
        CheckError::UnboundedScalarSubquery { .. }
    ));
}

#[test]
fn ordinary_from_products_preserve_exact_row_bounds() {
    let catalog = catalog();
    let mut hir = query(
        &catalog,
        literal(112, HirLiteral::Integer("1".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let HirStatementKind::Select(select) = &mut hir.statement.kind else {
        unreachable!()
    };
    select.from = vec![
        HirRelation {
            id: RelationId::new(112),
            origin: origin(),
            alias: None,
            kind: HirRelationKind::Values {
                rows: dibs_query_ir::HirValues::try_new(vec![vec![literal(
                    113,
                    HirLiteral::Integer("1".to_string()),
                )]])
                .unwrap(),
            },
        },
        HirRelation {
            id: RelationId::new(114),
            origin: origin(),
            alias: None,
            kind: HirRelationKind::Values {
                rows: dibs_query_ir::HirValues::try_new(vec![vec![literal(
                    115,
                    HirLiteral::Integer("2".to_string()),
                )]])
                .unwrap(),
            },
        },
    ];
    let checked = SemanticChecker::new(&catalog).check_query(&hir).unwrap();
    assert_eq!(checked.cardinality.lower(), LowerBound::One);
    assert_eq!(checked.cardinality.upper(), UpperBound::One);
}

#[test]
fn case_array_and_values_emit_proof_bearing_input_coercions() {
    let catalog = catalog();
    let numeric = catalog
        .resolve_type("pg_catalog.numeric")
        .unwrap()
        .id
        .clone();
    let case = query(
        &catalog,
        HirExpression {
            id: ExpressionId::new(116),
            origin: origin(),
            kind: HirExpressionKind::Case {
                operand: None,
                branches: vec![HirCaseBranch {
                    when: literal(117, HirLiteral::Boolean(true)),
                    then: literal(118, HirLiteral::Integer("1".to_string())),
                }],
                else_expression: Some(Box::new(literal(
                    119,
                    HirLiteral::Numeric("1.5".to_string()),
                ))),
            },
        },
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let case = SemanticChecker::new(&catalog).check_query(&case).unwrap();
    let TypedExpressionKind::Case {
        branches,
        else_expression,
        ..
    } = &projection(&case).kind
    else {
        panic!("expected CASE")
    };
    assert_eq!(projection(&case).type_id, numeric);
    assert_eq!(
        branches[0]
            .then
            .coercion
            .as_ref()
            .expect("integer CASE arm must coerce")
            .target_type,
        numeric
    );
    assert!(
        else_expression
            .as_ref()
            .expect("authored ELSE must be retained")
            .coercion
            .is_none()
    );

    let array = query(
        &catalog,
        HirExpression {
            id: ExpressionId::new(120),
            origin: origin(),
            kind: HirExpressionKind::Array(vec![
                literal(121, HirLiteral::Integer("1".to_string())),
                literal(122, HirLiteral::Numeric("1.5".to_string())),
            ]),
        },
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let array = SemanticChecker::new(&catalog).check_query(&array).unwrap();
    let TypedExpressionKind::Array { elements, .. } = &projection(&array).kind else {
        panic!("expected ARRAY")
    };
    assert_eq!(
        elements[0]
            .coercion
            .as_ref()
            .expect("integer array element must coerce")
            .target_type,
        numeric
    );
    assert!(elements[1].coercion.is_none());

    let mut values = query(
        &catalog,
        literal(123, HirLiteral::Integer("0".to_string())),
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let HirStatementKind::Select(select) = &mut values.statement.kind else {
        unreachable!()
    };
    select.from = vec![HirRelation {
        id: RelationId::new(124),
        origin: origin(),
        alias: None,
        kind: HirRelationKind::Values {
            rows: dibs_query_ir::HirValues::try_new(vec![
                vec![literal(125, HirLiteral::Integer("1".to_string()))],
                vec![literal(126, HirLiteral::Numeric("1.5".to_string()))],
            ])
            .unwrap(),
        },
    }];
    let checked = SemanticChecker::new(&catalog).check_query(&values).unwrap();
    let TypedStatementKind::Select(select) = &checked.statement.kind else {
        panic!("expected SELECT")
    };
    let TypedRelationKind::Values { rows } = &select.from[0].kind else {
        panic!("expected VALUES")
    };
    assert_eq!(rows.columns()[0].type_id, numeric);
    assert!(matches!(
        rows.columns()[0].common_type,
        CoercionEvidence::CommonType { .. }
    ));
    assert_eq!(
        rows.rows()[0][0]
            .coercion
            .as_ref()
            .expect("integer VALUES cell must coerce")
            .target_type,
        numeric
    );
    assert!(rows.rows()[1][0].coercion.is_none());
}

#[test]
fn values_domain_and_base_cells_preserve_common_type_evidence() {
    let mut catalog = catalog();
    let domain = catalog
        .register_type(TypeRegistration {
            qualified_name: "app.values_positive_bigint".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: Vec::new(),
            },
        })
        .unwrap();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let mut values = query(
        &catalog,
        literal(140, HirLiteral::Integer("0".to_string())),
        None,
        Vec::new(),
        None,
        None,
        vec![
            HirParameter {
                id: ParameterId::new(1),
                ordinal: 0,
                name: "domain_value".to_string(),
                origin: origin(),
                type_id: domain.clone(),
                typmod: None,
                nullable: false,
            },
            HirParameter {
                id: ParameterId::new(2),
                ordinal: 1,
                name: "base_value".to_string(),
                origin: origin(),
                type_id: bigint.clone(),
                typmod: None,
                nullable: false,
            },
        ],
    );
    let HirStatementKind::Select(select) = &mut values.statement.kind else {
        unreachable!()
    };
    select.from = vec![HirRelation {
        id: RelationId::new(141),
        origin: origin(),
        alias: None,
        kind: HirRelationKind::Values {
            rows: dibs_query_ir::HirValues::try_new(vec![
                vec![parameter(142, 1)],
                vec![parameter(143, 2)],
            ])
            .unwrap(),
        },
    }];
    let checked = SemanticChecker::new(&catalog).check_query(&values).unwrap();
    let TypedStatementKind::Select(select) = &checked.statement.kind else {
        panic!("expected SELECT")
    };
    let TypedRelationKind::Values { rows } = &select.from[0].kind else {
        panic!("expected VALUES")
    };
    let column = &rows.columns()[0];
    assert_eq!(column.type_id, bigint);
    assert!(!column.nullability.is_nullable());
    assert!(matches!(
        column.nullability.evidence(),
        [NullabilityEvidence::ValuesPropagation]
    ));
    assert!(matches!(
        &column.common_type,
        CoercionEvidence::CommonType { resolved, inputs }
            if resolved == &bigint && inputs == &[domain.clone(), bigint.clone()]
    ));
    assert!(matches!(
        rows.rows()[0][0]
            .coercion
            .as_ref()
            .map(|coercion| &coercion.evidence),
        Some(CoercionEvidence::DomainBase { domain: actual, base })
            if actual == &domain && base == &bigint
    ));
    assert!(rows.rows()[1][0].coercion.is_none());
}

#[test]
fn case_without_else_uses_implicit_nullable_unknown_arm() {
    let catalog = catalog();
    let case = query(
        &catalog,
        HirExpression {
            id: ExpressionId::new(127),
            origin: origin(),
            kind: HirExpressionKind::Case {
                operand: None,
                branches: vec![HirCaseBranch {
                    when: literal(128, HirLiteral::Boolean(true)),
                    then: literal(129, HirLiteral::Integer("1".to_string())),
                }],
                else_expression: None,
            },
        },
        None,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let checked = SemanticChecker::new(&catalog).check_query(&case).unwrap();
    let expression = projection(&checked);
    assert_eq!(
        expression.type_id,
        catalog.resolve_type("pg_catalog.integer").unwrap().id
    );
    assert!(expression.nullability.is_nullable());
    assert!(matches!(
        expression.nullability.evidence(),
        [NullabilityEvidence::CaseBranch]
    ));
    let TypedExpressionKind::Case {
        else_expression, ..
    } = &expression.kind
    else {
        panic!("expected CASE")
    };
    assert!(else_expression.is_none());
}
