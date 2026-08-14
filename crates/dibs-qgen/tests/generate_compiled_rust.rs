#[path = "../src/backend/rust.rs"]
mod rust_backend;

use dibs_pg_catalog::{ApiTypeId, PgCodecId, TableId, WireCodecId};
use dibs_query_ir::{
    ApiFieldName, ApiOperationName, ApiResultTypeName, ApiTypeMapping, BindFormat, Cardinality,
    CatalogRenderName, CatalogRenderNames, CompiledQuery, ExpressionId, FieldId, HirDelete,
    HirExpression, HirExpressionKind, HirLiteral, HirParameter, HirProjection, HirQuery, HirSelect,
    HirStatement, HirStatementKind, Nullability, NullabilityEvidence, OrderedBind, OutputField,
    Parameter, ParameterApiContract, ParameterBindAdapter, ParameterId, ParameterPassing, QueryId,
    ResultMode, RuntimeAssertion, SelectDistinct, Sensitivity, SourceOrigin, SourceSpan, Span,
    StatementId, TargetLanguage, TypedDelete, TypedExpression, TypedExpressionKind, TypedLimit,
    TypedProjection, TypedSelect, TypedStatement, TypedStatementKind, Volatility,
};
use dibs_query_syntax::SourceId;
use rust_backend::{GeneratedRust, RustGenerationError, generate_compiled_rust};

#[derive(Clone, Copy)]
struct TypeFixture {
    pg_name: &'static str,
    rust_type: &'static str,
    pg_codec: &'static str,
    wire_codec: &'static str,
}

const BIGINT: TypeFixture = TypeFixture {
    pg_name: "bigint",
    rust_type: "i64",
    pg_codec: "pg18:pg-codec:int8",
    wire_codec: "wire:postgres:binary:int64-be",
};
const TEXT: TypeFixture = TypeFixture {
    pg_name: "text",
    rust_type: "String",
    pg_codec: "pg18:pg-codec:text",
    wire_codec: "wire:postgres:text:utf8",
};
const JSONB: TypeFixture = TypeFixture {
    pg_name: "jsonb",
    rust_type: "Jsonb<facet_value::Value>",
    pg_codec: "pg18:pg-codec:jsonb",
    wire_codec: "wire:postgres:binary:jsonb-v1",
};
const BYTES: TypeFixture = TypeFixture {
    pg_name: "bytea",
    rust_type: "Vec<u8>",
    pg_codec: "pg18:pg-codec:bytea",
    wire_codec: "wire:postgres:binary:bytes",
};

#[derive(Clone)]
struct ParameterFixture {
    id: u32,
    source_name: &'static str,
    api_name: &'static str,
    api_type: &'static str,
    ty: TypeFixture,
    nullable: bool,
    passing: ParameterPassing,
    adapter: ParameterBindAdapter,
}

#[derive(Clone, Copy)]
struct OutputFixture {
    id: u32,
    sql_label: &'static str,
    rust_name: &'static str,
    ty: TypeFixture,
    nullable: bool,
}

#[test]
fn every_result_mode_uses_the_matching_runtime_contract() {
    let fixtures = [
        (
            ResultMode::Many,
            "QueryResult<Vec<LoadWidgetResult>>",
            "many(rows)",
        ),
        (
            ResultMode::Optional,
            "QueryResult<Option<LoadWidgetResult>>",
            "optional(&CONTEXT, rows)",
        ),
        (
            ResultMode::One,
            "QueryResult<LoadWidgetResult>",
            "one(&CONTEXT, rows)",
        ),
    ];

    for (mode, return_type, helper) in fixtures {
        let generated = generate_compiled_rust(&row_query(mode, &[], &[], standard_outputs()))
            .expect("row query generates");
        assert!(
            generated.source.contains(return_type),
            "{}",
            generated.source
        );
        assert!(generated.source.contains(helper), "{}", generated.source);
        assert!(generated.source.contains("let row_count = rows.len();"));
        assert!(
            generated
                .source
                .contains(".trace_rows(&CONTEXT, started, row_count)")
        );
        assert!(
            generated
                .source
                .contains(".trace_query_err(&CONTEXT, started)")
        );
        assert!(!generated.source.contains("LIMIT 1"));
    }

    let generated = generate_compiled_rust(&exec_query(&[direct_parameter(1, "id", BIGINT)]))
        .expect("exec query generates");
    assert!(generated.source.contains("QueryResult<u64>"));
    assert!(generated.source.contains("let affected = client"));
    assert!(generated.source.contains("exec(affected)"));
    assert!(
        generated
            .source
            .contains(".trace_affected(&CONTEXT, started, affected)")
    );
    assert!(
        generated
            .source
            .contains(".trace_query_err(&CONTEXT, started)")
    );
}

#[test]
fn declaration_order_owns_arguments_while_bind_order_may_repeat_and_reorder_them() {
    let parameters = [
        direct_parameter(1, "needle", TEXT),
        direct_parameter(2, "limit_rows", BIGINT),
    ];
    let binds = [2, 1, 1];

    let generated = generate_compiled_rust(&row_query(
        ResultMode::Many,
        &parameters,
        &binds,
        standard_outputs(),
    ))
    .expect("repeated binds generate");

    assert!(
        generated
            .source
            .contains(".query(SQL, &[&limit_rows, &needle, &needle])")
    );
}

#[test]
fn nullable_fields_and_jsonb_bytes_and_scalars_keep_completed_api_mappings() {
    let outputs = &[
        OutputFixture {
            id: 1,
            sql_label: "widget_id",
            rust_name: "id",
            ty: BIGINT,
            nullable: false,
        },
        OutputFixture {
            id: 2,
            sql_label: "display_name",
            rust_name: "name",
            ty: TEXT,
            nullable: true,
        },
        OutputFixture {
            id: 3,
            sql_label: "payload",
            rust_name: "payload",
            ty: JSONB,
            nullable: false,
        },
        OutputFixture {
            id: 4,
            sql_label: "blob",
            rust_name: "blob",
            ty: BYTES,
            nullable: false,
        },
    ];
    let parameters = [
        direct_parameter(1, "blob", BYTES),
        nullable_direct_parameter(2, "maybe_name", TEXT),
    ];

    let generated =
        generate_compiled_rust(&row_query(ResultMode::Many, &parameters, &[1, 2], outputs))
            .expect("codec fixture generates");

    assert!(
        generated
            .source
            .contains("#[facet(rename = \"widget_id\")]")
    );
    assert!(generated.source.contains("pub id: i64"));
    assert!(generated.source.contains("pub name: Option<String>"));
    assert!(
        generated
            .source
            .contains("pub payload: Jsonb<facet_value::Value>")
    );
    assert!(generated.source.contains("pub blob: Vec<u8>"));
    assert!(generated.source.contains("maybe_name: &Option<String>"));
}

#[test]
fn explicit_bind_adapters_use_real_lowering_or_fail_closed() {
    let deref_query = row_query(
        ResultMode::Many,
        &[parameter(
            1,
            "wrapped_source",
            ParameterContractFixture {
                api_name: "wrapped",
                api_type: "WrappedId",
                ty: BIGINT,
                nullable: false,
                passing: ParameterPassing::SharedReference,
                adapter: ParameterBindAdapter::Deref,
            },
        )],
        &[1],
        standard_outputs(),
    );
    let generated = generate_compiled_rust(&deref_query).expect("deref has direct lowering");
    assert!(generated.source.contains(".query(SQL, &[&&**wrapped])"));

    for adapter in [
        ParameterBindAdapter::FacetJsonb,
        ParameterBindAdapter::PgArray,
        ParameterBindAdapter::Named(ApiTypeId::new("TenantModelBind")),
    ] {
        let query = row_query(
            ResultMode::Many,
            &[parameter(
                1,
                "model_source",
                ParameterContractFixture {
                    api_name: "model",
                    api_type: "ApplicationModel",
                    ty: JSONB,
                    nullable: false,
                    passing: ParameterPassing::SharedReference,
                    adapter: adapter.clone(),
                },
            )],
            &[1],
            standard_outputs(),
        );
        assert_eq!(
            generate_compiled_rust(&query),
            Err(RustGenerationError::UnsupportedParameterBindAdapter {
                parameter_id: ParameterId::new(1),
                adapter,
            })
        );
    }
}

#[test]
fn target_owned_names_are_used_without_backend_synthesis() {
    let mut query = row_query(
        ResultMode::Many,
        &[parameter(
            1,
            "authored_parameter",
            ParameterContractFixture {
                api_name: "wire_parameter",
                api_type: "String",
                ty: TEXT,
                nullable: false,
                passing: ParameterPassing::StringSlice,
                adapter: ParameterBindAdapter::Direct,
            },
        )],
        &[1],
        standard_outputs(),
    );
    set_operation_name(&mut query, "load_widget_exact");

    let generated = generate_compiled_rust(&query).expect("target-owned names generate");
    assert!(
        generated
            .source
            .contains("pub async fn load_widget_exact<C>")
    );
    assert!(generated.source.contains("wire_parameter: &str"));
    assert!(!generated.source.contains("authored_parameter: &str"));
}

#[test]
fn generated_source_has_the_compile_time_shape_expected_by_query_crates() {
    let generated = generate_compiled_rust(&row_query(
        ResultMode::One,
        &[direct_parameter(1, "id", BIGINT)],
        &[1],
        standard_outputs(),
    ))
    .expect("compile-shape fixture generates");

    assert_generated_module_shape(&generated);
}

#[test]
fn generation_is_deterministic_and_does_not_mutate_the_artifact() {
    let query = row_query(
        ResultMode::Many,
        &[direct_parameter(1, "needle", TEXT)],
        &[1],
        standard_outputs(),
    );
    let before = query.clone();

    let first = generate_compiled_rust(&query).expect("first generation");
    let second = generate_compiled_rust(&query).expect("second generation");

    assert_eq!(first, second);
    assert_eq!(query, before);
    assert!(first.source.contains(&query.deterministic_sql));
    assert!(first.source.contains(query.execution_semantics_id.as_str()));
}

#[test]
fn generation_fails_closed_when_rust_contract_facts_are_absent() {
    let mut missing_type = row_query(ResultMode::Many, &[], &[], standard_outputs());
    missing_type.ordered_output_fields[0]
        .api_types
        .retain(|mapping| mapping.language != TargetLanguage::Rust);
    finalize_query(&mut missing_type);
    assert_eq!(
        generate_compiled_rust(&missing_type),
        Err(RustGenerationError::MissingRustOutputType {
            field_id: FieldId::new(1),
        })
    );

    let mut missing_name = row_query(ResultMode::Many, &[], &[], standard_outputs());
    missing_name.ordered_output_fields[0]
        .api_names
        .retain(|mapping| mapping.language != TargetLanguage::Rust);
    finalize_query(&mut missing_name);
    assert_eq!(
        generate_compiled_rust(&missing_name),
        Err(RustGenerationError::MissingRustOutputName {
            field_id: FieldId::new(1),
        })
    );

    let mut missing_parameter_contract = row_query(
        ResultMode::Many,
        &[direct_parameter(1, "id", BIGINT)],
        &[1],
        standard_outputs(),
    );
    missing_parameter_contract.ordered_parameters[0]
        .api_contracts
        .retain(|contract| contract.language != TargetLanguage::Rust);
    finalize_query(&mut missing_parameter_contract);
    assert_eq!(
        generate_compiled_rust(&missing_parameter_contract),
        Err(RustGenerationError::MissingRustParameterContract {
            parameter_id: ParameterId::new(1),
        })
    );

    let mut missing_operation = row_query(ResultMode::Many, &[], &[], standard_outputs());
    missing_operation
        .manifest
        .operation_names
        .retain(|name| name.language != TargetLanguage::Rust);
    missing_operation
        .manifest
        .result_type_names
        .retain(|name| name.language != TargetLanguage::Rust);
    finalize_query(&mut missing_operation);
    assert_eq!(
        generate_compiled_rust(&missing_operation),
        Err(RustGenerationError::MissingRustOperationName)
    );
}

#[test]
fn dynamic_limit_assertion_generates_typed_preflight() {
    let mut query = row_query(
        ResultMode::Many,
        &[direct_parameter(1, "row_limit", BIGINT)],
        &[1],
        standard_outputs(),
    );
    query.runtime_assertions = vec![RuntimeAssertion::ValidLimitParameter {
        parameter_id: ParameterId::new(1),
    }];
    finalize_query(&mut query);

    let generated = generate_compiled_rust(&query).expect("dynamic LIMIT preflight generates");
    let preflight = generated
        .source
        .find("valid_limit(&CONTEXT, \"row_limit\", *row_limit)?;")
        .expect("typed LIMIT preflight");
    let execution = generated
        .source
        .find("client\n            .query(SQL")
        .expect("PostgreSQL execution");
    assert!(preflight < execution);
}

#[test]
fn execution_and_decode_paths_attach_context_and_decode_every_row() {
    let generated = generate_compiled_rust(&row_query(
        ResultMode::Optional,
        &[],
        &[],
        standard_outputs(),
    ))
    .expect("optional query generates");

    assert!(
        generated
            .source
            .contains(".await.with_query_context(CONTEXT.clone())?")
    );
    assert!(generated.source.contains("for row in postgres_rows"));
    assert!(
        generated
            .source
            .contains("from_row(&row).with_query_context(CONTEXT.clone())?")
    );
    assert!(!generated.source.contains("into_iter().next()"));
}

fn assert_generated_module_shape(generated: &GeneratedRust) {
    assert!(
        generated
            .source
            .starts_with("// Generated by dibs-qgen. Do not edit.\n")
    );
    assert!(generated.source.contains("use dibs_runtime::prelude::*;"));
    assert!(
        generated
            .source
            .contains("use dibs_runtime::tokio_postgres;")
    );
    assert!(generated.source.contains("#[derive(Debug, Clone, Facet)]"));
    assert!(
        generated
            .source
            .contains("#[facet(crate = dibs_runtime::facet)]")
    );
    assert!(generated.source.contains("pub struct LoadWidgetResult"));
    assert!(generated.source.contains("pub async fn load_widget<C>"));
    assert!(
        generated
            .source
            .contains("where\n    C: tokio_postgres::GenericClient,")
    );
    assert!(
        generated
            .source
            .contains("const CONTEXT: QueryContext = QueryContext::from_static")
    );
    assert!(generated.source.contains("const SQL: &str ="));
}

struct ParameterContractFixture {
    api_name: &'static str,
    api_type: &'static str,
    ty: TypeFixture,
    nullable: bool,
    passing: ParameterPassing,
    adapter: ParameterBindAdapter,
}

fn parameter(
    id: u32,
    source_name: &'static str,
    contract: ParameterContractFixture,
) -> ParameterFixture {
    let ParameterContractFixture {
        api_name,
        api_type,
        ty,
        nullable,
        passing,
        adapter,
    } = contract;
    ParameterFixture {
        id,
        source_name,
        api_name,
        api_type,
        ty,
        nullable,
        passing,
        adapter,
    }
}

fn direct_parameter(id: u32, name: &'static str, ty: TypeFixture) -> ParameterFixture {
    parameter(
        id,
        name,
        ParameterContractFixture {
            api_name: name,
            api_type: ty.rust_type,
            ty,
            nullable: false,
            passing: direct_passing(ty),
            adapter: ParameterBindAdapter::Direct,
        },
    )
}

fn nullable_direct_parameter(id: u32, name: &'static str, ty: TypeFixture) -> ParameterFixture {
    parameter(
        id,
        name,
        ParameterContractFixture {
            api_name: name,
            api_type: ty.rust_type,
            ty,
            nullable: true,
            passing: ParameterPassing::SharedReference,
            adapter: ParameterBindAdapter::Direct,
        },
    )
}

fn direct_passing(ty: TypeFixture) -> ParameterPassing {
    match ty.rust_type {
        "String" => ParameterPassing::StringSlice,
        "Vec<u8>" => ParameterPassing::ByteSlice,
        _ => ParameterPassing::SharedReference,
    }
}

fn standard_outputs() -> &'static [OutputFixture] {
    &[OutputFixture {
        id: 1,
        sql_label: "widget_id",
        rust_name: "id",
        ty: BIGINT,
        nullable: false,
    }]
}

fn row_query(
    mode: ResultMode,
    parameters: &[ParameterFixture],
    binds: &[u32],
    outputs: &[OutputFixture],
) -> CompiledQuery {
    let cardinality = match mode {
        ResultMode::Many => Cardinality::many(),
        ResultMode::Optional => Cardinality::at_most_one(),
        ResultMode::One => Cardinality::exactly_one(),
        ResultMode::Exec => unreachable!("use exec_query"),
    };
    let runtime_assertions = match mode {
        ResultMode::Many => Vec::new(),
        ResultMode::Optional => vec![RuntimeAssertion::AtMostRows { maximum: 1 }],
        ResultMode::One => vec![
            RuntimeAssertion::AtMostRows { maximum: 1 },
            RuntimeAssertion::AtLeastRows { minimum: 1 },
        ],
        ResultMode::Exec => unreachable!("use exec_query"),
    };
    let mut query = base_query(parameters, outputs, cardinality, mode, runtime_assertions);
    query.ordered_bind_map = binds
        .iter()
        .enumerate()
        .map(|(index, id)| OrderedBind {
            position: u32::try_from(index + 1).unwrap(),
            parameter_id: ParameterId::new(*id),
        })
        .collect();
    finalize_query(&mut query);
    query
}

fn exec_query(parameters: &[ParameterFixture]) -> CompiledQuery {
    let mut query = base_query(
        parameters,
        &[],
        Cardinality::empty(),
        ResultMode::Exec,
        vec![RuntimeAssertion::Rowless],
    );
    query.ordered_bind_map = parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| OrderedBind {
            position: u32::try_from(index + 1).unwrap(),
            parameter_id: ParameterId::new(parameter.id),
        })
        .collect();
    finalize_query(&mut query);
    query
}

fn base_query(
    parameters: &[ParameterFixture],
    outputs: &[OutputFixture],
    cardinality: Cardinality,
    mode: ResultMode,
    runtime_assertions: Vec<RuntimeAssertion>,
) -> CompiledQuery {
    let query_id = QueryId::new(1);
    let statement_id = StatementId::new(1);
    let query_origin = origin(0, 10);
    let hir_parameters = parameters
        .iter()
        .enumerate()
        .map(|(ordinal, fixture)| HirParameter {
            id: ParameterId::new(fixture.id),
            ordinal: u32::try_from(ordinal).unwrap(),
            name: fixture.source_name.to_string(),
            origin: origin(fixture.id + 10, fixture.id + 11),
            type_id: pg_type_id(fixture.ty),
            typmod: None,
            nullable: fixture.nullable,
        })
        .collect();
    let ordered_parameters: Vec<Parameter> = parameters
        .iter()
        .enumerate()
        .map(|(ordinal, fixture)| Parameter {
            id: ParameterId::new(fixture.id),
            ordinal: u32::try_from(ordinal).unwrap(),
            source_name: fixture.source_name.to_string(),
            origin: origin(fixture.id + 10, fixture.id + 11),
            type_id: pg_type_id(fixture.ty),
            typmod: None,
            nullable: fixture.nullable,
            pg_codec_id: PgCodecId::new(fixture.ty.pg_codec),
            wire_codec_id: WireCodecId::new(fixture.ty.wire_codec),
            bind_format: BindFormat::Binary,
            api_contracts: vec![ParameterApiContract {
                language: TargetLanguage::Rust,
                name: fixture.api_name.to_string(),
                api_type: ApiTypeId::new(fixture.api_type),
                passing: fixture.passing,
                bind_adapter: fixture.adapter.clone(),
            }],
            sensitivity: Sensitivity::Public,
        })
        .collect();
    let hir_projections = outputs
        .iter()
        .map(|fixture| HirProjection {
            field_id: FieldId::new(fixture.id),
            alias: fixture.sql_label.to_string(),
            alias_origin: origin(fixture.id + 20, fixture.id + 21),
            expression: hir_literal(fixture),
        })
        .collect::<Vec<_>>();
    let typed_projections = outputs
        .iter()
        .map(|fixture| TypedProjection {
            field_id: FieldId::new(fixture.id),
            sql_label: fixture.sql_label.to_string(),
            expression: typed_literal(fixture),
            coercion: None,
        })
        .collect::<Vec<_>>();
    let ordered_output_fields: Vec<OutputField> = outputs
        .iter()
        .enumerate()
        .map(|(ordinal, fixture)| OutputField {
            id: FieldId::new(fixture.id),
            ordinal: u32::try_from(ordinal).unwrap(),
            sql_label: fixture.sql_label.to_string(),
            public_name: fixture.rust_name.to_string(),
            type_id: pg_type_id(fixture.ty),
            typmod: None,
            nullability: output_nullability(fixture),
            pg_codec_id: PgCodecId::new(fixture.ty.pg_codec),
            wire_codec_id: WireCodecId::new(fixture.ty.wire_codec),
            api_types: rust_api_types(fixture.ty.rust_type),
            api_names: vec![ApiFieldName {
                language: TargetLanguage::Rust,
                name: fixture.rust_name.to_string(),
            }],
            source_expression: ExpressionId::new(fixture.id),
            lineage_root: dibs_query_ir::LineageNodeId::new(fixture.id),
            sensitivity: Sensitivity::Public,
        })
        .collect();
    let (hir_statement_kind, typed_statement_kind) = if mode == ResultMode::Exec {
        (
            HirStatementKind::Delete(Box::new(HirDelete {
                ctes: Vec::new(),
                target: TableId::new("pg18:table:public.widget"),
                target_binding: dibs_query_ir::RelationId::new(1),
                using_relations: Vec::new(),
                predicate: None,
                returning: Vec::new(),
            })),
            TypedStatementKind::Delete(Box::new(TypedDelete {
                ctes: Vec::new(),
                target: TableId::new("pg18:table:public.widget"),
                target_binding: dibs_query_ir::RelationId::new(1),
                using_relations: Vec::new(),
                predicate: None,
                returning: Vec::new(),
            })),
        )
    } else {
        let limited = matches!(mode, ResultMode::Optional | ResultMode::One);
        (
            HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: hir_projections,
                from: Vec::new(),
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: limited.then(|| HirExpression {
                    id: ExpressionId::new(10_000),
                    origin: origin(8, 9),
                    kind: HirExpressionKind::Literal(HirLiteral::Integer("1".to_string())),
                }),
                offset: None,
                locks: Vec::new(),
            })),
            TypedStatementKind::Select(Box::new(TypedSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: SelectDistinct::AllRows,
                projections: typed_projections,
                from: Vec::new(),
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: limited.then_some(TypedLimit::Constant(1)),
                offset: None,
                locks: Vec::new(),
            })),
        )
    };
    let hir_statement = HirStatement {
        id: statement_id,
        origin: query_origin.clone(),
        kind: hir_statement_kind,
    };
    let typed_statement = TypedStatement {
        id: statement_id,
        origin: query_origin.clone(),
        cardinality: cardinality.clone(),
        kind: typed_statement_kind,
    };

    let source_map = dibs_query_ir::SourceMap::new(Vec::new());
    let compiler_versions = compiler_versions();
    let catalog_schema_fingerprint = schema_fingerprint();
    let read_write_lock_manifest = dibs_query_ir::ReadWriteLockManifest {
        reads: Vec::new(),
        writes: Vec::new(),
        locks: Vec::new(),
        volatility: Volatility::Immutable,
        mutation: None,
    };
    let resolved_references = dibs_query_ir::ReferenceIndex::new(Vec::new());
    let lineage = dibs_query_ir::LineageGraph::new(Vec::new(), Vec::new());
    let deterministic_sql = if mode == ResultMode::Exec {
        "DELETE FROM widget WHERE id = $1".to_string()
    } else {
        "SELECT widget_id, display_name, payload, blob FROM widget WHERE needle = $2 OR needle = $1 OR needle = $3".to_string()
    };
    let ordered_bind_map = Vec::new();
    let execution_semantics_id =
        dibs_query_ir::execution_identity(&dibs_query_ir::ExecutionIdentityInput {
            version: compiler_versions.execution_identity_format_version,
            postgres_major: 18,
            statement: typed_statement.clone(),
            parameters: ordered_parameters
                .iter()
                .map(|parameter| dibs_query_ir::ExecutionParameter {
                    id: parameter.id,
                    type_id: parameter.type_id.clone(),
                    typmod: parameter.typmod.clone(),
                    nullable: parameter.nullable,
                })
                .collect(),
            result_mode: mode,
            runtime_assertions: runtime_assertions.clone(),
            references: resolved_references.clone(),
            read_write_lock_manifest: read_write_lock_manifest.clone(),
            catalog_schema_fingerprint: catalog_schema_fingerprint.clone(),
        });
    let operation_names = vec![rust_operation("load_widget")];
    let result_type_names = if mode == ResultMode::Exec {
        Vec::new()
    } else {
        vec![ApiResultTypeName::try_new(TargetLanguage::Rust, "LoadWidgetResult").unwrap()]
    };
    let public_contract_id =
        dibs_query_ir::public_contract_identity(&dibs_query_ir::PublicIdentityInput {
            version: compiler_versions.public_identity_format_version,
            query_name: "LoadWidget".to_string(),
            operation_names: operation_names.clone(),
            result_type_names: result_type_names.clone(),
            parameters: ordered_parameters.clone(),
            output_fields: ordered_output_fields.clone(),
            result_mode: mode,
            transport_envelope: None,
        });
    let mut manifest = dibs_query_ir::QueryManifest {
        manifest_format_version: 1,
        query_id,
        execution_semantics_id: execution_semantics_id.clone(),
        public_contract_id: public_contract_id.clone(),
        compiler_versions: compiler_versions.clone(),
        catalog_schema_fingerprint: catalog_schema_fingerprint.clone(),
        operation_names,
        result_type_names,
        normalized_sql_hash: dibs_query_ir::ContentHash::of_bytes(deterministic_sql.as_bytes()),
        source_hash: dibs_query_ir::ContentHash::of_bytes(b"fixture"),
        source_map_hash: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
        generated_output_hashes: Vec::new(),
        parameters: ordered_parameters.clone(),
        output_fields: ordered_output_fields.clone(),
        inferred_cardinality: cardinality.clone(),
        runtime_assertions: runtime_assertions.clone(),
        relation_edges: Vec::new(),
        cte_dependencies: Vec::new(),
        read_write_lock_manifest: read_write_lock_manifest.clone(),
        lineage: lineage.clone(),
        opaque_analysis_boundaries: Vec::new(),
        plan_baseline_identity: None,
    };
    manifest = manifest.canonicalized();
    let manifest_identity = dibs_query_ir::ManifestIdentity::from_manifest(&manifest).unwrap();
    let artifact_hashes = dibs_query_ir::ArtifactHashes {
        normalized_sql: dibs_query_ir::ContentHash::of_bytes(deterministic_sql.as_bytes()),
        source: dibs_query_ir::ContentHash::of_bytes(b"fixture"),
        source_map: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
        manifest: dibs_query_ir::ContentHash::of_json(&manifest).unwrap(),
        generated_outputs: Vec::new(),
    };

    CompiledQuery {
        compiler_versions,
        catalog_schema_fingerprint,
        query_id,
        execution_semantics_id,
        public_contract_id,
        manifest_identity,
        query_name: "LoadWidget".to_string(),
        query_origin: query_origin.clone(),
        declared_result_mode: mode,
        inferred_cardinality: cardinality,
        runtime_assertions,
        deterministic_sql,
        ordered_bind_map,
        ordered_parameters,
        ordered_output_fields,
        catalog_render_names: catalog_render_names(parameters, outputs, mode),
        resolved_hir: HirQuery {
            id: query_id,
            name: "LoadWidget".to_string(),
            origin: query_origin,
            parameters: hir_parameters,
            statement: hir_statement,
        },
        typed_statement,
        resolved_references,
        lineage,
        read_write_lock_manifest,
        source_map,
        manifest,
        artifact_hashes,
    }
}

fn finalize_query(query: &mut CompiledQuery) {
    query.execution_semantics_id =
        dibs_query_ir::execution_identity(&query.execution_identity_input());
    query.manifest.execution_semantics_id = query.execution_semantics_id.clone();
    query.manifest.parameters = query.ordered_parameters.clone();
    query.manifest.output_fields = query.ordered_output_fields.clone();
    query.manifest.inferred_cardinality = query.inferred_cardinality.clone();
    query.manifest.runtime_assertions = query.runtime_assertions.clone();
    query.manifest.normalized_sql_hash =
        dibs_query_ir::ContentHash::of_bytes(query.deterministic_sql.as_bytes());
    query.public_contract_id =
        dibs_query_ir::public_contract_identity(&query.public_identity_input());
    query.manifest.public_contract_id = query.public_contract_id.clone();
    query.manifest = query.manifest.canonicalized();
    query.manifest_identity =
        dibs_query_ir::ManifestIdentity::from_manifest(&query.manifest).unwrap();
    query.artifact_hashes.normalized_sql =
        dibs_query_ir::ContentHash::of_bytes(query.deterministic_sql.as_bytes());
    query.artifact_hashes.manifest = dibs_query_ir::ContentHash::of_json(&query.manifest).unwrap();
}

fn set_operation_name(query: &mut CompiledQuery, name: &str) {
    query.manifest.operation_names = vec![rust_operation(name)];
    finalize_query(query);
}

fn rust_operation(name: &str) -> ApiOperationName {
    ApiOperationName::try_new(TargetLanguage::Rust, name).unwrap()
}

fn compiler_versions() -> dibs_query_ir::CompilerVersions {
    dibs_query_ir::CompilerVersions {
        artifact_schema_version: 1,
        compiler_semantic_version: "test".to_string(),
        query_language_version: 1,
        supported_postgres_major: 18,
        execution_identity_format_version: 1,
        public_identity_format_version: 1,
        manifest_format_version: 1,
    }
}

fn schema_fingerprint() -> dibs_pg_catalog::SchemaFingerprint {
    dibs_pg_catalog::SchemaFingerprint::from_hex_for_artifact(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
}

fn catalog_render_names(
    parameters: &[ParameterFixture],
    outputs: &[OutputFixture],
    mode: ResultMode,
) -> CatalogRenderNames {
    let mut entries = Vec::new();
    if mode == ResultMode::Exec {
        entries.push(CatalogRenderName::Table {
            id: TableId::new("pg18:table:public.widget"),
            qualified_name: vec!["public".to_string(), "widget".to_string()],
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    for ty in parameters
        .iter()
        .map(|fixture| fixture.ty)
        .chain(outputs.iter().map(|fixture| fixture.ty))
    {
        let id = pg_type_id(ty);
        if seen.insert(id.as_str().to_string()) {
            entries.push(CatalogRenderName::Type {
                id,
                qualified_name: vec!["pg_catalog".to_string(), ty.pg_name.to_string()],
            });
        }
    }
    for output in outputs.iter().filter(|fixture| !fixture.nullable) {
        let id = dibs_pg_catalog::CallableId::new("pg18:test:literal");
        if seen.insert(id.as_str().to_string()) {
            entries.push(CatalogRenderName::Callable {
                id,
                qualified_name: vec!["pg_catalog".to_string(), "literal".to_string()],
            });
        }
        let _ = output;
    }
    CatalogRenderNames::try_new(entries).unwrap()
}

fn hir_literal(fixture: &OutputFixture) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(fixture.id),
        origin: origin(fixture.id + 30, fixture.id + 31),
        kind: HirExpressionKind::Literal(HirLiteral::Integer("1".to_string())),
    }
}

fn typed_literal(fixture: &OutputFixture) -> TypedExpression {
    TypedExpression {
        id: ExpressionId::new(fixture.id),
        origin: origin(fixture.id + 30, fixture.id + 31),
        type_id: pg_type_id(fixture.ty),
        typmod: None,
        nullability: output_nullability(fixture),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Literal(HirLiteral::Integer("1".to_string())),
    }
}

fn output_nullability(fixture: &OutputFixture) -> Nullability {
    if fixture.nullable {
        Nullability::nullable(NullabilityEvidence::Conservative)
    } else {
        Nullability::not_null(NullabilityEvidence::CallableContract {
            callable_id: dibs_pg_catalog::CallableId::new("pg18:test:literal"),
            proves_non_null: true,
        })
    }
}

fn rust_api_types(rust_type: &str) -> Vec<ApiTypeMapping> {
    vec![ApiTypeMapping {
        language: TargetLanguage::Rust,
        type_id: ApiTypeId::new(rust_type),
    }]
}

fn pg_type_id(ty: TypeFixture) -> dibs_pg_catalog::TypeId {
    dibs_pg_catalog::TypeId::new(format!("pg18:type:pg_catalog.{}:base", ty.pg_name))
}

fn origin(start: u32, end: u32) -> SourceOrigin {
    SourceOrigin::authored(SourceSpan::new(SourceId::new(1), Span::new(start, end)))
}
