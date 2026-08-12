mod diagnostic;
mod resolve;
mod scope;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use dibs_pg_catalog::CatalogSnapshot;
use dibs_query_ir::{
    ApiFieldName, ApiOperationName, ApiResultTypeName, ApiTypeMapping, ArtifactHashes, BindFormat,
    CatalogRenderNames, CompiledQuery, CompilerVersions, ContentHash, ExecutionIdentityInput,
    ExecutionParameter, GeneratedContractMember, HirExpression, HirExpressionKind, HirProjection,
    HirRelation, HirRelationKind, LineageEdge, LineageGraph, LineageNode, LineageNodeId,
    LineageValue, OutputField, Parameter, ParameterApiContract, ParameterBindAdapter,
    ParameterPassing, PublicIdentityInput, QueryManifest, ReadWriteLockManifest, ReferenceAccess,
    ReferenceId, ReferenceIndex, ReferenceRole, ReferenceTarget, ResolvedReference, ResultMode,
    Sensitivity, SourceMap, TargetLanguage, TypedExpression, TypedExpressionKind, TypedNodeId,
    TypedStatement, TypedStatementKind, Volatility, execution_identity, public_contract_identity,
};
use dibs_query_syntax::{DibsParser, ResultMode as SyntaxResultMode, SourceId};
use dibs_query_typing::{CheckedOutput, SemanticChecker};

pub use diagnostic::{CompileDiagnostic, CompileDiagnosticCode, DiagnosticSet};

/// Strictly parses, resolves, checks, and compiles every query declaration in one source.
pub fn compile_query_source(
    parser: &DibsParser,
    source_id: SourceId,
    source: &str,
    catalog: &CatalogSnapshot,
) -> Result<Vec<CompiledQuery>, DiagnosticSet> {
    let file = parser
        .parse_strict(source_id, source)
        .map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(CompileDiagnostic::from_syntax)
                .collect::<DiagnosticSet>()
        })?;
    let modes = file
        .queries
        .iter()
        .map(|query| query.result_mode)
        .collect::<Vec<_>>();
    let resolved = resolve::resolve_file(source_id, file, catalog)?;
    let mut compiled = Vec::with_capacity(resolved.len());
    for (resolved, mode) in resolved.into_iter().zip(modes) {
        compiled.push(compile_resolved(
            source,
            catalog,
            resolved.hir,
            result_mode(mode),
        )?);
    }
    Ok(compiled)
}

fn compile_resolved(
    source: &str,
    catalog: &CatalogSnapshot,
    hir: dibs_query_ir::HirQuery,
    mode: ResultMode,
) -> Result<CompiledQuery, DiagnosticSet> {
    let checked = SemanticChecker::new(catalog)
        .check_query(&hir)
        .map_err(|error| check_diagnostic(error, hir.origin.span()))?;
    let runtime_assertions = checked.runtime_assertions.clone();
    checked.validate_mode(mode).map_err(|error| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::ResultModeMismatch,
            hir.origin.span(),
            error.to_string(),
        )]
    })?;

    let compiler_versions = CompilerVersions {
        artifact_schema_version: 1,
        compiler_semantic_version: env!("CARGO_PKG_VERSION").to_string(),
        query_language_version: 1,
        supported_postgres_major: 18,
        execution_identity_format_version: 1,
        public_identity_format_version: 1,
        manifest_format_version: 1,
    };
    let ordered_parameters = build_parameters(catalog, &hir, &checked)?;
    let (ordered_output_fields, lineage) = build_outputs(catalog, &hir, &checked)?;
    let resolved_references = build_references(&hir, &checked.statement)?;
    let read_write_lock_manifest = ReadWriteLockManifest {
        reads: collect_read_tables(&hir.statement),
        writes: Vec::new(),
        locks: Vec::new(),
        volatility: maximum_volatility(&checked.statement),
        mutation: None,
    }
    .canonicalized();
    let catalog_render_names = CatalogRenderNames::from_catalog(catalog).map_err(|error| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            hir.origin.span(),
            error.to_string(),
        )]
    })?;
    let operation_names = vec![
        ApiOperationName::try_new(TargetLanguage::Rust, to_snake_case(&hir.name)).map_err(
            |error| {
                vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::InvalidApiContract,
                    hir.origin.span(),
                    error.to_string(),
                )]
            },
        )?,
    ];
    let result_type_names = if mode == ResultMode::Exec {
        Vec::new()
    } else {
        vec![
            ApiResultTypeName::try_new(
                TargetLanguage::Rust,
                format!("{}Result", to_pascal_case(&hir.name)),
            )
            .map_err(|error| {
                vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::InvalidApiContract,
                    hir.origin.span(),
                    error.to_string(),
                )]
            })?,
        ]
    };
    let source_map = SourceMap::new(Vec::new());
    let source_hash = ContentHash::of_bytes(source.as_bytes());
    let source_map_hash = ContentHash::of_json(&source_map).map_err(|message| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            hir.origin.span(),
            message,
        )]
    })?;
    let schema_fingerprint = catalog.fingerprint().clone();
    let execution_semantics_id = execution_identity(&ExecutionIdentityInput {
        version: compiler_versions.execution_identity_format_version,
        postgres_major: 18,
        statement: checked.statement.clone(),
        parameters: ordered_parameters
            .iter()
            .map(|parameter| ExecutionParameter {
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
        catalog_schema_fingerprint: schema_fingerprint.clone(),
    });
    let public_contract_id = public_contract_identity(&PublicIdentityInput {
        version: compiler_versions.public_identity_format_version,
        query_name: hir.name.clone(),
        operation_names: operation_names.clone(),
        result_type_names: result_type_names.clone(),
        parameters: ordered_parameters.clone(),
        output_fields: ordered_output_fields.clone(),
        result_mode: mode,
        transport_envelope: None,
    });

    let placeholder_sql = String::new();
    let manifest = QueryManifest {
        manifest_format_version: compiler_versions.manifest_format_version,
        query_id: hir.id,
        execution_semantics_id: execution_semantics_id.clone(),
        public_contract_id: public_contract_id.clone(),
        compiler_versions: compiler_versions.clone(),
        catalog_schema_fingerprint: schema_fingerprint.clone(),
        operation_names,
        result_type_names,
        normalized_sql_hash: ContentHash::of_bytes(placeholder_sql.as_bytes()),
        source_hash: source_hash.clone(),
        source_map_hash: source_map_hash.clone(),
        generated_output_hashes: Vec::new(),
        parameters: ordered_parameters.clone(),
        output_fields: ordered_output_fields.clone(),
        inferred_cardinality: checked.cardinality.clone(),
        runtime_assertions: runtime_assertions.clone(),
        relation_edges: Vec::new(),
        cte_dependencies: Vec::new(),
        read_write_lock_manifest: read_write_lock_manifest.clone(),
        lineage: lineage.clone(),
        opaque_analysis_boundaries: Vec::new(),
        plan_baseline_identity: None,
    }
    .canonicalized();
    let manifest_identity =
        dibs_query_ir::ManifestIdentity::from_manifest(&manifest).map_err(|message| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::InvalidArtifact,
                hir.origin.span(),
                message,
            )]
        })?;
    let manifest_hash = ContentHash::of_json(&manifest).map_err(|message| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            hir.origin.span(),
            message,
        )]
    })?;
    let mut query = CompiledQuery {
        compiler_versions,
        catalog_schema_fingerprint: schema_fingerprint,
        query_id: hir.id,
        execution_semantics_id,
        public_contract_id,
        manifest_identity,
        query_name: hir.name.clone(),
        query_origin: hir.origin.clone(),
        declared_result_mode: mode,
        inferred_cardinality: checked.cardinality,
        runtime_assertions,
        deterministic_sql: placeholder_sql,
        ordered_bind_map: Vec::new(),
        ordered_parameters,
        ordered_output_fields,
        catalog_render_names,
        resolved_hir: hir,
        typed_statement: checked.statement,
        resolved_references,
        lineage,
        read_write_lock_manifest,
        source_map,
        manifest,
        artifact_hashes: ArtifactHashes {
            normalized_sql: ContentHash::of_bytes(b""),
            source: source_hash,
            source_map: source_map_hash,
            manifest: manifest_hash,
            generated_outputs: Vec::new(),
        },
    };
    finish_sql_and_hashes(&mut query)?;
    query.validate().map_err(|error| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            query.query_origin.span(),
            error.to_string(),
        )]
    })?;
    Ok(query)
}

fn finish_sql_and_hashes(query: &mut CompiledQuery) -> Result<(), DiagnosticSet> {
    let rendered = crate::backend::sql::render_compiler_surfaces(query).map_err(|error| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            query.query_origin.span(),
            error.to_string(),
        )]
    })?;
    query.deterministic_sql = rendered.sql;
    query.ordered_bind_map = rendered.ordered_binds;
    let sql_hash = ContentHash::of_bytes(query.deterministic_sql.as_bytes());
    query.artifact_hashes.normalized_sql = sql_hash.clone();
    query.manifest.normalized_sql_hash = sql_hash;
    query.manifest.parameters = query.ordered_parameters.clone();
    query.manifest.output_fields = query.ordered_output_fields.clone();
    query.manifest.inferred_cardinality = query.inferred_cardinality.clone();
    query.manifest.runtime_assertions = query.runtime_assertions.clone();
    query.manifest = query.manifest.canonicalized();
    query.execution_semantics_id = execution_identity(&query.execution_identity_input());
    query.manifest.execution_semantics_id = query.execution_semantics_id.clone();
    query.public_contract_id = public_contract_identity(&query.public_identity_input());
    query.manifest.public_contract_id = query.public_contract_id.clone();
    query.manifest_identity = dibs_query_ir::ManifestIdentity::from_manifest(&query.manifest)
        .map_err(|message| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::InvalidArtifact,
                query.query_origin.span(),
                message,
            )]
        })?;
    query.artifact_hashes.manifest = ContentHash::of_json(&query.manifest).map_err(|message| {
        vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            query.query_origin.span(),
            message,
        )]
    })?;
    Ok(())
}

fn build_parameters(
    catalog: &CatalogSnapshot,
    hir: &dibs_query_ir::HirQuery,
    checked: &CheckedOutput,
) -> Result<Vec<Parameter>, DiagnosticSet> {
    if hir.parameters.len() != checked.parameters.len() {
        return Err(vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            hir.origin.span(),
            "semantic checker returned a different parameter count than resolved HIR",
        )]);
    }
    hir.parameters
        .iter()
        .zip(&checked.parameters)
        .map(|(hir, checked)| {
            if hir.id != checked.id
                || hir.ordinal != checked.ordinal
                || hir.type_id != checked.type_id
                || hir.typmod != checked.typmod
                || hir.nullable != checked.nullable
            {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::InvalidArtifact,
                    hir.origin.span(),
                    "semantic checker parameter facts do not correspond to resolved HIR",
                )]);
            }
            let ty = catalog.type_by_id(&checked.type_id).ok_or_else(|| {
                vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::TypeMismatch,
                    hir.origin.span(),
                    format!(
                        "catalog type '{}' disappeared during compilation",
                        checked.type_id
                    ),
                )]
            })?;
            let (passing, adapter) = rust_parameter_policy(ty.rust_api_type.as_str());
            let contract = ParameterApiContract::try_new(
                TargetLanguage::Rust,
                to_snake_case(&hir.name),
                ty.rust_api_type.clone(),
                passing,
                adapter,
            )
            .map_err(|error| {
                vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::InvalidApiContract,
                    hir.origin.span(),
                    error.to_string(),
                )]
            })?;
            Ok(Parameter {
                id: checked.id,
                ordinal: checked.ordinal,
                source_name: hir.name.clone(),
                origin: hir.origin.clone(),
                type_id: checked.type_id.clone(),
                typmod: checked.typmod.clone(),
                nullable: checked.nullable,
                pg_codec_id: checked.pg_codec_id.clone(),
                wire_codec_id: checked.wire_codec_id.clone(),
                bind_format: BindFormat::Binary,
                api_contracts: vec![contract],
                sensitivity: Sensitivity::Public,
            })
        })
        .collect()
}

fn build_outputs(
    catalog: &CatalogSnapshot,
    hir: &dibs_query_ir::HirQuery,
    checked: &CheckedOutput,
) -> Result<(Vec<OutputField>, LineageGraph), DiagnosticSet> {
    let projections = statement_projections(&hir.statement);
    if projections.len() != checked.output_fields.len() {
        return Err(vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            hir.origin.span(),
            "semantic checker returned a different output count than resolved HIR",
        )]);
    }
    let mut fields = Vec::with_capacity(checked.output_fields.len());
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut projection_sources = BTreeMap::new();
    collect_statement_projection_sources(&hir.statement, &mut projection_sources);
    for (projection, checked_field) in projections.iter().zip(&checked.output_fields) {
        if projection.field_id != checked_field.id
            || projection.expression.id != checked_field.source_expression
            || projection.alias != checked_field.sql_label
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::InvalidArtifact,
                projection.expression.origin.span(),
                "semantic checker output facts do not correspond to resolved HIR",
            )]);
        }
        let ty = catalog.type_by_id(&checked_field.type_id).ok_or_else(|| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::TypeMismatch,
                projection.expression.origin.span(),
                format!(
                    "catalog type '{}' disappeared during compilation",
                    checked_field.type_id
                ),
            )]
        })?;
        let root = LineageNodeId::new(checked_field.id.get());
        let expression_node = LineageNodeId::new(1_000_000 + checked_field.source_expression.get());
        nodes.push(LineageNode {
            id: root,
            value: LineageValue::OutputField(checked_field.id),
        });
        nodes.push(LineageNode {
            id: expression_node,
            value: LineageValue::Expression(checked_field.source_expression),
        });
        edges.push(LineageEdge::derived(expression_node, root));
        collect_expression_lineage(
            &projection.expression,
            expression_node,
            &projection_sources,
            &mut nodes,
            &mut edges,
        );
        fields.push(OutputField {
            id: checked_field.id,
            ordinal: checked_field.ordinal,
            sql_label: checked_field.sql_label.clone(),
            public_name: checked_field.sql_label.clone(),
            type_id: checked_field.type_id.clone(),
            typmod: checked_field.typmod.clone(),
            nullability: checked_field.nullability.clone(),
            pg_codec_id: checked_field.pg_codec_id.clone(),
            wire_codec_id: checked_field.wire_codec_id.clone(),
            api_types: vec![ApiTypeMapping {
                language: TargetLanguage::Rust,
                type_id: ty.rust_api_type.clone(),
            }],
            api_names: vec![ApiFieldName {
                language: TargetLanguage::Rust,
                name: checked_target_name(
                    &checked_field.sql_label,
                    projection.expression.origin.span(),
                )?,
            }],
            source_expression: checked_field.source_expression,
            lineage_root: root,
            sensitivity: Sensitivity::Public,
        });
    }
    Ok((fields, LineageGraph::new(nodes, edges)))
}

fn collect_expression_lineage(
    expression: &HirExpression,
    expression_node: LineageNodeId,
    projection_sources: &BTreeMap<dibs_query_ir::FieldId, HirExpression>,
    nodes: &mut Vec<LineageNode>,
    edges: &mut Vec<LineageEdge>,
) {
    match &expression.kind {
        HirExpressionKind::Column { column_id, .. } => {
            let column_node = LineageNodeId::new(2_000_000 + expression.id.get());
            nodes.push(LineageNode {
                id: column_node,
                value: LineageValue::CatalogColumn(column_id.clone()),
            });
            edges.push(LineageEdge::derived(column_node, expression_node));
        }
        HirExpressionKind::DerivedColumn { field_id, .. } => {
            if let Some(source) = projection_sources.get(field_id) {
                collect_expression_lineage(
                    source,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
        }
        HirExpressionKind::ScalarSubquery(statement) => {
            for projection in statement_projections(statement) {
                collect_expression_lineage(
                    &projection.expression,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
        }
        HirExpressionKind::Call(call) => {
            for argument in &call.arguments {
                collect_expression_lineage(
                    argument,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
            for order in &call.order_by {
                collect_expression_lineage(
                    &order.expression,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
            if let Some(filter) = &call.filter {
                collect_expression_lineage(
                    filter,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
            for order in &call.within_group {
                collect_expression_lineage(
                    &order.expression,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
        }
        HirExpressionKind::Operator { operands, .. } => {
            for operand in operands {
                collect_expression_lineage(
                    operand,
                    expression_node,
                    projection_sources,
                    nodes,
                    edges,
                );
            }
        }
        _ => {}
    }
}

fn build_references(
    hir: &dibs_query_ir::HirQuery,
    typed: &TypedStatement,
) -> Result<ReferenceIndex, DiagnosticSet> {
    let mut references = Vec::new();
    let mut next_id = 1u32;
    collect_statement_references(hir, &hir.statement, typed, &mut next_id, &mut references)?;
    Ok(ReferenceIndex::new(references))
}

fn collect_statement_projection_sources(
    statement: &dibs_query_ir::HirStatement,
    output: &mut BTreeMap<dibs_query_ir::FieldId, HirExpression>,
) {
    for projection in statement_projections(statement) {
        output.insert(projection.field_id, projection.expression.clone());
        collect_expression_projection_sources(&projection.expression, output);
    }
    if let dibs_query_ir::HirStatementKind::Select(select) = &statement.kind {
        for relation in &select.from {
            collect_relation_projection_sources(relation, output);
        }
    }
}

fn collect_relation_projection_sources(
    relation: &HirRelation,
    output: &mut BTreeMap<dibs_query_ir::FieldId, HirExpression>,
) {
    match &relation.kind {
        HirRelationKind::Subquery(statement) => {
            collect_statement_projection_sources(statement, output)
        }
        HirRelationKind::Join { left, right, .. } => {
            collect_relation_projection_sources(left, output);
            collect_relation_projection_sources(right, output);
        }
        _ => {}
    }
}

fn collect_expression_projection_sources(
    expression: &HirExpression,
    output: &mut BTreeMap<dibs_query_ir::FieldId, HirExpression>,
) {
    if let HirExpressionKind::ScalarSubquery(statement) = &expression.kind {
        collect_statement_projection_sources(statement, output);
    }
}

fn collect_statement_references(
    query: &dibs_query_ir::HirQuery,
    statement: &dibs_query_ir::HirStatement,
    typed: &TypedStatement,
    next_id: &mut u32,
    references: &mut Vec<ResolvedReference>,
) -> Result<(), DiagnosticSet> {
    let (dibs_query_ir::HirStatementKind::Select(select), TypedStatementKind::Select(typed_select)) =
        (&statement.kind, &typed.kind)
    else {
        return Ok(());
    };
    if select.from.len() != typed_select.from.len()
        || select.projections.len() != typed_select.projections.len()
        || select.order_by.len() != typed_select.order_by.len()
        || select.predicate.is_some() != typed_select.predicate.is_some()
    {
        return Err(reference_shape_diagnostic(query));
    }
    for (relation, typed_relation) in select.from.iter().zip(&typed_select.from) {
        collect_relation_references(query, relation, typed_relation, next_id, references)?;
    }
    for (projection, typed_projection) in select.projections.iter().zip(&typed_select.projections) {
        collect_expression_references(
            query,
            projection.field_id,
            &projection.expression,
            &typed_projection.expression,
            ReferenceRole::Projection,
            next_id,
            references,
        )?;
    }
    if let (Some(predicate), Some(typed_predicate)) = (&select.predicate, &typed_select.predicate) {
        collect_expression_references(
            query,
            dibs_query_ir::FieldId::new(0),
            predicate,
            typed_predicate,
            ReferenceRole::Predicate,
            next_id,
            references,
        )?;
    }
    for (order, typed_order) in select.order_by.iter().zip(&typed_select.order_by) {
        collect_expression_references(
            query,
            dibs_query_ir::FieldId::new(0),
            &order.expression,
            &typed_order.expression,
            ReferenceRole::Ordering,
            next_id,
            references,
        )?;
    }
    Ok(())
}

fn collect_relation_references(
    query: &dibs_query_ir::HirQuery,
    relation: &HirRelation,
    typed: &dibs_query_ir::TypedRelation,
    next_id: &mut u32,
    references: &mut Vec<ResolvedReference>,
) -> Result<(), DiagnosticSet> {
    if relation.id != typed.id {
        return Err(reference_shape_diagnostic(query));
    }
    match (&relation.kind, &typed.kind) {
        (
            HirRelationKind::Table { table_id },
            dibs_query_ir::TypedRelationKind::Table {
                table_id: typed_table,
            },
        ) if table_id == typed_table => push_reference(
            query,
            next_id,
            references,
            TypedNodeId::Relation(relation.id),
            relation.origin.clone(),
            ReferenceTarget::Table(table_id.clone()),
            ReferenceRole::Projection,
        ),
        (
            HirRelationKind::Subquery(statement),
            dibs_query_ir::TypedRelationKind::Subquery(typed_statement),
        ) => collect_statement_references(query, statement, typed_statement, next_id, references)?,
        (
            HirRelationKind::Join {
                left,
                right,
                predicate,
                ..
            },
            dibs_query_ir::TypedRelationKind::Join {
                left: typed_left,
                right: typed_right,
                predicate: typed_predicate,
                ..
            },
        ) => {
            collect_relation_references(query, left, typed_left, next_id, references)?;
            collect_relation_references(query, right, typed_right, next_id, references)?;
            match (predicate.as_deref(), typed_predicate.as_deref()) {
                (Some(expression), Some(typed_expression)) => collect_expression_references(
                    query,
                    dibs_query_ir::FieldId::new(0),
                    expression,
                    typed_expression,
                    ReferenceRole::JoinKey,
                    next_id,
                    references,
                )?,
                (None, None) => {}
                _ => return Err(reference_shape_diagnostic(query)),
            }
        }
        (
            HirRelationKind::Function { .. }
            | HirRelationKind::Cte { .. }
            | HirRelationKind::Values { .. }
            | HirRelationKind::SetOperation { .. },
            _,
        ) if typed.corresponds_to_hir(relation) => {}
        _ => return Err(reference_shape_diagnostic(query)),
    }
    Ok(())
}

fn collect_expression_references(
    query: &dibs_query_ir::HirQuery,
    field: dibs_query_ir::FieldId,
    expression: &HirExpression,
    typed: &TypedExpression,
    role: ReferenceRole,
    next_id: &mut u32,
    references: &mut Vec<ResolvedReference>,
) -> Result<(), DiagnosticSet> {
    if expression.id != typed.id {
        return Err(reference_shape_diagnostic(query));
    }
    let node = if field.get() == 0 {
        TypedNodeId::Expression(expression.id)
    } else {
        TypedNodeId::Field(field)
    };
    match (&expression.kind, &typed.kind) {
        (
            HirExpressionKind::Parameter(parameter),
            TypedExpressionKind::Parameter(typed_parameter),
        ) if parameter == typed_parameter => push_reference(
            query,
            next_id,
            references,
            node,
            expression.origin.clone(),
            ReferenceTarget::Parameter(*parameter),
            role,
        ),
        (
            HirExpressionKind::Column { binding, column_id },
            TypedExpressionKind::Column {
                binding: typed_binding,
                column_id: typed_column,
            },
        ) if binding == typed_binding && column_id == typed_column => push_reference(
            query,
            next_id,
            references,
            node,
            expression.origin.clone(),
            ReferenceTarget::Column(column_id.clone()),
            role,
        ),
        (
            HirExpressionKind::DerivedColumn { binding, field_id },
            TypedExpressionKind::DerivedColumn {
                binding: typed_binding,
                field_id: typed_field,
            },
        ) if binding == typed_binding && field_id == typed_field => push_reference(
            query,
            next_id,
            references,
            node,
            expression.origin.clone(),
            ReferenceTarget::OutputField(*field_id),
            role,
        ),
        (HirExpressionKind::Call(call), TypedExpressionKind::Call(typed_call))
            if call.callable_id == typed_call.authored_callable_id
                && call.arguments.len() == typed_call.arguments.len()
                && call.order_by.len() == typed_call.order_by.len()
                && call.within_group.len() == typed_call.within_group.len()
                && call.filter.is_some() == typed_call.filter.is_some()
                && call.over.is_some() == typed_call.over.is_some() =>
        {
            push_reference(
                query,
                next_id,
                references,
                TypedNodeId::Expression(expression.id),
                expression.origin.clone(),
                ReferenceTarget::Callable(typed_call.callable_id.clone()),
                ReferenceRole::FunctionUse,
            );
            for (argument, typed_argument) in call.arguments.iter().zip(&typed_call.arguments) {
                collect_expression_references(
                    query,
                    field,
                    argument,
                    &typed_argument.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
            for (order, typed_order) in call.order_by.iter().zip(&typed_call.order_by) {
                collect_expression_references(
                    query,
                    field,
                    &order.expression,
                    &typed_order.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
            if let (Some(filter), Some(typed_filter)) = (&call.filter, &typed_call.filter) {
                collect_expression_references(
                    query,
                    field,
                    filter,
                    typed_filter,
                    ReferenceRole::Predicate,
                    next_id,
                    references,
                )?;
            }
            for (order, typed_order) in call.within_group.iter().zip(&typed_call.within_group) {
                collect_expression_references(
                    query,
                    field,
                    &order.expression,
                    &typed_order.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
            collect_window_reference_references(
                query,
                field,
                call.over.as_ref(),
                typed_call.over.as_ref(),
                role,
                next_id,
                references,
            )?;
        }
        (
            HirExpressionKind::Operator {
                operator_id: authored_operator,
                operands,
            },
            TypedExpressionKind::Operator {
                authored_operator_id,
                operator_id,
                operands: typed_operands,
            },
        ) if authored_operator == authored_operator_id
            && operands.len() == typed_operands.len() =>
        {
            if !operator_id.as_str().starts_with("pg18:operator:syntax:") {
                push_reference(
                    query,
                    next_id,
                    references,
                    TypedNodeId::Expression(expression.id),
                    expression.origin.clone(),
                    ReferenceTarget::Operator(operator_id.clone()),
                    ReferenceRole::OperatorUse,
                );
            }
            for (operand, typed_operand) in operands.iter().zip(typed_operands) {
                collect_expression_references(
                    query,
                    field,
                    operand,
                    &typed_operand.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
        }
        (
            HirExpressionKind::Case {
                operand,
                branches,
                else_expression,
            },
            TypedExpressionKind::Case {
                operand: typed_operand,
                branches: typed_branches,
                else_expression: typed_else,
                ..
            },
        ) if operand.is_some() == typed_operand.is_some()
            && branches.len() == typed_branches.len()
            && else_expression.is_some() == typed_else.is_some() =>
        {
            if let (Some(operand), Some(typed_operand)) = (operand, typed_operand) {
                collect_expression_references(
                    query,
                    field,
                    operand,
                    typed_operand,
                    role,
                    next_id,
                    references,
                )?;
            }
            for (branch, typed_branch) in branches.iter().zip(typed_branches) {
                collect_expression_references(
                    query,
                    field,
                    &branch.when,
                    &typed_branch.when,
                    ReferenceRole::Predicate,
                    next_id,
                    references,
                )?;
                collect_expression_references(
                    query,
                    field,
                    &branch.then,
                    &typed_branch.then.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
            if let (Some(value), Some(typed_value)) = (else_expression, typed_else) {
                collect_expression_references(
                    query,
                    field,
                    value,
                    &typed_value.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
        }
        (HirExpressionKind::Row(values), TypedExpressionKind::Row(typed_values))
            if values.len() == typed_values.len() =>
        {
            for (value, typed_value) in values.iter().zip(typed_values) {
                collect_expression_references(
                    query,
                    field,
                    value,
                    typed_value,
                    role,
                    next_id,
                    references,
                )?;
            }
        }
        (HirExpressionKind::Array(values), TypedExpressionKind::Array { elements, .. })
            if values.len() == elements.len() =>
        {
            for (value, typed_value) in values.iter().zip(elements) {
                collect_expression_references(
                    query,
                    field,
                    value,
                    &typed_value.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
        }
        (
            HirExpressionKind::ScalarSubquery(statement),
            TypedExpressionKind::ScalarSubquery(typed_statement),
        ) => {
            collect_statement_references(query, statement, typed_statement, next_id, references)?;
        }
        (HirExpressionKind::Literal(hir), TypedExpressionKind::Literal(typed)) if hir == typed => {}
        _ => return Err(reference_shape_diagnostic(query)),
    }
    Ok(())
}

fn collect_window_reference_references(
    query: &dibs_query_ir::HirQuery,
    field: dibs_query_ir::FieldId,
    hir: Option<&dibs_query_ir::WindowReference<HirExpression>>,
    typed: Option<&dibs_query_ir::WindowReference<TypedExpression>>,
    role: ReferenceRole,
    next_id: &mut u32,
    references: &mut Vec<ResolvedReference>,
) -> Result<(), DiagnosticSet> {
    match (hir, typed) {
        (None, None) => Ok(()),
        (
            Some(dibs_query_ir::WindowReference::Named(hir)),
            Some(dibs_query_ir::WindowReference::Named(typed)),
        ) if hir == typed => Ok(()),
        (
            Some(dibs_query_ir::WindowReference::Inline(hir)),
            Some(dibs_query_ir::WindowReference::Inline(typed)),
        ) if hir.partition_by.len() == typed.partition_by.len()
            && hir.order_by.len() == typed.order_by.len()
            && hir.frame.is_some() == typed.frame.is_some() =>
        {
            for (expression, typed_expression) in hir.partition_by.iter().zip(&typed.partition_by) {
                collect_expression_references(
                    query,
                    field,
                    expression,
                    typed_expression,
                    role,
                    next_id,
                    references,
                )?;
            }
            for (order, typed_order) in hir.order_by.iter().zip(&typed.order_by) {
                collect_expression_references(
                    query,
                    field,
                    &order.expression,
                    &typed_order.expression,
                    role,
                    next_id,
                    references,
                )?;
            }
            if let (Some(frame), Some(typed_frame)) = (&hir.frame, &typed.frame) {
                collect_frame_bound_references(
                    query,
                    field,
                    &frame.start,
                    &typed_frame.start,
                    role,
                    next_id,
                    references,
                )?;
                match (&frame.end, &typed_frame.end) {
                    (Some(end), Some(typed_end)) => collect_frame_bound_references(
                        query, field, end, typed_end, role, next_id, references,
                    )?,
                    (None, None) => {}
                    _ => return Err(reference_shape_diagnostic(query)),
                }
            }
            Ok(())
        }
        _ => Err(reference_shape_diagnostic(query)),
    }
}

fn collect_frame_bound_references(
    query: &dibs_query_ir::HirQuery,
    field: dibs_query_ir::FieldId,
    hir: &dibs_query_ir::FrameBound<HirExpression>,
    typed: &dibs_query_ir::FrameBound<TypedExpression>,
    role: ReferenceRole,
    next_id: &mut u32,
    references: &mut Vec<ResolvedReference>,
) -> Result<(), DiagnosticSet> {
    match (hir, typed) {
        (
            dibs_query_ir::FrameBound::Preceding(expression),
            dibs_query_ir::FrameBound::Preceding(typed_expression),
        )
        | (
            dibs_query_ir::FrameBound::Following(expression),
            dibs_query_ir::FrameBound::Following(typed_expression),
        ) => collect_expression_references(
            query,
            field,
            expression,
            typed_expression,
            role,
            next_id,
            references,
        ),
        (
            dibs_query_ir::FrameBound::UnboundedPreceding,
            dibs_query_ir::FrameBound::UnboundedPreceding,
        )
        | (dibs_query_ir::FrameBound::CurrentRow, dibs_query_ir::FrameBound::CurrentRow)
        | (
            dibs_query_ir::FrameBound::UnboundedFollowing,
            dibs_query_ir::FrameBound::UnboundedFollowing,
        ) => Ok(()),
        _ => Err(reference_shape_diagnostic(query)),
    }
}

fn reference_shape_diagnostic(query: &dibs_query_ir::HirQuery) -> DiagnosticSet {
    vec![CompileDiagnostic::new(
        CompileDiagnosticCode::InvalidArtifact,
        query.origin.span(),
        "semantic checker reference topology does not correspond to resolved HIR",
    )]
}

fn push_reference(
    query: &dibs_query_ir::HirQuery,
    next_id: &mut u32,
    references: &mut Vec<ResolvedReference>,
    enclosing_node: TypedNodeId,
    origin: dibs_query_ir::SourceOrigin,
    target: ReferenceTarget,
    role: ReferenceRole,
) {
    references.push(ResolvedReference {
        id: ReferenceId::new(*next_id),
        query_id: query.id,
        enclosing_node,
        origin,
        target,
        role,
        access: ReferenceAccess::Read,
        lineage_node: None,
        generated_members: Vec::<GeneratedContractMember>::new(),
    });
    *next_id += 1;
}

fn result_mode(mode: SyntaxResultMode) -> ResultMode {
    match mode {
        SyntaxResultMode::Many => ResultMode::Many,
        SyntaxResultMode::Optional => ResultMode::Optional,
        SyntaxResultMode::One => ResultMode::One,
        SyntaxResultMode::Exec => ResultMode::Exec,
    }
}

fn rust_parameter_policy(api_type: &str) -> (ParameterPassing, ParameterBindAdapter) {
    match api_type {
        "String" => (ParameterPassing::StringSlice, ParameterBindAdapter::Direct),
        "Vec<u8>" => (ParameterPassing::ByteSlice, ParameterBindAdapter::Direct),
        _ => (
            ParameterPassing::SharedReference,
            ParameterBindAdapter::Direct,
        ),
    }
}

fn statement_projections(statement: &dibs_query_ir::HirStatement) -> &[HirProjection] {
    match &statement.kind {
        dibs_query_ir::HirStatementKind::Select(select) => &select.projections,
        dibs_query_ir::HirStatementKind::Insert(insert) => &insert.returning,
        dibs_query_ir::HirStatementKind::Update(update) => &update.returning,
        dibs_query_ir::HirStatementKind::Delete(delete) => &delete.returning,
    }
}

fn collect_read_tables(statement: &dibs_query_ir::HirStatement) -> Vec<dibs_pg_catalog::TableId> {
    let mut tables = Vec::new();
    if let dibs_query_ir::HirStatementKind::Select(select) = &statement.kind {
        for relation in &select.from {
            collect_relation_tables(relation, &mut tables);
        }
    }
    tables.sort();
    tables.dedup();
    tables
}

fn collect_relation_tables(relation: &HirRelation, output: &mut Vec<dibs_pg_catalog::TableId>) {
    match &relation.kind {
        HirRelationKind::Table { table_id } => output.push(table_id.clone()),
        HirRelationKind::Join { left, right, .. } => {
            collect_relation_tables(left, output);
            collect_relation_tables(right, output);
        }
        _ => {}
    }
}

fn maximum_volatility(statement: &dibs_query_ir::TypedStatement) -> Volatility {
    let mut volatility = Volatility::Immutable;
    if let dibs_query_ir::TypedStatementKind::Select(select) = &statement.kind {
        for projection in &select.projections {
            volatility = volatility.max(projection.expression.volatility);
        }
        if let Some(predicate) = &select.predicate {
            volatility = volatility.max(predicate.volatility);
        }
        for order in &select.order_by {
            volatility = volatility.max(order.expression.volatility);
        }
    }
    volatility
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 && !output.ends_with('_') {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    output.trim_matches('_').to_string()
}

fn checked_target_name(
    value: &str,
    span: dibs_query_ir::SourceSpan,
) -> Result<String, DiagnosticSet> {
    let name = to_snake_case(value);
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || name.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        return Err(vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidApiContract,
            span,
            format!("output label '{value}' cannot become a valid Rust field name"),
        )]);
    }
    Ok(name)
}

fn to_pascal_case(value: &str) -> String {
    let mut output = String::new();
    let mut upper = true;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            upper = true;
        } else if upper {
            output.push(character.to_ascii_uppercase());
            upper = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn check_diagnostic(
    error: dibs_query_typing::CheckError,
    fallback_span: dibs_query_ir::SourceSpan,
) -> DiagnosticSet {
    use dibs_query_typing::CheckError;
    let (code, span) = match &error {
        CheckError::UnknownParameter { origin, .. } => {
            (CompileDiagnosticCode::UnknownParameter, origin.span())
        }
        CheckError::UnknownColumn { origin, .. } | CheckError::UnknownCteField { origin, .. } => {
            (CompileDiagnosticCode::UnknownField, origin.span())
        }
        CheckError::InvalidLimit { origin, .. } => {
            (CompileDiagnosticCode::InvalidLimit, origin.span())
        }
        CheckError::NonBooleanPredicate { origin, .. }
        | CheckError::UnboundedScalarSubquery { origin, .. }
        | CheckError::AggregateInPreGroupClause { origin, .. }
        | CheckError::UngroupedAggregateProjection { origin }
        | CheckError::DistinctOnOrderMismatch { origin }
        | CheckError::NumericLiteralOutOfRange { origin, .. } => {
            (CompileDiagnosticCode::TypeMismatch, origin.span())
        }
        CheckError::UnsupportedStatement { origin, .. }
        | CheckError::UnsupportedRecursiveCte { origin } => {
            (CompileDiagnosticCode::UnsupportedClause, origin.span())
        }
        CheckError::Type(dibs_query_typing::TypeResolutionError::IncompatibleCallable {
            ..
        }) => (CompileDiagnosticCode::UnknownCallable, fallback_span),
        CheckError::Type(dibs_query_typing::TypeResolutionError::AmbiguousCallable { .. }) => {
            (CompileDiagnosticCode::AmbiguousCallable, fallback_span)
        }
        CheckError::Type(_)
        | CheckError::SetColumnCountMismatch { .. }
        | CheckError::InvalidTypedShape(_) => (CompileDiagnosticCode::TypeMismatch, fallback_span),
    };
    vec![CompileDiagnostic::new(code, span, error.to_string())]
}
