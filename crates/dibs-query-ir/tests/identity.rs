use dibs_pg_catalog::{
    ApiTypeId, ColumnId, PgCodecId, SchemaFingerprint, TableId, TypeId, WireCodecId,
};
use dibs_query_ir::{
    ApiFieldName, ApiTypeMapping, ArtifactHashes, BindFormat, Cardinality, CardinalityEvidence,
    CompiledQuery, CompilerVersions, ExecutionIdentity, ExecutionIdentityInput, ExpressionId,
    FieldId, GeneratedContractMember, GeneratedMemberKind, HirExpression, HirExpressionKind,
    HirProjection, HirQuery, HirRelation, HirSelect, HirStatement, LineageEdge, LineageGraph,
    LineageNode, LineageNodeId, ManifestIdentity, Nullability, NullabilityEvidence, OrderedBind,
    OutputField, Parameter, ParameterId, PublicContractIdentity, PublicIdentityInput, QueryId,
    QueryManifest, ReadWriteLockManifest, ReferenceAccess, ReferenceId, ReferenceIndex,
    ReferenceRole, ReferenceTarget, ResolvedReference, ResultMode, RuntimeAssertion, Sensitivity,
    SourceMap, SourceMapEntry, SourceOrigin, SourceSpan, Span, SqlByteRange, SqlNodeId,
    SqlProvenance, StatementId, TargetLanguage, TypedExpression, TypedExpressionKind, TypedNodeId,
    TypedRelation, TypedSelect, TypedStatement, Typmod, Volatility, canonical_manifest_json,
    execution_identity, public_contract_identity,
};
use dibs_query_syntax::SourceId;

fn origin(source: u32, start: u32, end: u32) -> SourceOrigin {
    SourceOrigin::authored(SourceSpan::new(
        SourceId::new(source),
        Span::new(start, end),
    ))
}

fn type_id() -> TypeId {
    TypeId::new("pg18:type:pg_catalog.bigint:base")
}

fn api_types() -> Vec<ApiTypeMapping> {
    vec![
        ApiTypeMapping {
            language: TargetLanguage::Rust,
            type_id: ApiTypeId::new("i64"),
        },
        ApiTypeMapping {
            language: TargetLanguage::TypeScript,
            type_id: ApiTypeId::new("bigint"),
        },
    ]
}

fn fixture_query(alias: &str, alias_origin: SourceOrigin) -> CompiledQuery {
    let query_id = QueryId::new(1);
    let statement_id = StatementId::new(1);
    let relation_id = dibs_query_ir::RelationId::new(1);
    let expression_id = ExpressionId::new(1);
    let field_id = FieldId::new(1);
    let parameter_id = ParameterId::new(1);
    let table_id = TableId::new("pg18:table:public.job");
    let column_id = ColumnId::new("pg18:column:public.job.id");
    let expression_origin = origin(1, 10, 16);

    let hir_expression = HirExpression {
        id: expression_id,
        origin: expression_origin.clone(),
        kind: HirExpressionKind::Column {
            binding: relation_id,
            column_id: column_id.clone(),
        },
    };
    let hir = HirQuery {
        id: query_id,
        name: "FindJob".to_string(),
        origin: origin(1, 0, 40),
        parameters: vec![dibs_query_ir::HirParameter {
            id: parameter_id,
            ordinal: 0,
            name: "id".to_string(),
            origin: origin(1, 4, 6),
            type_id: type_id(),
            typmod: None,
            nullable: false,
        }],
        statement: HirStatement {
            id: statement_id,
            origin: origin(1, 8, 40),
            kind: dibs_query_ir::HirStatementKind::Select(HirSelect {
                ctes: Vec::new(),
                projections: vec![HirProjection {
                    field_id,
                    alias: "id".to_string(),
                    alias_origin,
                    expression: hir_expression.clone(),
                }],
                from: vec![HirRelation {
                    id: relation_id,
                    origin: origin(1, 17, 25),
                    table_id: table_id.clone(),
                    alias: Some(alias.to_string()),
                }],
                predicate: None,
                group_by: Vec::new(),
                having: None,
                order_by: Vec::new(),
                limit: None,
                offset: None,
                locks: Vec::new(),
            }),
        },
    };

    let typed_expression = TypedExpression {
        id: expression_id,
        origin: expression_origin.clone(),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::BaseColumnNotNull {
            column_id: column_id.clone(),
        }),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Column {
            binding: relation_id,
            column_id: column_id.clone(),
        },
    };
    let typed_statement = TypedStatement {
        id: statement_id,
        origin: origin(1, 8, 40),
        cardinality: Cardinality::at_most_one_with(CardinalityEvidence::Limit { limit: 1 }),
        kind: dibs_query_ir::TypedStatementKind::Select(Box::new(TypedSelect {
            ctes: Vec::new(),
            projections: vec![dibs_query_ir::TypedProjection {
                field_id,
                expression: typed_expression.clone(),
            }],
            from: vec![TypedRelation {
                id: relation_id,
                origin: origin(1, 17, 25),
                cardinality: Cardinality::many(),
                kind: dibs_query_ir::TypedRelationKind::Table {
                    table_id: table_id.clone(),
                },
            }],
            predicate: None,
            group_by: Vec::new(),
            having: None,
            order_by: Vec::new(),
            limit: Some(dibs_query_ir::TypedLimit::Constant(1)),
            offset: None,
            locks: Vec::new(),
        })),
    };

    let reference = ResolvedReference {
        id: ReferenceId::new(1),
        query_id,
        enclosing_node: TypedNodeId::Expression(expression_id),
        origin: expression_origin.clone(),
        target: ReferenceTarget::Column(column_id.clone()),
        role: ReferenceRole::Projection,
        access: ReferenceAccess::Read,
        lineage_node: Some(LineageNodeId::new(2)),
        generated_members: vec![GeneratedContractMember {
            language: TargetLanguage::Rust,
            kind: GeneratedMemberKind::OutputField,
            name: "id".to_string(),
        }],
    };
    let references = ReferenceIndex::new(vec![reference]);
    let lineage = LineageGraph::new(
        vec![
            LineageNode {
                id: LineageNodeId::new(1),
                value: dibs_query_ir::LineageValue::OutputField(field_id),
            },
            LineageNode {
                id: LineageNodeId::new(2),
                value: dibs_query_ir::LineageValue::Expression(expression_id),
            },
            LineageNode {
                id: LineageNodeId::new(3),
                value: dibs_query_ir::LineageValue::CatalogColumn(column_id.clone()),
            },
            LineageNode {
                id: LineageNodeId::new(4),
                value: dibs_query_ir::LineageValue::GeneratedMember(GeneratedContractMember {
                    language: TargetLanguage::Rust,
                    kind: GeneratedMemberKind::OutputField,
                    name: "id".to_string(),
                }),
            },
        ],
        vec![
            LineageEdge::derived(LineageNodeId::new(1), LineageNodeId::new(2)),
            LineageEdge::derived(LineageNodeId::new(2), LineageNodeId::new(3)),
            LineageEdge::generated(LineageNodeId::new(1), LineageNodeId::new(4)),
        ],
    );

    let parameter = Parameter {
        id: parameter_id,
        ordinal: 0,
        source_name: "id".to_string(),
        origin: origin(1, 4, 6),
        type_id: type_id(),
        typmod: None,
        nullable: false,
        pg_codec_id: PgCodecId::new("pg18:codec:int8"),
        wire_codec_id: WireCodecId::new("wire:signed-int64"),
        bind_format: BindFormat::Binary,
        api_types: api_types(),
        sensitivity: Sensitivity::Public,
    };
    let output = OutputField {
        id: field_id,
        ordinal: 0,
        sql_label: "id".to_string(),
        public_name: "id".to_string(),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::not_null(NullabilityEvidence::BaseColumnNotNull {
            column_id: column_id.clone(),
        }),
        pg_codec_id: PgCodecId::new("pg18:codec:int8"),
        wire_codec_id: WireCodecId::new("wire:signed-int64"),
        api_types: api_types(),
        api_names: vec![
            ApiFieldName {
                language: TargetLanguage::Rust,
                name: "id".to_string(),
            },
            ApiFieldName {
                language: TargetLanguage::TypeScript,
                name: "id".to_string(),
            },
        ],
        source_expression: expression_id,
        lineage_root: LineageNodeId::new(1),
        sensitivity: Sensitivity::Public,
    };

    let source_map = SourceMap::new(vec![SourceMapEntry {
        sql_node_id: SqlNodeId::new(1),
        typed_node: Some(TypedNodeId::Expression(expression_id)),
        source: Some(expression_origin),
        sql_range: SqlByteRange::new(7, 9),
        provenance: SqlProvenance::Authored,
    }]);

    let compiler_versions = CompilerVersions {
        artifact_schema_version: 1,
        compiler_semantic_version: "0.1.0".to_string(),
        query_language_version: 1,
        supported_postgres_major: 18,
        execution_identity_format_version: 1,
        public_identity_format_version: 1,
        manifest_format_version: 1,
    };
    let execution_input = ExecutionIdentityInput {
        version: compiler_versions.execution_identity_format_version,
        postgres_major: compiler_versions.supported_postgres_major,
        statement: typed_statement.clone(),
        parameters: vec![dibs_query_ir::ExecutionParameter {
            id: parameter_id,
            type_id: type_id(),
            typmod: None,
            nullable: false,
        }],
        result_mode: ResultMode::Optional,
        references: references.clone(),
        read_write_lock_manifest: ReadWriteLockManifest {
            reads: vec![table_id.clone()],
            writes: Vec::new(),
            locks: Vec::new(),
            volatility: Volatility::Immutable,
            mutation: None,
        },
        catalog_schema_fingerprint: SchemaFingerprint::from_hex_for_artifact(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    };
    let public_input = PublicIdentityInput {
        version: compiler_versions.public_identity_format_version,
        query_name: "FindJob".to_string(),
        parameters: vec![parameter.clone()],
        output_fields: vec![output.clone()],
        result_mode: ResultMode::Optional,
        transport_envelope: None,
    };
    let execution_semantics_id = execution_identity(&execution_input);
    let public_contract_id = public_contract_identity(&public_input);

    let manifest = QueryManifest {
        manifest_format_version: 1,
        query_id,
        execution_semantics_id: execution_semantics_id.clone(),
        public_contract_id: public_contract_id.clone(),
        compiler_versions: compiler_versions.clone(),
        catalog_schema_fingerprint: execution_input.catalog_schema_fingerprint.clone(),
        normalized_sql_hash: dibs_query_ir::ContentHash::of_bytes(
            b"SELECT id FROM job WHERE id = $1 LIMIT 1",
        ),
        source_hash: dibs_query_ir::ContentHash::of_bytes(b"fixture source"),
        source_map_hash: dibs_query_ir::ContentHash::of_json(&source_map).unwrap(),
        generated_output_hashes: Vec::new(),
        parameters: vec![parameter.clone()],
        output_fields: vec![output.clone()],
        inferred_cardinality: typed_statement.cardinality.clone(),
        runtime_assertions: vec![RuntimeAssertion::AtMostRows { maximum: 1 }],
        relation_edges: Vec::new(),
        cte_dependencies: Vec::new(),
        read_write_lock_manifest: execution_input.read_write_lock_manifest.clone(),
        lineage: lineage.clone(),
        opaque_analysis_boundaries: Vec::new(),
        plan_baseline_identity: None,
    };
    let manifest_identity = ManifestIdentity::from_manifest(&manifest).unwrap();

    CompiledQuery {
        compiler_versions,
        catalog_schema_fingerprint: execution_input.catalog_schema_fingerprint,
        query_id,
        execution_semantics_id,
        public_contract_id,
        manifest_identity,
        query_name: "FindJob".to_string(),
        query_origin: origin(1, 0, 40),
        declared_result_mode: ResultMode::Optional,
        inferred_cardinality: typed_statement.cardinality.clone(),
        runtime_assertions: vec![RuntimeAssertion::AtMostRows { maximum: 1 }],
        deterministic_sql: "SELECT id FROM job WHERE id = $1 LIMIT 1".to_string(),
        ordered_bind_map: vec![OrderedBind {
            position: 1,
            parameter_id,
        }],
        ordered_parameters: vec![parameter],
        ordered_output_fields: vec![output],
        resolved_hir: hir,
        typed_statement,
        resolved_references: references,
        lineage,
        read_write_lock_manifest: execution_input.read_write_lock_manifest,
        source_map,
        manifest,
        artifact_hashes: ArtifactHashes {
            normalized_sql: dibs_query_ir::ContentHash::of_bytes(
                b"SELECT id FROM job WHERE id = $1 LIMIT 1",
            ),
            source: dibs_query_ir::ContentHash::of_bytes(b"fixture source"),
            source_map: dibs_query_ir::ContentHash::of_bytes(b"fixture map"),
            manifest: dibs_query_ir::ContentHash::of_bytes(b"fixture manifest"),
            generated_outputs: Vec::new(),
        },
    }
}

#[test]
fn limit_one_changes_upper_bound_not_lower_bound() {
    assert_eq!(Cardinality::many().limit(1), Cardinality::at_most_one());
    assert_eq!(
        Cardinality::at_least_one().limit(1),
        Cardinality::exactly_one()
    );
}

#[test]
fn nullability_non_null_requires_positive_proof() {
    let nullable = Nullability::nullable(NullabilityEvidence::OuterJoinNullExtension {
        binding: dibs_query_ir::RelationId::new(4),
    });
    let non_null = Nullability::not_null(NullabilityEvidence::BaseColumnNotNull {
        column_id: ColumnId::new("pg18:column:public.job.id"),
    });

    assert!(nullable.is_nullable());
    assert!(!non_null.is_nullable());
    assert!(non_null.has_non_null_proof());
}

#[test]
fn references_are_role_typed_and_lineage_reaches_catalog_columns() {
    let query = fixture_query("job", origin(1, 21, 24));
    let column_id = ColumnId::new("pg18:column:public.job.id");

    let matches = query
        .resolved_references
        .references_to(&ReferenceTarget::Column(column_id.clone()));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].role, ReferenceRole::Projection);
    assert_eq!(matches[0].access, ReferenceAccess::Read);
    assert!(
        query
            .lineage
            .catalog_columns_for_field(FieldId::new(1))
            .contains(&column_id)
    );
}

#[test]
fn source_map_exact_forward_and_reverse_lookups_keep_repeated_fragments() {
    let shared = origin(4, 20, 23);
    let map = SourceMap::new(vec![
        SourceMapEntry {
            sql_node_id: SqlNodeId::new(1),
            typed_node: Some(TypedNodeId::Expression(ExpressionId::new(7))),
            source: Some(shared.clone()),
            sql_range: SqlByteRange::new(8, 10),
            provenance: SqlProvenance::Authored,
        },
        SourceMapEntry {
            sql_node_id: SqlNodeId::new(2),
            typed_node: Some(TypedNodeId::Expression(ExpressionId::new(7))),
            source: Some(shared.clone()),
            sql_range: SqlByteRange::new(25, 27),
            provenance: SqlProvenance::Authored,
        },
        SourceMapEntry {
            sql_node_id: SqlNodeId::new(3),
            typed_node: None,
            source: None,
            sql_range: SqlByteRange::new(10, 11),
            provenance: SqlProvenance::GeneratedPunctuation,
        },
    ]);

    let forward = map.entries_for_source(shared.span());
    assert_eq!(forward.len(), 2);
    assert_eq!(forward[0].sql_range, SqlByteRange::new(8, 10));
    assert_eq!(forward[1].sql_range, SqlByteRange::new(25, 27));
    assert_eq!(map.entries_at_sql_offset(9).len(), 1);
    assert_eq!(map.entries_at_sql_offset(10).len(), 1);
    assert_eq!(
        map.entries_at_sql_offset(10)[0].provenance,
        SqlProvenance::GeneratedPunctuation
    );
    assert_eq!(map.entries_at_sql_offset(25).len(), 1);
    assert!(map.entries_at_sql_offset(27).is_empty());
    assert_eq!(
        map.entries_overlapping_sql(SqlByteRange::new(9, 26)).len(),
        3
    );
}

#[test]
fn unordered_inputs_are_canonical_but_semantic_order_is_preserved() {
    let a = fixture_query("job", origin(1, 21, 24));
    let mut b = a.clone();
    b.resolved_references.reverse_unordered_for_test();
    b.lineage.reverse_unordered_for_test();
    b.read_write_lock_manifest.reads.reverse();
    b.manifest.read_write_lock_manifest.reads.reverse();

    assert_eq!(
        execution_identity(&a.execution_identity_input()),
        execution_identity(&b.execution_identity_input())
    );
    assert_eq!(
        canonical_manifest_json(&a.manifest).unwrap(),
        canonical_manifest_json(&b.manifest).unwrap()
    );

    let mut reordered_parameters = a.public_identity_input();
    reordered_parameters.parameters.reverse();
    reordered_parameters.parameters.push(Parameter {
        id: ParameterId::new(2),
        ordinal: 1,
        source_name: "other".to_string(),
        origin: origin(1, 41, 46),
        type_id: type_id(),
        typmod: Some(Typmod::new("numeric(20,6)")),
        nullable: true,
        pg_codec_id: PgCodecId::new("pg18:codec:numeric"),
        wire_codec_id: WireCodecId::new("wire:numeric-decimal"),
        bind_format: BindFormat::Text,
        api_types: api_types(),
        sensitivity: Sensitivity::Confidential,
    });
    assert_ne!(
        public_contract_identity(&a.public_identity_input()),
        public_contract_identity(&reordered_parameters)
    );
}

#[test]
fn alias_and_span_changes_do_not_change_execution_identity() {
    let a = fixture_query("job", origin(1, 21, 24));
    let b = fixture_query("j", origin(2, 50, 53));

    assert_eq!(
        execution_identity(&a.execution_identity_input()),
        execution_identity(&b.execution_identity_input())
    );
    assert_eq!(a.execution_semantics_id, b.execution_semantics_id);

    let mut renamed_output = b.public_identity_input();
    renamed_output.output_fields[0].public_name = "job_id".to_string();
    assert_ne!(
        public_contract_identity(&a.public_identity_input()),
        public_contract_identity(&renamed_output)
    );
}

#[test]
fn execution_and_public_identities_have_separate_inputs() {
    let base = fixture_query("job", origin(1, 21, 24));

    let mut public_rename = base.clone();
    public_rename.query_name = "LoadJob".to_string();
    public_rename.ordered_output_fields[0].public_name = "job_id".to_string();
    assert_eq!(
        execution_identity(&base.execution_identity_input()),
        execution_identity(&public_rename.execution_identity_input())
    );
    assert_ne!(
        public_contract_identity(&base.public_identity_input()),
        public_contract_identity(&public_rename.public_identity_input())
    );

    let mut changed_semantics = base.clone();
    changed_semantics.typed_statement.cardinality = Cardinality::many();
    assert_ne!(
        execution_identity(&base.execution_identity_input()),
        execution_identity(&changed_semantics.execution_identity_input())
    );
}

#[test]
fn facet_json_round_trips_complete_compiled_query() {
    let query = fixture_query("job", origin(1, 21, 24));
    let json = facet_json::to_string(&query).unwrap();
    let decoded: CompiledQuery = facet_json::from_str(&json).unwrap();

    assert_eq!(decoded, query);
    assert_eq!(decoded.compiler_versions.supported_postgres_major, 18);
    assert_eq!(decoded.deterministic_sql, query.deterministic_sql);
    assert_eq!(decoded.ordered_bind_map, query.ordered_bind_map);
}

#[test]
fn manifests_serialize_deterministically_without_map_iteration_order() {
    let a = fixture_query("job", origin(1, 21, 24)).manifest;
    let mut b = a.clone();
    b.lineage.reverse_unordered_for_test();
    b.read_write_lock_manifest.reads.reverse();
    b.generated_output_hashes.reverse();

    let a_json = canonical_manifest_json(&a).unwrap();
    let b_json = canonical_manifest_json(&b).unwrap();
    assert_eq!(a_json, b_json);
    assert_eq!(
        ManifestIdentity::from_manifest(&a).unwrap(),
        ManifestIdentity::from_manifest(&b).unwrap()
    );
}

#[test]
fn identity_types_are_distinct_and_blake3_backed() {
    let query = fixture_query("job", origin(1, 21, 24));
    let execution: &ExecutionIdentity = &query.execution_semantics_id;
    let public: &PublicContractIdentity = &query.public_contract_id;

    assert_eq!(execution.as_str().len(), 64);
    assert_eq!(public.as_str().len(), 64);
    assert_ne!(execution.as_str(), public.as_str());
}
