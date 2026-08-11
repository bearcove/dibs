use std::path::PathBuf;

use dibs_pg_catalog::{
    ApiTypeId, ColumnId, OperatorId, PgCodecId, SchemaFingerprint, TableId, TypeId, WireCodecId,
};
use dibs_qgen::generate_compiled_rust;
use dibs_query_ir::{
    ApiFieldName, ApiOperationName, ApiResultTypeName, ApiTypeMapping, ArtifactHashes, BindFormat,
    Cardinality, CatalogRenderName, CatalogRenderNames, CompiledQuery, CompilerVersions,
    ExecutionParameter, ExpressionId, FieldId, HirDelete, HirExpression, HirExpressionKind,
    HirLiteral, HirParameter, HirProjection, HirQuery, HirRelation, HirRelationKind, HirSelect,
    HirStatement, HirStatementKind, LineageGraph, LineageNodeId, ManifestIdentity, Nullability,
    NullabilityEvidence, OrderedBind, OutputField, Parameter, ParameterApiContract,
    ParameterBindAdapter, ParameterId, ParameterPassing, PublicIdentityInput, QueryId,
    QueryManifest, ReadWriteLockManifest, ReferenceIndex, RelationAlias, RelationId, ResultMode,
    RuntimeAssertion, SelectDistinct, Sensitivity, SourceId, SourceMap, SourceOrigin, SourceSpan,
    Span, StatementId, TargetLanguage, TypedArgument, TypedDelete, TypedExpression,
    TypedExpressionKind, TypedLimit, TypedProjection, TypedRelation, TypedRelationKind,
    TypedSelect, TypedStatement, TypedStatementKind, Volatility, execution_identity,
    public_contract_identity,
};

const BIGINT: &str = "pg18:type:pg_catalog.bigint:base";
const BOOLEAN: &str = "pg18:type:pg_catalog.boolean:base";
const WIDGET: &str = "pg18:table:public.widget";
const WIDGET_ID: &str = "pg18:column:public.widget.id";
const EQ: &str = "pg18:operator:pg_catalog.=";
const AND: &str = "pg18:operator:pg_catalog.AND";

fn main() {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let fixtures = [
        (
            "many.rs",
            row_query("FindWidgets", "find_widgets", ResultMode::Many, true),
        ),
        (
            "optional.rs",
            row_query(
                "FindOptionalWidget",
                "find_optional_widget",
                ResultMode::Optional,
                false,
            ),
        ),
        (
            "one.rs",
            row_query(
                "FindRequiredWidget",
                "find_required_widget",
                ResultMode::One,
                false,
            ),
        ),
        ("exec.rs", exec_query()),
    ];

    for (file_name, query) in fixtures {
        let generated = generate_compiled_rust(&query).expect("fixture query generates Rust");
        std::fs::write(out_dir.join(file_name), generated.source)
            .expect("generated fixture source is writable");
    }
}

fn row_query(
    query_name: &str,
    operation_name: &str,
    mode: ResultMode,
    with_bind_contracts: bool,
) -> CompiledQuery {
    let query_id = QueryId::new(1);
    let statement_id = StatementId::new(1);
    let relation_id = RelationId::new(1);
    let field_id = FieldId::new(1);
    let column_id = ColumnId::new(WIDGET_ID);
    let table_id = TableId::new(WIDGET);
    let output_expression_id = ExpressionId::new(1);
    let parameters = if with_bind_contracts {
        vec![
            parameter(
                1,
                "direct_id",
                "i64",
                ParameterPassing::SharedReference,
                ParameterBindAdapter::Direct,
            ),
            parameter(
                2,
                "wrapped_id",
                "crate::WrappedId",
                ParameterPassing::SharedReference,
                ParameterBindAdapter::Deref,
            ),
        ]
    } else {
        Vec::new()
    };
    let hir_parameters = parameters.iter().map(hir_parameter).collect::<Vec<_>>();
    let typed_column = typed_column(output_expression_id, relation_id, column_id.clone());
    let hir_column = hir_column(output_expression_id, relation_id, column_id.clone());
    let (typed_predicate, hir_predicate) = if with_bind_contracts {
        let typed_first = typed_operator(
            ExpressionId::new(3),
            EQ,
            vec![
                typed_column.clone(),
                typed_parameter(ExpressionId::new(2), ParameterId::new(1)),
            ],
        );
        let typed_second = typed_operator(
            ExpressionId::new(5),
            EQ,
            vec![
                typed_column.clone(),
                typed_parameter(ExpressionId::new(4), ParameterId::new(2)),
            ],
        );
        let hir_first = hir_operator(
            ExpressionId::new(3),
            EQ,
            vec![
                hir_column.clone(),
                hir_parameter_expression(ExpressionId::new(2), ParameterId::new(1)),
            ],
        );
        let hir_second = hir_operator(
            ExpressionId::new(5),
            EQ,
            vec![
                hir_column.clone(),
                hir_parameter_expression(ExpressionId::new(4), ParameterId::new(2)),
            ],
        );
        (
            Some(typed_operator(
                ExpressionId::new(6),
                AND,
                vec![typed_first, typed_second],
            )),
            Some(hir_operator(
                ExpressionId::new(6),
                AND,
                vec![hir_first, hir_second],
            )),
        )
    } else {
        (None, None)
    };
    let limit = matches!(mode, ResultMode::Optional | ResultMode::One);
    let cardinality = match mode {
        ResultMode::Many => Cardinality::many(),
        ResultMode::Optional => Cardinality::at_most_one(),
        ResultMode::One => Cardinality::exactly_one(),
        ResultMode::Exec => unreachable!("row fixture is not exec"),
    };
    let typed_statement = TypedStatement {
        id: statement_id,
        origin: origin(),
        cardinality: cardinality.clone(),
        kind: TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: SelectDistinct::AllRows,
            projections: vec![TypedProjection {
                field_id,
                sql_label: "widget_id".to_string(),
                expression: typed_column,
                coercion: None,
            }],
            from: vec![TypedRelation {
                id: relation_id,
                origin: origin(),
                alias: Some(RelationAlias {
                    name: "widget".to_string(),
                    column_names: Vec::new(),
                }),
                cardinality: Cardinality::many(),
                kind: TypedRelationKind::Table {
                    table_id: table_id.clone(),
                },
            }],
            predicate: typed_predicate,
            group_by: Vec::new(),
            having: None,
            windows: Vec::new(),
            order_by: Vec::new(),
            limit: limit.then_some(TypedLimit::Constant(1)),
            offset: None,
            locks: Vec::new(),
        })),
    };
    let hir_statement = HirStatement {
        id: statement_id,
        origin: origin(),
        kind: HirStatementKind::Select(Box::new(HirSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: SelectDistinct::AllRows,
            projections: vec![HirProjection {
                field_id,
                alias: "widget_id".to_string(),
                alias_origin: origin(),
                expression: hir_column,
            }],
            from: vec![HirRelation {
                id: relation_id,
                origin: origin(),
                alias: Some(RelationAlias {
                    name: "widget".to_string(),
                    column_names: Vec::new(),
                }),
                kind: HirRelationKind::Table {
                    table_id: table_id.clone(),
                },
            }],
            predicate: hir_predicate,
            group_by: Vec::new(),
            having: None,
            windows: Vec::new(),
            order_by: Vec::new(),
            limit: limit.then(|| HirExpression {
                id: ExpressionId::new(100),
                origin: origin(),
                kind: HirExpressionKind::Literal(HirLiteral::Integer("1".to_string())),
            }),
            offset: None,
            locks: Vec::new(),
        })),
    };
    let output = OutputField {
        id: field_id,
        ordinal: 0,
        sql_label: "widget_id".to_string(),
        public_name: "id".to_string(),
        type_id: TypeId::new(BIGINT),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::BaseColumnNotNull {
            column_id: column_id.clone(),
        }),
        pg_codec_id: PgCodecId::new("pg18:pg-codec:int8"),
        wire_codec_id: WireCodecId::new("wire:postgres:binary:int64-be"),
        api_types: vec![ApiTypeMapping {
            language: TargetLanguage::Rust,
            type_id: ApiTypeId::new("i64"),
        }],
        api_names: vec![ApiFieldName {
            language: TargetLanguage::Rust,
            name: "id".to_string(),
        }],
        source_expression: output_expression_id,
        lineage_root: LineageNodeId::new(1),
        sensitivity: Sensitivity::Public,
    };
    let runtime_assertions = match mode {
        ResultMode::Many => Vec::new(),
        ResultMode::Optional => vec![RuntimeAssertion::AtMostRows { maximum: 1 }],
        ResultMode::One => vec![
            RuntimeAssertion::AtMostRows { maximum: 1 },
            RuntimeAssertion::AtLeastRows { minimum: 1 },
        ],
        ResultMode::Exec => unreachable!("row fixture is not exec"),
    };
    let sql = if with_bind_contracts {
        "SELECT id AS widget_id FROM public.widget WHERE id = $1 AND id = $2"
    } else if limit {
        "SELECT id AS widget_id FROM public.widget LIMIT 1"
    } else {
        "SELECT id AS widget_id FROM public.widget"
    };
    let ordered_binds = if with_bind_contracts {
        vec![
            OrderedBind {
                position: 1,
                parameter_id: ParameterId::new(1),
            },
            OrderedBind {
                position: 2,
                parameter_id: ParameterId::new(2),
            },
        ]
    } else {
        Vec::new()
    };
    completed_query(QueryParts {
        query_id,
        query_name,
        operation_name,
        mode,
        runtime_assertions,
        sql,
        parameters,
        hir_parameters,
        output_fields: vec![output],
        ordered_binds,
        typed_statement,
        hir_statement,
        catalog_render_names: catalog_names(with_bind_contracts),
        read_write_lock_manifest: ReadWriteLockManifest {
            reads: vec![table_id],
            writes: Vec::new(),
            locks: Vec::new(),
            volatility: Volatility::Immutable,
            mutation: None,
        },
    })
}

fn exec_query() -> CompiledQuery {
    let query_id = QueryId::new(2);
    let statement_id = StatementId::new(2);
    let target_binding = RelationId::new(7);
    let table_id = TableId::new(WIDGET);
    let column_id = ColumnId::new(WIDGET_ID);
    let parameter = parameter(
        1,
        "id",
        "i64",
        ParameterPassing::SharedReference,
        ParameterBindAdapter::Direct,
    );
    let hir_parameter = hir_parameter(&parameter);
    let typed_predicate = typed_operator(
        ExpressionId::new(3),
        EQ,
        vec![
            typed_column(ExpressionId::new(1), target_binding, column_id.clone()),
            typed_parameter(ExpressionId::new(2), ParameterId::new(1)),
        ],
    );
    let hir_predicate = hir_operator(
        ExpressionId::new(3),
        EQ,
        vec![
            hir_column(ExpressionId::new(1), target_binding, column_id),
            hir_parameter_expression(ExpressionId::new(2), ParameterId::new(1)),
        ],
    );
    completed_query(QueryParts {
        query_id,
        query_name: "DeleteWidget",
        operation_name: "delete_widget",
        mode: ResultMode::Exec,
        runtime_assertions: vec![RuntimeAssertion::Rowless],
        sql: "DELETE FROM public.widget WHERE id = $1",
        parameters: vec![parameter],
        hir_parameters: vec![hir_parameter],
        output_fields: Vec::new(),
        ordered_binds: vec![OrderedBind {
            position: 1,
            parameter_id: ParameterId::new(1),
        }],
        typed_statement: TypedStatement {
            id: statement_id,
            origin: origin(),
            cardinality: Cardinality::empty(),
            kind: TypedStatementKind::Delete(Box::new(TypedDelete {
                ctes: Vec::new(),
                target: table_id.clone(),
                target_binding,
                using_relations: Vec::new(),
                predicate: Some(typed_predicate),
                returning: Vec::new(),
            })),
        },
        hir_statement: HirStatement {
            id: statement_id,
            origin: origin(),
            kind: HirStatementKind::Delete(Box::new(HirDelete {
                ctes: Vec::new(),
                target: table_id.clone(),
                target_binding,
                using_relations: Vec::new(),
                predicate: Some(hir_predicate),
                returning: Vec::new(),
            })),
        },
        catalog_render_names: catalog_names(false),
        read_write_lock_manifest: ReadWriteLockManifest {
            reads: Vec::new(),
            writes: vec![table_id.clone()],
            locks: Vec::new(),
            volatility: Volatility::Immutable,
            mutation: Some(dibs_query_ir::MutationManifest::Delete {
                target: table_id,
                has_predicate: true,
            }),
        },
    })
}

struct QueryParts<'a> {
    query_id: QueryId,
    query_name: &'a str,
    operation_name: &'a str,
    mode: ResultMode,
    runtime_assertions: Vec<RuntimeAssertion>,
    sql: &'a str,
    parameters: Vec<Parameter>,
    hir_parameters: Vec<HirParameter>,
    output_fields: Vec<OutputField>,
    ordered_binds: Vec<OrderedBind>,
    typed_statement: TypedStatement,
    hir_statement: HirStatement,
    catalog_render_names: CatalogRenderNames,
    read_write_lock_manifest: ReadWriteLockManifest,
}

fn completed_query(parts: QueryParts<'_>) -> CompiledQuery {
    let compiler_versions = CompilerVersions {
        artifact_schema_version: 1,
        compiler_semantic_version: "compile-fixture".to_string(),
        query_language_version: 1,
        supported_postgres_major: 18,
        execution_identity_format_version: 1,
        public_identity_format_version: 1,
        manifest_format_version: 1,
    };
    let schema_fingerprint = SchemaFingerprint::from_hex_for_artifact(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let references = ReferenceIndex::new(Vec::new());
    let source_map = SourceMap::new(Vec::new());
    let lineage = LineageGraph::new(Vec::new(), Vec::new());
    let result_type_names = if parts.mode == ResultMode::Exec {
        Vec::new()
    } else {
        vec![
            ApiResultTypeName::try_new(TargetLanguage::Rust, format!("{}Row", parts.query_name))
                .expect("valid Rust result type name"),
        ]
    };
    let operation_names = vec![
        ApiOperationName::try_new(TargetLanguage::Rust, parts.operation_name)
            .expect("valid Rust operation name"),
    ];
    let execution_semantics_id = execution_identity(&dibs_query_ir::ExecutionIdentityInput {
        version: compiler_versions.execution_identity_format_version,
        postgres_major: 18,
        statement: parts.typed_statement.clone(),
        parameters: parts
            .parameters
            .iter()
            .map(|parameter| ExecutionParameter {
                id: parameter.id,
                type_id: parameter.type_id.clone(),
                typmod: parameter.typmod.clone(),
                nullable: parameter.nullable,
            })
            .collect(),
        result_mode: parts.mode,
        runtime_assertions: parts.runtime_assertions.clone(),
        references: references.clone(),
        read_write_lock_manifest: parts.read_write_lock_manifest.clone(),
        catalog_schema_fingerprint: schema_fingerprint.clone(),
    });
    let public_contract_id = public_contract_identity(&PublicIdentityInput {
        version: compiler_versions.public_identity_format_version,
        query_name: parts.query_name.to_string(),
        operation_names: operation_names.clone(),
        parameters: parts.parameters.clone(),
        result_type_names: result_type_names.clone(),
        output_fields: parts.output_fields.clone(),
        result_mode: parts.mode,
        transport_envelope: None,
    });
    let mut manifest = QueryManifest {
        manifest_format_version: 1,
        query_id: parts.query_id,
        execution_semantics_id: execution_semantics_id.clone(),
        public_contract_id: public_contract_id.clone(),
        compiler_versions: compiler_versions.clone(),
        catalog_schema_fingerprint: schema_fingerprint.clone(),
        operation_names,
        normalized_sql_hash: dibs_query_ir::ContentHash::of_bytes(parts.sql.as_bytes()),
        source_hash: dibs_query_ir::ContentHash::of_bytes(b"compile fixture"),
        result_type_names,
        source_map_hash: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
        generated_output_hashes: Vec::new(),
        parameters: parts.parameters.clone(),
        output_fields: parts.output_fields.clone(),
        inferred_cardinality: parts.typed_statement.cardinality.clone(),
        runtime_assertions: parts.runtime_assertions.clone(),
        relation_edges: Vec::new(),
        cte_dependencies: Vec::new(),
        read_write_lock_manifest: parts.read_write_lock_manifest.clone(),
        lineage: lineage.clone(),
        opaque_analysis_boundaries: Vec::new(),
        plan_baseline_identity: None,
    };
    manifest = manifest.canonicalized();
    let manifest_identity = ManifestIdentity::from_manifest(&manifest).unwrap();
    let artifact_hashes = ArtifactHashes {
        normalized_sql: dibs_query_ir::ContentHash::of_bytes(parts.sql.as_bytes()),
        source: dibs_query_ir::ContentHash::of_bytes(b"compile fixture"),
        source_map: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
        manifest: dibs_query_ir::ContentHash::of_json(&manifest).unwrap(),
        generated_outputs: Vec::new(),
    };
    CompiledQuery {
        compiler_versions,
        catalog_schema_fingerprint: schema_fingerprint,
        query_id: parts.query_id,
        execution_semantics_id,
        public_contract_id,
        manifest_identity,
        query_name: parts.query_name.to_string(),
        query_origin: origin(),
        declared_result_mode: parts.mode,
        inferred_cardinality: parts.typed_statement.cardinality.clone(),
        runtime_assertions: parts.runtime_assertions,
        deterministic_sql: parts.sql.to_string(),
        ordered_bind_map: parts.ordered_binds,
        ordered_parameters: parts.parameters,
        ordered_output_fields: parts.output_fields,
        catalog_render_names: parts.catalog_render_names,
        resolved_hir: HirQuery {
            id: parts.query_id,
            name: parts.query_name.to_string(),
            origin: origin(),
            parameters: parts.hir_parameters,
            statement: parts.hir_statement,
        },
        typed_statement: parts.typed_statement,
        resolved_references: references,
        lineage,
        read_write_lock_manifest: parts.read_write_lock_manifest,
        source_map,
        manifest,
        artifact_hashes,
    }
}

fn parameter(
    id: u32,
    name: &str,
    api_type: &str,
    passing: ParameterPassing,
    bind_adapter: ParameterBindAdapter,
) -> Parameter {
    Parameter {
        id: ParameterId::new(id),
        ordinal: id - 1,
        source_name: name.to_string(),
        origin: origin(),
        type_id: TypeId::new(BIGINT),
        typmod: None,
        nullable: false,
        pg_codec_id: PgCodecId::new("pg18:pg-codec:int8"),
        wire_codec_id: WireCodecId::new("wire:postgres:binary:int64-be"),
        bind_format: BindFormat::Binary,
        api_contracts: vec![ParameterApiContract {
            language: TargetLanguage::Rust,
            name: name.to_string(),
            api_type: ApiTypeId::new(api_type),
            passing,
            bind_adapter,
        }],
        sensitivity: Sensitivity::Public,
    }
}

fn hir_parameter(parameter: &Parameter) -> HirParameter {
    HirParameter {
        id: parameter.id,
        ordinal: parameter.ordinal,
        name: parameter.source_name.clone(),
        origin: parameter.origin.clone(),
        type_id: parameter.type_id.clone(),
        typmod: parameter.typmod.clone(),
        nullable: parameter.nullable,
    }
}

fn typed_column(id: ExpressionId, binding: RelationId, column_id: ColumnId) -> TypedExpression {
    TypedExpression {
        id,
        origin: origin(),
        type_id: TypeId::new(BIGINT),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::BaseColumnNotNull {
            column_id: column_id.clone(),
        }),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Column { binding, column_id },
    }
}

fn hir_column(id: ExpressionId, binding: RelationId, column_id: ColumnId) -> HirExpression {
    HirExpression {
        id,
        origin: origin(),
        kind: HirExpressionKind::Column { binding, column_id },
    }
}

fn typed_parameter(id: ExpressionId, parameter_id: ParameterId) -> TypedExpression {
    TypedExpression {
        id,
        origin: origin(),
        type_id: TypeId::new(BIGINT),
        typmod: None,
        nullability: Nullability::nullable(NullabilityEvidence::Conservative),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Parameter(parameter_id),
    }
}

fn hir_parameter_expression(id: ExpressionId, parameter_id: ParameterId) -> HirExpression {
    HirExpression {
        id,
        origin: origin(),
        kind: HirExpressionKind::Parameter(parameter_id),
    }
}

fn typed_operator(
    id: ExpressionId,
    operator: &str,
    operands: Vec<TypedExpression>,
) -> TypedExpression {
    TypedExpression {
        id,
        origin: origin(),
        type_id: TypeId::new(BOOLEAN),
        typmod: None,
        nullability: Nullability::nullable(NullabilityEvidence::Conservative),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Operator {
            authored_operator_id: OperatorId::new(operator),
            operator_id: OperatorId::new(operator),
            operands: operands
                .into_iter()
                .map(|expression| TypedArgument {
                    expression,
                    coercion: None,
                })
                .collect(),
        },
    }
}

fn hir_operator(id: ExpressionId, operator: &str, operands: Vec<HirExpression>) -> HirExpression {
    HirExpression {
        id,
        origin: origin(),
        kind: HirExpressionKind::Operator {
            operator_id: OperatorId::new(operator),
            operands,
        },
    }
}

fn catalog_names(include_and: bool) -> CatalogRenderNames {
    let mut entries = vec![
        CatalogRenderName::Table {
            id: TableId::new(WIDGET),
            qualified_name: vec!["public".to_string(), "widget".to_string()],
        },
        CatalogRenderName::Column {
            id: ColumnId::new(WIDGET_ID),
            name: "id".to_string(),
        },
        CatalogRenderName::Type {
            id: TypeId::new(BIGINT),
            qualified_name: vec!["pg_catalog".to_string(), "int8".to_string()],
        },
        CatalogRenderName::Type {
            id: TypeId::new(BOOLEAN),
            qualified_name: vec!["pg_catalog".to_string(), "bool".to_string()],
        },
        CatalogRenderName::Operator {
            id: OperatorId::new(EQ),
            qualified_name: vec!["pg_catalog".to_string(), "=".to_string()],
        },
    ];
    if include_and {
        entries.push(CatalogRenderName::Operator {
            id: OperatorId::new(AND),
            qualified_name: vec!["pg_catalog".to_string(), "AND".to_string()],
        });
    }
    CatalogRenderNames::try_new(entries).unwrap()
}

fn origin() -> SourceOrigin {
    SourceOrigin::authored(SourceSpan::new(SourceId::new(1), Span::new(0, 1)))
}
