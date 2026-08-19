use dibs_pg_catalog::{
    ApiTypeId, ColumnId, PgCodecId, SchemaFingerprint, TableId, TypeId, WireCodecId,
};
use dibs_query_ir::{
    ApiFieldName, ApiOperationName, ApiResultTypeName, ApiTypeMapping, ArtifactHashes, BindFormat,
    Cardinality, CardinalityEvidence, CatalogRenderName, CatalogRenderNames, CompiledQuery,
    CompilerVersions, ExecutionIdentity, ExecutionIdentityInput, ExpressionId, FieldId,
    GeneratedContractMember, GeneratedMemberKind, HirExpression, HirExpressionKind, HirInsert,
    HirInsertSource, HirProjection, HirQuery, HirRelation, HirRelationKind, HirSelect,
    HirStatement, LineageEdge, LineageGraph, LineageNode, LineageNodeId, ManifestIdentity,
    Nullability, NullabilityEvidence, OrderedBind, OutputField, Parameter, ParameterApiContract,
    ParameterBindAdapter, ParameterId, ParameterPassing, PublicContractIdentity,
    PublicIdentityInput, QueryId, QueryManifest, ReadWriteLockManifest, ReferenceAccess,
    ReferenceId, ReferenceIndex, ReferenceRole, ReferenceTarget, ResolvedReference, ResultMode,
    RuntimeAssertion, Sensitivity, SourceMap, SourceMapEntry, SourceOrigin, SourceSpan, Span,
    SqlByteRange, SqlNodeId, SqlProvenance, StatementId, TargetLanguage, TypedExpression,
    TypedExpressionKind, TypedInsert, TypedInsertSource, TypedNodeId, TypedRelation, TypedSelect,
    TypedStatement, Typmod, Volatility, canonical_manifest_json, execution_identity,
    public_contract_identity,
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
            kind: dibs_query_ir::HirStatementKind::Select(Box::new(HirSelect {
                recursive: false,
                ctes: Vec::new(),
                distinct: dibs_query_ir::SelectDistinct::AllRows,
                projections: vec![HirProjection {
                    field_id,
                    alias: alias.to_string(),
                    alias_origin,
                    expression: HirExpression {
                        id: expression_id,
                        origin: origin(1, 8, 10),
                        kind: HirExpressionKind::Column {
                            binding: relation_id,
                            column_id: column_id.clone(),
                        },
                    },
                }],
                from: vec![HirRelation {
                    id: relation_id,
                    origin: origin(1, 17, 25),
                    alias: Some(dibs_query_ir::RelationAlias {
                        name: alias.to_string(),
                        column_names: Vec::new(),
                    }),
                    kind: HirRelationKind::Table {
                        table_id: table_id.clone(),
                    },
                }],
                predicate: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
                order_by: Vec::new(),
                limit: Some(HirExpression {
                    id: ExpressionId::new(2),
                    origin: origin(1, 36, 37),
                    kind: HirExpressionKind::Literal(dibs_query_ir::HirLiteral::Integer(
                        "1".to_string(),
                    )),
                }),
                offset: None,
                locks: Vec::new(),
            })),
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
            recursive: false,
            ctes: Vec::new(),
            distinct: dibs_query_ir::SelectDistinct::AllRows,
            projections: vec![dibs_query_ir::TypedProjection {
                sql_label: alias.to_string(),
                field_id,
                expression: typed_expression.clone(),
            }],
            from: vec![TypedRelation {
                id: relation_id,
                origin: origin(1, 17, 25),
                alias: Some(dibs_query_ir::RelationAlias {
                    name: alias.to_string(),
                    column_names: Vec::new(),
                }),
                cardinality: Cardinality::many(),
                kind: dibs_query_ir::TypedRelationKind::Table {
                    table_id: table_id.clone(),
                },
            }],
            predicate: None,
            group_by: Vec::new(),
            windows: Vec::new(),
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
        api_contracts: vec![ParameterApiContract {
            language: TargetLanguage::Rust,
            name: "id".to_string(),
            api_type: ApiTypeId::new("i64"),
            passing: ParameterPassing::SharedReference,
            bind_adapter: ParameterBindAdapter::Direct,
        }],
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
        runtime_assertions: Vec::new(),
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
        operation_names: vec![ApiOperationName {
            language: TargetLanguage::Rust,
            name: "find_job".to_string(),
        }],
        result_type_names: vec![ApiResultTypeName {
            language: TargetLanguage::Rust,
            name: "FindJobResult".to_string(),
        }],
        query_name: "FindJob".to_string(),
        parameters: vec![parameter.clone()],
        output_fields: vec![output.clone()],
        result_mode: ResultMode::Optional,
        transport_envelope: None,
    };
    let execution_semantics_id = execution_identity(&execution_input);
    let public_contract_id = public_contract_identity(&public_input);
    let manifest = QueryManifest {
        operation_names: vec![ApiOperationName {
            language: TargetLanguage::Rust,
            name: "find_job".to_string(),
        }],
        result_type_names: vec![ApiResultTypeName {
            language: TargetLanguage::Rust,
            name: "FindJobResult".to_string(),
        }],
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
        runtime_assertions: Vec::new(),
        relation_edges: Vec::new(),
        cte_dependencies: Vec::new(),
        read_write_lock_manifest: execution_input.read_write_lock_manifest.clone(),
        lineage: lineage.clone(),
        opaque_analysis_boundaries: Vec::new(),
        plan_baseline_identity: None,
    };
    let manifest_identity = ManifestIdentity::from_manifest(&manifest).unwrap();
    let source_map_hash = dibs_query_ir::ContentHash::of_json(&source_map).unwrap();
    let manifest_hash = dibs_query_ir::ContentHash::of_json(&manifest).unwrap();

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
        runtime_assertions: Vec::new(),
        deterministic_sql: "SELECT id FROM job WHERE id = $1 LIMIT 1".to_string(),
        ordered_bind_map: vec![OrderedBind {
            position: 1,
            parameter_id,
        }],
        catalog_render_names: CatalogRenderNames::try_new(vec![
            CatalogRenderName::Table {
                id: table_id.clone(),
                qualified_name: vec!["public".to_string(), "job".to_string()],
            },
            CatalogRenderName::Column {
                id: column_id.clone(),
                name: "id".to_string(),
            },
            CatalogRenderName::Type {
                id: type_id(),
                qualified_name: vec!["pg_catalog".to_string(), "bigint".to_string()],
            },
        ])
        .unwrap(),
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
            source_map: source_map_hash,
            manifest: manifest_hash,
            generated_outputs: Vec::new(),
        },
    }
    .validate()
    .unwrap()
    .to_owned()
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
        api_contracts: vec![ParameterApiContract {
            language: TargetLanguage::Rust,
            name: "other".to_string(),
            api_type: ApiTypeId::new("i64"),
            passing: ParameterPassing::SharedReference,
            bind_adapter: ParameterBindAdapter::Direct,
        }],
        sensitivity: Sensitivity::Confidential,
    });
    assert_ne!(
        public_contract_identity(&a.public_identity_input()),
        public_contract_identity(&reordered_parameters)
    );

    let mut assertions_a = a.execution_identity_input();
    assertions_a.runtime_assertions = vec![
        RuntimeAssertion::AtMostRows { maximum: 1 },
        RuntimeAssertion::AtLeastRows { minimum: 0 },
    ];
    let mut assertions_b = assertions_a.clone();
    assertions_b.runtime_assertions.reverse();
    assert_eq!(
        execution_identity(&assertions_a),
        execution_identity(&assertions_b)
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

    let mut changed_assertions = base.execution_identity_input();
    changed_assertions.runtime_assertions = vec![RuntimeAssertion::AtMostRows { maximum: 1 }];
    assert_ne!(
        execution_identity(&base.execution_identity_input()),
        execution_identity(&changed_assertions)
    );
}

#[test]
fn facet_json_round_trips_complete_compiled_query() {
    let query = fixture_query("job", origin(1, 21, 24));
    let json = facet_json::to_string(&query).unwrap();
    let decoded: CompiledQuery = facet_json::from_str(&json).unwrap();
    decoded.validate().unwrap();
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
    b.result_type_names.extend([
        ApiResultTypeName {
            language: TargetLanguage::TypeScript,
            name: "FindJobResult".to_string(),
        },
        ApiResultTypeName {
            language: TargetLanguage::Swift,
            name: "FindJobResult".to_string(),
        },
    ]);
    let mut a = a;
    a.result_type_names.extend([
        ApiResultTypeName {
            language: TargetLanguage::Swift,
            name: "FindJobResult".to_string(),
        },
        ApiResultTypeName {
            language: TargetLanguage::TypeScript,
            name: "FindJobResult".to_string(),
        },
    ]);

    let a_json = canonical_manifest_json(&a).unwrap();
    let b_json = canonical_manifest_json(&b).unwrap();
    assert_eq!(a_json, b_json);
    assert_eq!(
        ManifestIdentity::from_manifest(&a).unwrap(),
        ManifestIdentity::from_manifest(&b).unwrap()
    );
}

#[test]
fn insert_target_binding_is_explicit_and_execution_relevant() {
    let target = TableId::new("pg18:table:public.job");
    let hir = HirInsert {
        ctes: Vec::new(),
        target: target.clone(),
        target_binding: dibs_query_ir::RelationId::new(7),
        columns: Vec::new(),
        source: HirInsertSource::DefaultValues,
        conflict: None,
        returning: Vec::new(),
    };
    let typed = TypedInsert {
        ctes: Vec::new(),
        target,
        target_binding: dibs_query_ir::RelationId::new(7),
        columns: Vec::new(),
        source: TypedInsertSource::DefaultValues,
        conflict: None,
        returning: Vec::new(),
    };
    assert!(matches!(
        (&hir.target_binding, &typed.target_binding),
        (left, right) if left == right
    ));

    let mut base = fixture_query("job", origin(1, 21, 24)).execution_identity_input();
    base.statement.kind = dibs_query_ir::TypedStatementKind::Insert(Box::new(typed.clone()));
    let mut rebound = base.clone();
    let dibs_query_ir::TypedStatementKind::Insert(insert) = &mut rebound.statement.kind else {
        unreachable!()
    };
    insert.target_binding = dibs_query_ir::RelationId::new(8);
    assert_ne!(execution_identity(&base), execution_identity(&rebound));

    let hir_statement = HirStatement {
        id: base.statement.id,
        origin: base.statement.origin.clone(),
        kind: dibs_query_ir::HirStatementKind::Insert(Box::new(hir)),
    };
    assert!(base.statement.corresponds_to_hir(&hir_statement));

    let mut wrong_target = typed;
    wrong_target.target = TableId::new("pg18:table:public.other_job");
    let wrong_target = TypedStatement {
        id: base.statement.id,
        origin: base.statement.origin,
        cardinality: base.statement.cardinality,
        kind: dibs_query_ir::TypedStatementKind::Insert(Box::new(wrong_target)),
    };
    assert!(!wrong_target.corresponds_to_hir(&hir_statement));
}

#[test]
fn target_owned_result_type_name_is_public_identity_and_facet_contract() {
    let query = fixture_query("job", origin(1, 21, 24));
    let mut public = query.public_identity_input();
    assert_eq!(
        public.result_type_names,
        vec![ApiResultTypeName {
            language: TargetLanguage::Rust,
            name: "FindJobResult".to_string(),
        }]
    );
    let base = public_contract_identity(&public);
    public.result_type_names[0].name = "LoadedJob".to_string();
    assert_ne!(base, public_contract_identity(&public));

    let mut reordered = query.public_identity_input();
    reordered.result_type_names.extend([
        ApiResultTypeName {
            language: TargetLanguage::TypeScript,
            name: "FindJobResult".to_string(),
        },
        ApiResultTypeName {
            language: TargetLanguage::Swift,
            name: "FindJobResult".to_string(),
        },
    ]);
    let ordered_identity = public_contract_identity(&reordered);
    reordered.result_type_names.reverse();
    assert_eq!(ordered_identity, public_contract_identity(&reordered));

    assert!(ApiResultTypeName::try_new(TargetLanguage::Rust, "type").is_err());
    let invalid = r#"{"language":"Rust","name":"type"}"#;
    assert!(facet_json::from_str::<ApiResultTypeName>(invalid).is_err());
}

#[test]
fn compiled_query_rejects_duplicate_or_unpaired_target_names() {
    let mut duplicate_operation = fixture_query("job", origin(1, 21, 24));
    duplicate_operation
        .manifest
        .operation_names
        .push(ApiOperationName {
            language: TargetLanguage::Rust,
            name: "load_job".to_string(),
        });
    assert!(matches!(
        duplicate_operation.validate(),
        Err(dibs_query_ir::CompiledQueryError::PublicApiNameMismatch)
    ));

    let mut missing_result = fixture_query("job", origin(1, 21, 24));
    missing_result.manifest.result_type_names.clear();
    assert!(matches!(
        missing_result.validate(),
        Err(dibs_query_ir::CompiledQueryError::PublicApiNameMismatch)
    ));
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

#[test]
fn hir_relation_topology_covers_every_typed_relation_form() {
    let table = HirRelation {
        id: dibs_query_ir::RelationId::new(1),
        origin: origin(1, 0, 1),
        alias: Some(dibs_query_ir::RelationAlias {
            name: "job".to_string(),
            column_names: Vec::new(),
        }),
        kind: HirRelationKind::Table {
            table_id: TableId::new("pg18:table:public.job"),
        },
    };
    let relation = HirRelation {
        id: dibs_query_ir::RelationId::new(2),
        origin: origin(1, 0, 10),
        alias: None,
        kind: HirRelationKind::Join {
            kind: dibs_query_ir::JoinKind::Inner,
            left: Box::new(table),
            right: Box::new(HirRelation {
                id: dibs_query_ir::RelationId::new(3),
                origin: origin(1, 2, 4),
                alias: None,
                kind: HirRelationKind::Cte {
                    cte_id: dibs_query_ir::CteId::new(1),
                },
            }),
            predicate: None,
            lateral: false,
        },
    };
    assert!(matches!(relation.kind, HirRelationKind::Join { .. }));

    let variants = [
        HirRelationKind::Subquery(Box::new(relation_statement_fixture())),
        HirRelationKind::Function {
            callable_id: dibs_pg_catalog::CallableId::new("pg18:callable:app.jobs"),
            arguments: Vec::new(),
        },
        HirRelationKind::Values {
            rows: dibs_query_ir::HirValues::try_new(vec![vec![hir_integer("1")]]).unwrap(),
        },
        HirRelationKind::SetOperation {
            kind: dibs_query_ir::SetOperationKind::Union,
            all: true,
            left: Box::new(relation_statement_fixture()),
            right: Box::new(relation_statement_fixture()),
        },
    ];
    assert_eq!(variants.len(), 4);
}

#[test]
fn typed_conflict_target_preserves_named_constraint_exclusively() {
    let constraint =
        dibs_pg_catalog::ConstraintId::new("pg18:constraint:public.job:job_external_key");
    let clause = dibs_query_ir::TypedConflictClause {
        target: dibs_query_ir::ConflictTarget::Constraint(constraint.clone()),
        action: dibs_query_ir::TypedConflictAction::Nothing,
    };

    assert_eq!(
        clause.target,
        dibs_query_ir::ConflictTarget::Constraint(constraint)
    );
}

#[test]
fn typed_conflict_clause_rejects_impossible_postgresql_forms() {
    let update = dibs_query_ir::TypedConflictAction::Update {
        assignments: Vec::new(),
        predicate: None,
    };
    let unspecified_update = dibs_query_ir::TypedConflictClause {
        target: dibs_query_ir::ConflictTarget::Unspecified,
        action: update.clone(),
    };
    assert!(unspecified_update.validate().is_err());

    let empty_inference = dibs_query_ir::TypedConflictClause {
        target: dibs_query_ir::ConflictTarget::Inference {
            expressions: Vec::new(),
            predicate: None,
        },
        action: dibs_query_ir::TypedConflictAction::Nothing,
    };
    assert!(empty_inference.validate().is_err());

    let empty_update = dibs_query_ir::TypedConflictClause {
        target: dibs_query_ir::ConflictTarget::Constraint(dibs_pg_catalog::ConstraintId::new(
            "pg18:constraint:public.job:job_pkey",
        )),
        action: update,
    };
    assert!(empty_update.validate().is_err());
}

#[test]
fn typed_arguments_and_values_cannot_desynchronize() {
    let argument = dibs_query_ir::TypedArgument {
        expression: typed_integer("1"),
        coercion: None,
    };
    let call = TypedExpressionKind::Call(Box::new(dibs_query_ir::TypedCall {
        authored_callable_id: dibs_pg_catalog::CallableId::new("pg18:callable:app.identity"),
        callable_id: dibs_pg_catalog::CallableId::new("pg18:callable:app.identity"),
        arguments: vec![argument],
        distinct: false,
        star: false,
        order_by: Vec::new(),
        filter: None,
        within_group: Vec::new(),
        over: None,
    }));
    assert!(matches!(call, TypedExpressionKind::Call(call) if call.arguments.len() == 1));

    assert!(dibs_query_ir::TypedValues::try_new(Vec::new()).is_err());
    assert!(
        dibs_query_ir::TypedValues::try_new(vec![
            vec![typed_integer("1")],
            vec![typed_integer("2"), typed_integer("3")],
        ])
        .is_err()
    );
}

#[test]
fn hir_correspondence_uses_authored_ids_while_execution_uses_resolved_ids() {
    let hir_call = HirExpression {
        id: ExpressionId::new(70),
        origin: origin(1, 0, 8),
        kind: HirExpressionKind::Call(Box::new(dibs_query_ir::HirCall {
            callable_id: dibs_pg_catalog::CallableId::new("unresolved:function:app.pick"),
            arguments: vec![hir_integer("1")],
            distinct: false,
            star: false,
            order_by: Vec::new(),
            filter: None,
            within_group: Vec::new(),
            over: None,
        })),
    };
    let typed_call = TypedExpression {
        id: ExpressionId::new(70),
        origin: origin(1, 0, 8),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::nullable(NullabilityEvidence::Conservative),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Call(Box::new(dibs_query_ir::TypedCall {
            authored_callable_id: dibs_pg_catalog::CallableId::new("unresolved:function:app.pick"),
            callable_id: dibs_pg_catalog::CallableId::new(
                "pg18:callable:app.pick(pg_catalog.bigint)",
            ),
            arguments: vec![dibs_query_ir::TypedArgument {
                expression: typed_integer_with_nullability("1", true),
                coercion: None,
            }],
            distinct: false,
            star: false,
            order_by: Vec::new(),
            filter: None,
            within_group: Vec::new(),
            over: None,
        })),
    };
    let typed_call_statement = typed_expression_fixture_statement(typed_call.clone());
    let hir_call_statement = hir_expression_fixture_statement(hir_call);
    assert!(typed_call_statement.corresponds_to_hir(&hir_call_statement));
    let mut wrong_authored_call = typed_call_statement.clone();
    let dibs_query_ir::TypedStatementKind::Select(select) = &mut wrong_authored_call.kind else {
        unreachable!()
    };
    let TypedExpressionKind::Call(call) = &mut select.projections[0].expression.kind else {
        unreachable!()
    };
    call.authored_callable_id = dibs_pg_catalog::CallableId::new("unresolved:function:app.other");
    assert!(!wrong_authored_call.corresponds_to_hir(&hir_call_statement));

    let hir_operator = HirExpression {
        id: ExpressionId::new(71),
        origin: origin(1, 0, 5),
        kind: HirExpressionKind::Operator {
            operator_id: dibs_pg_catalog::OperatorId::new("unresolved:operator:pg_catalog.+"),
            operands: vec![hir_integer("1"), hir_integer("1")],
        },
    };
    let typed_operator = TypedExpression {
        id: ExpressionId::new(71),
        origin: origin(1, 0, 5),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::nullable(NullabilityEvidence::Conservative),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Operator {
            authored_operator_id: dibs_pg_catalog::OperatorId::new(
                "unresolved:operator:pg_catalog.+",
            ),
            operator_id: dibs_pg_catalog::OperatorId::new(
                "pg18:operator:pg_catalog.+(pg_catalog.bigint,pg_catalog.bigint)",
            ),
            operands: vec![
                dibs_query_ir::TypedArgument {
                    expression: typed_integer_with_nullability("1", true),
                    coercion: None,
                },
                dibs_query_ir::TypedArgument {
                    expression: typed_integer_with_nullability("1", true),
                    coercion: None,
                },
            ],
        },
    };
    let typed_operator_statement = typed_expression_fixture_statement(typed_operator);
    let hir_operator_statement = hir_expression_fixture_statement(hir_operator);
    assert!(typed_operator_statement.corresponds_to_hir(&hir_operator_statement));
    let mut wrong_authored_operator = typed_operator_statement;
    let dibs_query_ir::TypedStatementKind::Select(select) = &mut wrong_authored_operator.kind
    else {
        unreachable!()
    };
    let TypedExpressionKind::Operator {
        authored_operator_id,
        ..
    } = &mut select.projections[0].expression.kind
    else {
        unreachable!()
    };
    *authored_operator_id = dibs_pg_catalog::OperatorId::new("unresolved:operator:pg_catalog.-");
    assert!(!wrong_authored_operator.corresponds_to_hir(&hir_operator_statement));

    let base_execution = fixture_query("job", origin(1, 21, 24)).execution_identity_input();
    let mut first = base_execution.clone();
    first.statement = typed_expression_fixture_statement(typed_call.clone());
    let mut second = first.clone();
    {
        let dibs_query_ir::TypedStatementKind::Select(select) = &mut second.statement.kind else {
            unreachable!()
        };
        let TypedExpressionKind::Call(call) = &mut select.projections[0].expression.kind else {
            unreachable!()
        };
        call.authored_callable_id = dibs_pg_catalog::CallableId::new("unresolved:function:renamed");
    }
    assert_eq!(execution_identity(&first), execution_identity(&second));
    let dibs_query_ir::TypedStatementKind::Select(select) = &mut second.statement.kind else {
        unreachable!()
    };
    let TypedExpressionKind::Call(call) = &mut select.projections[0].expression.kind else {
        unreachable!()
    };
    call.callable_id = dibs_pg_catalog::CallableId::new("pg18:callable:app.pick(pg_catalog.text)");
    assert_ne!(execution_identity(&first), execution_identity(&second));
}

#[test]
fn cte_output_fields_must_match_statement_projections() {
    let statement = Box::new(TypedStatement {
        id: StatementId::new(20),
        origin: origin(1, 0, 1),
        cardinality: Cardinality::many(),
        kind: dibs_query_ir::TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: dibs_query_ir::SelectDistinct::AllRows,
            projections: vec![dibs_query_ir::TypedProjection {
                field_id: FieldId::new(20),
                sql_label: "value".to_string(),
                expression: typed_integer("1"),
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
    assert!(
        dibs_query_ir::TypedCte::try_new(
            dibs_query_ir::CteId::new(20),
            "example".to_string(),
            dibs_query_ir::CteMaterialization::Default,
            statement,
            vec![FieldId::new(99)],
            vec!["value".to_string()],
        )
        .is_err()
    );
}

#[test]
fn proof_types_reject_impossible_or_unproven_states_during_json_decode() {
    assert!(
        Cardinality::try_new(
            dibs_query_ir::LowerBound::One,
            dibs_query_ir::UpperBound::Zero,
            vec![CardinalityEvidence::EmptyRelation],
        )
        .is_err()
    );
    assert!(Nullability::try_not_null(NullabilityEvidence::Conservative).is_err());

    let cardinality_json = r#"{"lower":"One","upper":"Zero","proof":["EmptyRelation"]}"#;
    assert!(facet_json::from_str::<Cardinality>(cardinality_json).is_err());
    let nullability_json = r#"{"nullable":false,"evidence":["Conservative"]}"#;
    assert!(facet_json::from_str::<Nullability>(nullability_json).is_err());
}

#[test]
fn checked_compiled_query_rejects_cross_surface_mismatches() {
    let valid = fixture_query("job", origin(1, 21, 24));
    assert!(valid.validate().is_ok());

    let mut invalid_bind = valid.clone();
    invalid_bind.ordered_bind_map[0].position = 2;
    assert!(matches!(
        invalid_bind.validate(),
        Err(dibs_query_ir::CompiledQueryError::NonContiguousBindPosition { .. })
    ));

    let mut invalid_cardinality = valid.clone();
    invalid_cardinality.inferred_cardinality = Cardinality::many();
    assert!(matches!(
        invalid_cardinality.validate(),
        Err(dibs_query_ir::CompiledQueryError::CardinalityMismatch)
    ));

    let mut invalid_pg = valid;
    invalid_pg.compiler_versions.supported_postgres_major = 17;
    assert!(matches!(
        invalid_pg.validate(),
        Err(dibs_query_ir::CompiledQueryError::UnsupportedPostgresMajor { actual: 17 })
    ));

    let mut invalid_manifest_identity = fixture_query("job", origin(1, 21, 24));
    let mut other_manifest = invalid_manifest_identity.manifest.clone();
    other_manifest.manifest_format_version += 1;
    invalid_manifest_identity.manifest_identity =
        ManifestIdentity::from_manifest(&other_manifest).unwrap();
    assert!(matches!(
        invalid_manifest_identity.validate(),
        Err(dibs_query_ir::CompiledQueryError::ManifestIdentityMismatch)
    ));

    let mut missing_parameter_type = fixture_query("job", origin(1, 21, 24));
    missing_parameter_type.ordered_parameters[0].type_id =
        TypeId::new("pg18:type:base:pg_catalog.integer");
    missing_parameter_type.resolved_hir.parameters[0].type_id =
        missing_parameter_type.ordered_parameters[0].type_id.clone();
    assert!(matches!(
        missing_parameter_type.validate(),
        Err(dibs_query_ir::CompiledQueryError::MissingCatalogRenderName)
    ));

    let mut invalid_manifest_version = fixture_query("job", origin(1, 21, 24));
    invalid_manifest_version.manifest.manifest_format_version += 1;
    invalid_manifest_version.manifest_identity =
        ManifestIdentity::from_manifest(&invalid_manifest_version.manifest).unwrap();
    invalid_manifest_version.artifact_hashes.manifest =
        dibs_query_ir::ContentHash::of_json(&invalid_manifest_version.manifest).unwrap();
    assert!(matches!(
        invalid_manifest_version.validate(),
        Err(dibs_query_ir::CompiledQueryError::ManifestMismatch)
    ));
}

#[test]
fn checked_compiled_query_rejects_hir_typed_divergence() {
    let mut query = fixture_query("job", origin(1, 21, 24));
    let dibs_query_ir::HirStatementKind::Select(select) = &mut query.resolved_hir.statement.kind
    else {
        panic!("fixture must contain SELECT HIR");
    };
    let HirRelationKind::Table { table_id } = &mut select.from[0].kind else {
        panic!("fixture must contain a table relation");
    };
    *table_id = TableId::new("pg18:table:public.other_job");

    assert!(matches!(
        query.validate(),
        Err(dibs_query_ir::CompiledQueryError::HirTypedMismatch)
    ));
}

#[test]
fn structural_syntax_operators_do_not_require_catalog_render_names() {
    let mut query = fixture_query("job", origin(1, 21, 24));
    let dibs_query_ir::TypedStatementKind::Select(select) = &mut query.typed_statement.kind else {
        unreachable!()
    };
    select.predicate = Some(TypedExpression {
        id: ExpressionId::new(90),
        origin: origin(1, 26, 38),
        type_id: type_id(),
        typmod: None,
        nullability: Nullability::nullable(NullabilityEvidence::Conservative),
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Operator {
            authored_operator_id: dibs_pg_catalog::OperatorId::new("pg18:operator:syntax:AND"),
            operator_id: dibs_pg_catalog::OperatorId::new("pg18:operator:syntax:AND"),
            operands: vec![
                dibs_query_ir::TypedArgument {
                    expression: typed_integer_with_nullability("1", true),
                    coercion: None,
                },
                dibs_query_ir::TypedArgument {
                    expression: typed_integer_with_nullability("1", true),
                    coercion: None,
                },
            ],
        },
    });
    let dibs_query_ir::HirStatementKind::Select(select) = &mut query.resolved_hir.statement.kind
    else {
        unreachable!()
    };
    select.predicate = Some(HirExpression {
        id: ExpressionId::new(90),
        origin: origin(1, 26, 38),
        kind: HirExpressionKind::Operator {
            operator_id: dibs_pg_catalog::OperatorId::new("pg18:operator:syntax:AND"),
            operands: vec![hir_integer("1"), hir_integer("1")],
        },
    });
    query.execution_semantics_id = execution_identity(&query.execution_identity_input());
    query.manifest.execution_semantics_id = query.execution_semantics_id.clone();
    query.manifest_identity = ManifestIdentity::from_manifest(&query.manifest).unwrap();
    query.artifact_hashes.manifest = dibs_query_ir::ContentHash::of_json(&query.manifest).unwrap();

    query.validate().unwrap();

    let mut nested_catalog_operand = query;
    let dibs_query_ir::TypedStatementKind::Select(select) =
        &mut nested_catalog_operand.typed_statement.kind
    else {
        unreachable!()
    };
    let Some(TypedExpression {
        kind: TypedExpressionKind::Operator { operands, .. },
        ..
    }) = &mut select.predicate
    else {
        unreachable!()
    };
    operands[0].expression.kind = TypedExpressionKind::Column {
        binding: dibs_query_ir::RelationId::new(1),
        column_id: ColumnId::new("pg18:column:public.missing.id"),
    };
    assert!(matches!(
        nested_catalog_operand.validate(),
        Err(dibs_query_ir::CompiledQueryError::MissingCatalogRenderName)
    ));
}

#[test]
fn result_modes_require_matching_cardinality_assertions_and_row_shape() {
    let mut one_without_lower_assertion = fixture_query("job", origin(1, 21, 24));
    one_without_lower_assertion.declared_result_mode = ResultMode::One;
    one_without_lower_assertion.runtime_assertions =
        vec![RuntimeAssertion::AtMostRows { maximum: 1 }];
    assert!(matches!(
        one_without_lower_assertion.validate(),
        Err(dibs_query_ir::CompiledQueryError::ResultModeMismatch)
    ));

    let mut optional_without_upper_assertion = fixture_query("job", origin(1, 21, 24));
    optional_without_upper_assertion.inferred_cardinality = Cardinality::many();
    optional_without_upper_assertion.typed_statement.cardinality = Cardinality::many();
    optional_without_upper_assertion.runtime_assertions.clear();
    assert!(matches!(
        optional_without_upper_assertion.validate(),
        Err(dibs_query_ir::CompiledQueryError::ResultModeMismatch)
    ));

    let mut exec_with_rows = fixture_query("job", origin(1, 21, 24));
    exec_with_rows.declared_result_mode = ResultMode::Exec;
    exec_with_rows.runtime_assertions = vec![RuntimeAssertion::Rowless];
    assert!(matches!(
        exec_with_rows.validate(),
        Err(dibs_query_ir::CompiledQueryError::ResultModeMismatch)
    ));
}

#[test]
fn result_mode_rejects_empty_or_contradictory_effective_ranges() {
    let mut impossible_static_range = fixture_query("job", origin(1, 21, 24));
    impossible_static_range.declared_result_mode = ResultMode::One;
    impossible_static_range.inferred_cardinality = Cardinality::empty();
    impossible_static_range.typed_statement.cardinality = Cardinality::empty();
    impossible_static_range.runtime_assertions = vec![
        RuntimeAssertion::AtMostRows { maximum: 1 },
        RuntimeAssertion::AtLeastRows { minimum: 1 },
    ];
    assert!(matches!(
        impossible_static_range.validate(),
        Err(dibs_query_ir::CompiledQueryError::ResultModeMismatch)
    ));

    let mut contradictory_assertions = fixture_query("job", origin(1, 21, 24));
    contradictory_assertions.declared_result_mode = ResultMode::One;
    contradictory_assertions.inferred_cardinality = Cardinality::unknown();
    contradictory_assertions.typed_statement.cardinality = Cardinality::unknown();
    contradictory_assertions.runtime_assertions = vec![
        RuntimeAssertion::AtMostRows { maximum: 0 },
        RuntimeAssertion::AtLeastRows { minimum: 1 },
    ];
    assert!(matches!(
        contradictory_assertions.validate(),
        Err(dibs_query_ir::CompiledQueryError::ResultModeMismatch)
    ));
}

#[test]
fn one_mode_accepts_complete_runtime_row_count_assertions() {
    let mut one_with_runtime_proof = fixture_query("job", origin(1, 21, 24));
    one_with_runtime_proof.declared_result_mode = ResultMode::One;
    one_with_runtime_proof.runtime_assertions = vec![
        RuntimeAssertion::AtMostRows { maximum: 1 },
        RuntimeAssertion::AtLeastRows { minimum: 1 },
    ];
    let execution_input = one_with_runtime_proof.execution_identity_input();
    one_with_runtime_proof.execution_semantics_id = execution_identity(&execution_input);
    let public_input = one_with_runtime_proof.public_identity_input();
    one_with_runtime_proof.public_contract_id = public_contract_identity(&public_input);
    one_with_runtime_proof.manifest.execution_semantics_id =
        one_with_runtime_proof.execution_semantics_id.clone();
    one_with_runtime_proof.manifest.public_contract_id =
        one_with_runtime_proof.public_contract_id.clone();
    one_with_runtime_proof.manifest.runtime_assertions =
        one_with_runtime_proof.runtime_assertions.clone();
    one_with_runtime_proof.manifest_identity =
        ManifestIdentity::from_manifest(&one_with_runtime_proof.manifest).unwrap();
    one_with_runtime_proof.artifact_hashes.manifest =
        dibs_query_ir::ContentHash::of_json(&one_with_runtime_proof.manifest).unwrap();
    assert!(one_with_runtime_proof.validate().is_ok());
}

fn hir_integer(value: &str) -> HirExpression {
    HirExpression {
        id: ExpressionId::new(10),
        origin: origin(1, 0, 1),
        kind: HirExpressionKind::Literal(dibs_query_ir::HirLiteral::Integer(value.to_string())),
    }
}

fn typed_integer(value: &str) -> TypedExpression {
    typed_integer_with_nullability(value, false)
}

fn typed_integer_with_nullability(value: &str, nullable: bool) -> TypedExpression {
    TypedExpression {
        id: ExpressionId::new(10),
        origin: origin(1, 0, 1),
        type_id: type_id(),
        typmod: None,
        nullability: if nullable {
            Nullability::nullable(NullabilityEvidence::Conservative)
        } else {
            Nullability::not_null(NullabilityEvidence::CallableContract {
                callable_id: dibs_pg_catalog::CallableId::new("pg18:literal:integer"),
                proves_non_null: true,
            })
        },
        volatility: Volatility::Immutable,
        kind: TypedExpressionKind::Literal(dibs_query_ir::HirLiteral::Integer(value.to_string())),
    }
}

fn relation_statement_fixture() -> HirStatement {
    HirStatement {
        id: StatementId::new(10),
        origin: origin(1, 0, 1),
        kind: dibs_query_ir::HirStatementKind::Select(Box::new(HirSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: dibs_query_ir::SelectDistinct::AllRows,
            projections: Vec::new(),
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
    }
}

fn hir_expression_fixture_statement(expression: HirExpression) -> HirStatement {
    HirStatement {
        id: StatementId::new(70),
        origin: expression.origin.clone(),
        kind: dibs_query_ir::HirStatementKind::Select(Box::new(HirSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: dibs_query_ir::SelectDistinct::AllRows,
            projections: vec![HirProjection {
                field_id: FieldId::new(70),
                alias: "value".to_string(),
                alias_origin: expression.origin.clone(),
                expression,
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
    }
}

fn typed_expression_fixture_statement(expression: TypedExpression) -> TypedStatement {
    TypedStatement {
        id: StatementId::new(70),
        origin: expression.origin.clone(),
        cardinality: Cardinality::exactly_one(),
        kind: dibs_query_ir::TypedStatementKind::Select(Box::new(TypedSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: dibs_query_ir::SelectDistinct::AllRows,
            projections: vec![dibs_query_ir::TypedProjection {
                field_id: FieldId::new(70),
                sql_label: "value".to_string(),
                expression,
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
    }
}
