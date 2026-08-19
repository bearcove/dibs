use std::collections::BTreeSet;

use dibs_pg_catalog::SchemaFingerprint;

use crate::{
    ArtifactHashes, Cardinality, CatalogRenderNames, CompilerVersions, ContentHash,
    ExecutionIdentity, ExecutionIdentityInput, ExecutionParameter, HirQuery, LineageGraph,
    ManifestIdentity, OrderedBind, OutputField, Parameter, ParameterId, PublicContractIdentity,
    PublicIdentityInput, QueryId, QueryManifest, ReadWriteLockManifest, ReferenceIndex, ResultMode,
    RuntimeAssertion, SourceMap, SourceOrigin, TypedStatement, TypedStatementKind,
    execution_identity, public_contract_identity,
};

/// Complete immutable checked query artifact consumed by all backends and runtimes.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = CompiledQuery::is_valid)]
pub struct CompiledQuery {
    /// Artifact, compiler, query-language, PostgreSQL, and identity format versions.
    pub compiler_versions: CompilerVersions,
    /// Stable fingerprint of the catalog/schema snapshot used to compile.
    pub catalog_schema_fingerprint: SchemaFingerprint,
    /// Revision-local query identity.
    pub query_id: QueryId,
    /// Stable execution-semantics identity.
    pub execution_semantics_id: ExecutionIdentity,
    /// Stable public output/request contract identity.
    pub public_contract_id: PublicContractIdentity,
    /// Stable canonical manifest identity.
    pub manifest_identity: ManifestIdentity,
    /// Public declaration name.
    pub query_name: String,
    /// Exact source origin for diagnostics and navigation.
    pub query_origin: SourceOrigin,
    /// Declared runtime row/result contract.
    pub declared_result_mode: ResultMode,
    /// Inferred proof-bearing final cardinality.
    pub inferred_cardinality: Cardinality,
    /// Runtime checks required where static proof is incomplete.
    pub runtime_assertions: Vec<RuntimeAssertion>,
    /// Complete deterministic PostgreSQL SQL ready for static execution.
    pub deterministic_sql: String,
    /// Ordered one-based PostgreSQL bind positions.
    pub ordered_bind_map: Vec<OrderedBind>,
    /// Ordered public parameter contract.
    pub ordered_parameters: Vec<Parameter>,
    /// Ordered flat PostgreSQL output-row contract.
    pub ordered_output_fields: Vec<OutputField>,
    /// Canonical SQL names for every catalog identity needed by typed SQL rendering.
    pub catalog_render_names: CatalogRenderNames,
    /// Complete resolved HIR retained for tools and semantic review.
    pub resolved_hir: HirQuery,
    /// Complete typed PostgreSQL IR used by SQL and API backends.
    pub typed_statement: TypedStatement,
    /// Compiler-owned role-typed reference index.
    pub resolved_references: ReferenceIndex,
    /// Output lineage graph reaching stable catalog columns and generated members.
    pub lineage: LineageGraph,
    /// Read/write/lock/volatility/mutation summary.
    pub read_write_lock_manifest: ReadWriteLockManifest,
    /// Exact bidirectional source-to-typed-to-rendered SQL map.
    pub source_map: SourceMap,
    /// Machine-readable review and observability manifest.
    pub manifest: QueryManifest,
    /// Content hashes for every immutable artifact surface.
    pub artifact_hashes: ArtifactHashes,
}

impl CompiledQuery {
    /// Validates all duplicated contracts and returns the same artifact on success.
    pub fn validate(&self) -> Result<&Self, CompiledQueryError> {
        if self.compiler_versions.supported_postgres_major != 18 {
            return Err(CompiledQueryError::UnsupportedPostgresMajor {
                actual: self.compiler_versions.supported_postgres_major,
            });
        }
        self.typed_statement
            .validate()
            .map_err(|_| CompiledQueryError::InvalidTypedStatement)?;
        if self.inferred_cardinality != self.typed_statement.cardinality {
            return Err(CompiledQueryError::CardinalityMismatch);
        }
        validate_ordinals(&self.ordered_parameters, &self.ordered_output_fields)?;
        validate_hir_parameters(&self.resolved_hir, &self.ordered_parameters)?;
        validate_binds(&self.ordered_bind_map, &self.ordered_parameters)?;
        validate_outputs(&self.typed_statement, &self.ordered_output_fields)?;
        validate_catalog_render_names(self)?;
        if !self
            .typed_statement
            .corresponds_to_hir(&self.resolved_hir.statement)
        {
            return Err(CompiledQueryError::HirTypedMismatch);
        }
        validate_result_mode(
            self.declared_result_mode,
            &self.inferred_cardinality,
            &self.runtime_assertions,
            &self.ordered_output_fields,
        )?;
        validate_public_api_names(
            self.declared_result_mode,
            &self.manifest.operation_names,
            &self.manifest.result_type_names,
        )?;
        if self.query_id != self.resolved_hir.id || self.query_name != self.resolved_hir.name {
            return Err(CompiledQueryError::QueryIdentityMismatch);
        }
        if execution_identity(&self.execution_identity_input()) != self.execution_semantics_id {
            return Err(CompiledQueryError::ExecutionIdentityMismatch);
        }
        if public_contract_identity(&self.public_identity_input()) != self.public_contract_id {
            return Err(CompiledQueryError::PublicIdentityMismatch);
        }
        if ManifestIdentity::from_manifest(&self.manifest)
            .map_err(|_| CompiledQueryError::Serialization)?
            != self.manifest_identity
        {
            return Err(CompiledQueryError::ManifestIdentityMismatch);
        }
        validate_manifest(self)?;
        if self.artifact_hashes.normalized_sql
            != ContentHash::of_bytes(self.deterministic_sql.as_bytes())
        {
            return Err(CompiledQueryError::SqlHashMismatch);
        }
        if self.artifact_hashes.source_map
            != ContentHash::of_json(&self.source_map)
                .map_err(|_| CompiledQueryError::Serialization)?
        {
            return Err(CompiledQueryError::SourceMapHashMismatch);
        }
        if self.artifact_hashes.manifest
            != ContentHash::of_json(&self.manifest)
                .map_err(|_| CompiledQueryError::Serialization)?
        {
            return Err(CompiledQueryError::ManifestHashMismatch);
        }
        Ok(self)
    }

    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }

    /// Reconstructs the execution identity input from the immutable artifact.
    #[must_use]
    pub fn execution_identity_input(&self) -> ExecutionIdentityInput {
        ExecutionIdentityInput {
            version: self.compiler_versions.execution_identity_format_version,
            postgres_major: self.compiler_versions.supported_postgres_major,
            statement: self.typed_statement.clone(),
            parameters: self
                .ordered_parameters
                .iter()
                .map(|parameter| ExecutionParameter {
                    id: parameter.id,
                    type_id: parameter.type_id.clone(),
                    typmod: parameter.typmod.clone(),
                    nullable: parameter.nullable,
                })
                .collect(),
            result_mode: self.declared_result_mode,
            runtime_assertions: self.runtime_assertions.clone(),
            references: self.resolved_references.clone(),
            read_write_lock_manifest: self.read_write_lock_manifest.clone(),
            catalog_schema_fingerprint: self.catalog_schema_fingerprint.clone(),
        }
    }

    /// Reconstructs the public-contract identity input from the immutable artifact.
    #[must_use]
    pub fn public_identity_input(&self) -> PublicIdentityInput {
        PublicIdentityInput {
            version: self.compiler_versions.public_identity_format_version,
            query_name: self.query_name.clone(),
            parameters: self.ordered_parameters.clone(),
            output_fields: self.ordered_output_fields.clone(),
            result_mode: self.declared_result_mode,
            transport_envelope: None,
            operation_names: self.manifest.operation_names.clone(),
            result_type_names: self.manifest.result_type_names.clone(),
        }
    }
}

/// Inconsistent or invalid compiled-query artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledQueryError {
    /// Artifact targets a PostgreSQL major other than 18.
    UnsupportedPostgresMajor {
        /// Actual stored major.
        actual: u16,
    },
    /// Typed statement contains an invalid proof or topology shape.
    InvalidTypedStatement,
    /// Stored final cardinality differs from typed statement cardinality.
    CardinalityMismatch,
    /// Parameter ordinals are not zero-based and contiguous.
    ParameterOrdinal {
        /// Expected ordinal.
        expected: u32,
        /// Actual ordinal.
        actual: u32,
    },
    /// Output ordinals are not zero-based and contiguous.
    OutputOrdinal {
        /// Expected ordinal.
        expected: u32,
        /// Actual ordinal.
        actual: u32,
    },
    /// HIR and execution parameter contracts differ.
    ParameterContractMismatch,
    /// Bind positions are not one-based and contiguous.
    NonContiguousBindPosition {
        /// Expected position.
        expected: u32,
        /// Actual position.
        actual: u32,
    },
    /// Bind map references an undeclared parameter.
    UnknownBindParameter(ParameterId),
    /// Typed statement output fields differ from the ordered output contract.
    OutputContractMismatch,
    /// Catalog rendering vocabulary is missing an identity used by typed IR.
    MissingCatalogRenderName,
    /// Resolved HIR and typed IR do not describe the same statement topology.
    HirTypedMismatch,
    /// Result mode, proof, runtime assertions, and output shape disagree.
    ResultModeMismatch,
    /// Target-language operation/result type names disagree with the result shape.
    PublicApiNameMismatch,
    /// Query ID/name differs from resolved HIR.
    QueryIdentityMismatch,
    /// Stored execution identity is stale.
    ExecutionIdentityMismatch,
    /// Stored public identity is stale.
    PublicIdentityMismatch,
    /// Stored manifest identity is stale.
    ManifestIdentityMismatch,
    /// Manifest duplicates differ from authoritative artifact surfaces.
    ManifestMismatch,
    /// SQL hash is stale.
    SqlHashMismatch,
    /// Source-map hash is stale.
    SourceMapHashMismatch,
    /// Manifest hash is stale.
    ManifestHashMismatch,
    /// Facet JSON serialization failed.
    Serialization,
}

impl std::fmt::Display for CompiledQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid compiled query: {self:?}")
    }
}

impl std::error::Error for CompiledQueryError {}

fn validate_ordinals(
    parameters: &[Parameter],
    outputs: &[OutputField],
) -> Result<(), CompiledQueryError> {
    for (expected, parameter) in parameters.iter().enumerate() {
        let expected = expected as u32;
        if parameter.ordinal != expected {
            return Err(CompiledQueryError::ParameterOrdinal {
                expected,
                actual: parameter.ordinal,
            });
        }
    }
    for (expected, output) in outputs.iter().enumerate() {
        let expected = expected as u32;
        if output.ordinal != expected {
            return Err(CompiledQueryError::OutputOrdinal {
                expected,
                actual: output.ordinal,
            });
        }
    }
    Ok(())
}

fn validate_hir_parameters(
    hir: &HirQuery,
    parameters: &[Parameter],
) -> Result<(), CompiledQueryError> {
    let matches = hir.parameters.len() == parameters.len()
        && hir
            .parameters
            .iter()
            .zip(parameters)
            .all(|(hir, contract)| {
                hir.id == contract.id
                    && hir.ordinal == contract.ordinal
                    && hir.name == contract.source_name
                    && hir.type_id == contract.type_id
                    && hir.typmod == contract.typmod
                    && hir.nullable == contract.nullable
            });
    matches
        .then_some(())
        .ok_or(CompiledQueryError::ParameterContractMismatch)
}

fn validate_binds(
    binds: &[OrderedBind],
    parameters: &[Parameter],
) -> Result<(), CompiledQueryError> {
    let declared: BTreeSet<_> = parameters.iter().map(|parameter| parameter.id).collect();
    for (index, bind) in binds.iter().enumerate() {
        let expected = index as u32 + 1;
        if bind.position != expected {
            return Err(CompiledQueryError::NonContiguousBindPosition {
                expected,
                actual: bind.position,
            });
        }
        if !declared.contains(&bind.parameter_id) {
            return Err(CompiledQueryError::UnknownBindParameter(bind.parameter_id));
        }
    }
    Ok(())
}

fn validate_outputs(
    statement: &TypedStatement,
    outputs: &[OutputField],
) -> Result<(), CompiledQueryError> {
    let projections = match &statement.kind {
        TypedStatementKind::Select(select) => &select.projections,
        TypedStatementKind::Insert(insert) => &insert.returning,
        TypedStatementKind::Update(update) => &update.returning,
        TypedStatementKind::Delete(delete) => &delete.returning,
    };
    let matches = projections.len() == outputs.len()
        && projections.iter().zip(outputs).all(|(projection, output)| {
            projection.field_id == output.id
                && projection.expression.id == output.source_expression
                && projection.expression.type_id == output.type_id
                && projection.expression.typmod == output.typmod
                && projection.expression.nullability == output.nullability
        });
    matches
        .then_some(())
        .ok_or(CompiledQueryError::OutputContractMismatch)
}

fn validate_result_mode(
    mode: ResultMode,
    cardinality: &Cardinality,
    assertions: &[RuntimeAssertion],
    outputs: &[OutputField],
) -> Result<(), CompiledQueryError> {
    let mut minimum = match cardinality.lower() {
        crate::LowerBound::Zero => 0,
        crate::LowerBound::One => 1,
    };
    let mut maximum = match cardinality.upper() {
        crate::UpperBound::Zero => Some(0),
        crate::UpperBound::One => Some(1),
        crate::UpperBound::Finite(value) => Some(value),
        crate::UpperBound::Unbounded | crate::UpperBound::Unknown => None,
    };
    let mut rowless_asserted = false;
    for assertion in assertions {
        match assertion {
            RuntimeAssertion::AtMostRows { maximum: asserted } => {
                maximum = Some(maximum.map_or(*asserted, |current| current.min(*asserted)));
            }
            RuntimeAssertion::AtLeastRows { minimum: asserted } => {
                minimum = minimum.max(*asserted);
            }
            RuntimeAssertion::Rowless => {
                rowless_asserted = true;
                maximum = Some(0);
            }
            RuntimeAssertion::ValidLimitParameter { .. } => {}
        }
    }
    let range_is_possible = maximum.is_none_or(|maximum| minimum <= maximum);
    let includes_one = minimum <= 1 && maximum.is_none_or(|maximum| maximum >= 1);
    let valid = range_is_possible
        && match mode {
            ResultMode::Many => !outputs.is_empty(),
            ResultMode::Optional => !outputs.is_empty() && maximum.is_some_and(|value| value <= 1),
            ResultMode::One => {
                !outputs.is_empty()
                    && includes_one
                    && minimum >= 1
                    && maximum.is_some_and(|value| value <= 1)
            }
            ResultMode::Exec => outputs.is_empty() && rowless_asserted && maximum == Some(0),
        };
    valid
        .then_some(())
        .ok_or(CompiledQueryError::ResultModeMismatch)
}

fn validate_public_api_names(
    mode: ResultMode,
    operations: &[crate::ApiOperationName],
    result_types: &[crate::ApiResultTypeName],
) -> Result<(), CompiledQueryError> {
    let operation_languages: BTreeSet<_> = operations.iter().map(|name| name.language).collect();
    let result_languages: BTreeSet<_> = result_types.iter().map(|name| name.language).collect();
    let operations_are_unique = operation_languages.len() == operations.len();
    let results_are_unique = result_languages.len() == result_types.len();
    let valid = operations_are_unique
        && results_are_unique
        && match mode {
            ResultMode::Exec => result_types.is_empty(),
            ResultMode::Many | ResultMode::Optional | ResultMode::One => {
                operation_languages == result_languages
            }
        };
    valid
        .then_some(())
        .ok_or(CompiledQueryError::PublicApiNameMismatch)
}
fn validate_manifest(query: &CompiledQuery) -> Result<(), CompiledQueryError> {
    let manifest = &query.manifest;
    let canonical_manifest = manifest.canonicalized();
    let mut generated_outputs = query.artifact_hashes.generated_outputs.clone();
    generated_outputs.sort_by(|left, right| {
        (&left.language, &left.generator_version, &left.hash).cmp(&(
            &right.language,
            &right.generator_version,
            &right.hash,
        ))
    });
    generated_outputs.dedup();
    let matches = manifest.manifest_format_version
        == query.compiler_versions.manifest_format_version
        && manifest.query_id == query.query_id
        && manifest.execution_semantics_id == query.execution_semantics_id
        && manifest.public_contract_id == query.public_contract_id
        && manifest.compiler_versions == query.compiler_versions
        && manifest.catalog_schema_fingerprint == query.catalog_schema_fingerprint
        && manifest.normalized_sql_hash == query.artifact_hashes.normalized_sql
        && manifest.source_hash == query.artifact_hashes.source
        && manifest.source_map_hash == query.artifact_hashes.source_map
        && canonical_manifest.generated_output_hashes == generated_outputs
        && manifest.parameters == query.ordered_parameters
        && manifest.operation_names == canonical_manifest.operation_names
        && manifest.result_type_names == canonical_manifest.result_type_names
        && manifest.output_fields == query.ordered_output_fields
        && manifest.inferred_cardinality == query.inferred_cardinality
        && manifest.runtime_assertions == query.runtime_assertions
        && manifest.read_write_lock_manifest == query.read_write_lock_manifest
        && manifest.lineage == query.lineage;
    matches
        .then_some(())
        .ok_or(CompiledQueryError::ManifestMismatch)
}

fn validate_catalog_render_names(query: &CompiledQuery) -> Result<(), CompiledQueryError> {
    let mut required = BTreeSet::new();
    collect_statement_catalog_identities(&query.typed_statement, &mut required);
    for parameter in &query.ordered_parameters {
        required.insert(parameter.type_id.as_str().to_string());
    }
    for output in &query.ordered_output_fields {
        required.insert(output.type_id.as_str().to_string());
        collect_nullability_catalog_identities(&output.nullability, &mut required);
    }
    for reference in &query.resolved_references.references {
        collect_reference_target_catalog_identity(&reference.target, &mut required);
    }
    required
        .iter()
        .all(|id| {
            query
                .catalog_render_names
                .entries()
                .iter()
                .any(|entry| render_name_has_id(entry, id))
        })
        .then_some(())
        .ok_or(CompiledQueryError::MissingCatalogRenderName)
}

fn collect_reference_target_catalog_identity(
    target: &crate::ReferenceTarget,
    output: &mut BTreeSet<String>,
) {
    let id = match target {
        crate::ReferenceTarget::Table(id) => Some(id.as_str()),
        crate::ReferenceTarget::Column(id) => Some(id.as_str()),
        crate::ReferenceTarget::Constraint(id) => Some(id.as_str()),
        crate::ReferenceTarget::Index(id) => Some(id.as_str()),
        crate::ReferenceTarget::Type(id) => Some(id.as_str()),
        crate::ReferenceTarget::Callable(id) => Some(id.as_str()),
        crate::ReferenceTarget::Operator(id) => Some(id.as_str()),
        crate::ReferenceTarget::Collation(id) => Some(id.as_str()),
        crate::ReferenceTarget::Parameter(_)
        | crate::ReferenceTarget::Cast(_)
        | crate::ReferenceTarget::RelationBinding(_)
        | crate::ReferenceTarget::Cte(_)
        | crate::ReferenceTarget::OutputField(_) => None,
    };
    if let Some(id) = id {
        output.insert(id.to_string());
    }
}

fn render_name_has_id(entry: &crate::CatalogRenderName, required: &str) -> bool {
    match entry {
        crate::CatalogRenderName::Table { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Column { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Callable { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Operator { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Type { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Collation { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Constraint { id, .. } => id.as_str() == required,
        crate::CatalogRenderName::Index { id, .. } => id.as_str() == required,
    }
}

fn collect_statement_catalog_identities(statement: &TypedStatement, output: &mut BTreeSet<String>) {
    collect_cardinality_catalog_identities(&statement.cardinality, output);
    match &statement.kind {
        TypedStatementKind::Select(select) => {
            for cte in &select.ctes {
                collect_statement_catalog_identities(&cte.statement, output);
            }
            collect_distinct_catalog_identities(&select.distinct, output);
            collect_projections_catalog_identities(&select.projections, output);
            for relation in &select.from {
                collect_relation_catalog_identities(relation, output);
            }
            collect_expression_option_catalog_identities(select.predicate.as_ref(), output);
            collect_expressions_catalog_identities(&select.group_by, output);
            collect_expression_option_catalog_identities(select.having.as_ref(), output);
            for window in &select.windows {
                collect_window_spec_catalog_identities(&window.specification, output);
            }
            collect_ordering_catalog_identities(&select.order_by, output);
        }
        TypedStatementKind::Insert(insert) => {
            output.insert(insert.target.as_str().to_string());
            output.extend(insert.columns.iter().map(|id| id.as_str().to_string()));
            for cte in &insert.ctes {
                collect_statement_catalog_identities(&cte.statement, output);
            }
            match &insert.source {
                crate::TypedInsertSource::Values(values) => {
                    for row in values.rows() {
                        collect_expressions_catalog_identities(row, output);
                    }
                }
                crate::TypedInsertSource::Select(statement) => {
                    collect_statement_catalog_identities(statement, output);
                }
                crate::TypedInsertSource::DefaultValues => {}
            }
            if let Some(conflict) = &insert.conflict {
                collect_conflict_catalog_identities(conflict, output);
            }
            collect_projections_catalog_identities(&insert.returning, output);
        }
        TypedStatementKind::Update(update) => {
            output.insert(update.target.as_str().to_string());
            for cte in &update.ctes {
                collect_statement_catalog_identities(&cte.statement, output);
            }
            collect_assignments_catalog_identities(&update.assignments, output);
            for relation in &update.from {
                collect_relation_catalog_identities(relation, output);
            }
            collect_expression_option_catalog_identities(update.predicate.as_ref(), output);
            collect_projections_catalog_identities(&update.returning, output);
        }
        TypedStatementKind::Delete(delete) => {
            output.insert(delete.target.as_str().to_string());
            for cte in &delete.ctes {
                collect_statement_catalog_identities(&cte.statement, output);
            }
            for relation in &delete.using_relations {
                collect_relation_catalog_identities(relation, output);
            }
            collect_expression_option_catalog_identities(delete.predicate.as_ref(), output);
            collect_projections_catalog_identities(&delete.returning, output);
        }
    }
}

fn collect_relation_catalog_identities(
    relation: &crate::TypedRelation,
    output: &mut BTreeSet<String>,
) {
    collect_cardinality_catalog_identities(&relation.cardinality, output);
    match &relation.kind {
        crate::TypedRelationKind::Table { table_id } => {
            output.insert(table_id.as_str().to_string());
        }
        crate::TypedRelationKind::Cte { .. } => {}
        crate::TypedRelationKind::Subquery(statement) => {
            collect_statement_catalog_identities(statement, output);
        }
        crate::TypedRelationKind::Function {
            callable_id,
            arguments,
        } => {
            output.insert(callable_id.as_str().to_string());
            collect_expressions_catalog_identities(arguments, output);
        }
        crate::TypedRelationKind::Join {
            left,
            right,
            predicate,
            ..
        } => {
            collect_relation_catalog_identities(left, output);
            collect_relation_catalog_identities(right, output);
            collect_expression_option_catalog_identities(predicate.as_deref(), output);
        }
        crate::TypedRelationKind::Values { rows } => {
            for row in rows.rows() {
                collect_expressions_catalog_identities(row, output);
            }
        }
        crate::TypedRelationKind::SetOperation { left, right, .. } => {
            collect_statement_catalog_identities(left, output);
            collect_statement_catalog_identities(right, output);
        }
    }
}

fn collect_expression_catalog_identities(
    expression: &crate::TypedExpression,
    output: &mut BTreeSet<String>,
) {
    output.insert(expression.type_id.as_str().to_string());
    collect_nullability_catalog_identities(&expression.nullability, output);
    match &expression.kind {
        crate::TypedExpressionKind::Literal(_) | crate::TypedExpressionKind::Parameter(_) => {}
        crate::TypedExpressionKind::Column { column_id, .. } => {
            output.insert(column_id.as_str().to_string());
        }
        crate::TypedExpressionKind::Call(call) => {
            output.insert(call.callable_id.as_str().to_string());
            for argument in &call.arguments {
                collect_expression_catalog_identities(&argument.expression, output);
                if let Some(coercion) = &argument.coercion {
                    collect_coercion_catalog_identities(coercion, output);
                }
            }
            collect_ordering_catalog_identities(&call.order_by, output);
            collect_expression_option_catalog_identities(call.filter.as_deref(), output);
            collect_ordering_catalog_identities(&call.within_group, output);
            if let Some(window) = &call.over {
                collect_window_reference_catalog_identities(window, output);
            }
        }
        crate::TypedExpressionKind::Operator {
            operator_id,
            operands,
            ..
        } => {
            if !is_structural_syntax_operator(operator_id) {
                output.insert(operator_id.as_str().to_string());
            }
            for operand in operands {
                collect_expression_catalog_identities(&operand.expression, output);
                if let Some(coercion) = &operand.coercion {
                    collect_coercion_catalog_identities(coercion, output);
                }
            }
        }
        crate::TypedExpressionKind::Cast {
            expression,
            coercion,
            ..
        } => {
            collect_expression_catalog_identities(expression, output);
            collect_coercion_catalog_identities(coercion, output);
        }
        crate::TypedExpressionKind::Collate {
            collation_id,
            expression,
        } => {
            output.insert(collation_id.as_str().to_string());
            collect_expression_catalog_identities(expression, output);
        }
        crate::TypedExpressionKind::Case {
            operand,
            branches,
            else_expression,
            result_coercion,
        } => {
            collect_expression_option_catalog_identities(operand.as_deref(), output);
            for branch in branches {
                collect_expression_catalog_identities(&branch.when, output);
                collect_expression_catalog_identities(&branch.then, output);
            }
            collect_expression_option_catalog_identities(else_expression.as_deref(), output);
            collect_coercion_evidence_catalog_identities(result_coercion, output);
        }
        crate::TypedExpressionKind::ScalarSubquery(statement) => {
            collect_statement_catalog_identities(statement, output);
        }
        crate::TypedExpressionKind::Row(values) => {
            collect_expressions_catalog_identities(values, output);
        }
        crate::TypedExpressionKind::Array { elements, coercion } => {
            collect_expressions_catalog_identities(elements, output);
            collect_coercion_evidence_catalog_identities(coercion, output);
        }
        crate::TypedExpressionKind::CteColumn { .. } => {}
    }
}

fn is_structural_syntax_operator(operator_id: &dibs_pg_catalog::OperatorId) -> bool {
    operator_id.as_str().starts_with("pg18:operator:syntax:")
}

fn collect_coercion_catalog_identities(
    coercion: &crate::TypedCoercion,
    output: &mut BTreeSet<String>,
) {
    output.insert(coercion.source_type.as_str().to_string());
    output.insert(coercion.target_type.as_str().to_string());
    collect_coercion_evidence_catalog_identities(&coercion.evidence, output);
}

fn collect_coercion_evidence_catalog_identities(
    evidence: &crate::CoercionEvidence,
    output: &mut BTreeSet<String>,
) {
    match evidence {
        crate::CoercionEvidence::Exact => {}
        crate::CoercionEvidence::CatalogCast { .. } => {}
        crate::CoercionEvidence::DomainBase { domain, base } => {
            output.insert(domain.as_str().to_string());
            output.insert(base.as_str().to_string());
        }
        crate::CoercionEvidence::UnknownLiteral { resolved } => {
            output.insert(resolved.as_str().to_string());
        }
        crate::CoercionEvidence::CommonType { resolved, inputs } => {
            output.insert(resolved.as_str().to_string());
            output.extend(inputs.iter().map(|id| id.as_str().to_string()));
        }
        crate::CoercionEvidence::Polymorphic {
            callable_id,
            bound_types,
        } => {
            output.insert(callable_id.as_str().to_string());
            output.extend(bound_types.iter().map(|id| id.as_str().to_string()));
        }
    }
}

fn collect_conflict_catalog_identities(
    conflict: &crate::TypedConflictClause,
    output: &mut BTreeSet<String>,
) {
    match &conflict.target {
        crate::ConflictTarget::Constraint(constraint) => {
            output.insert(constraint.as_str().to_string());
        }
        crate::ConflictTarget::Inference {
            expressions,
            predicate,
        } => {
            collect_expressions_catalog_identities(expressions, output);
            collect_expression_option_catalog_identities(predicate.as_deref(), output);
        }
        crate::ConflictTarget::Unspecified => {}
    }
    if let crate::TypedConflictAction::Update {
        assignments,
        predicate,
    } = &conflict.action
    {
        collect_assignments_catalog_identities(assignments, output);
        collect_expression_option_catalog_identities(predicate.as_deref(), output);
    }
}

fn collect_assignments_catalog_identities(
    assignments: &[crate::TypedAssignment],
    output: &mut BTreeSet<String>,
) {
    for assignment in assignments {
        output.insert(assignment.target.as_str().to_string());
        collect_expression_catalog_identities(&assignment.value, output);
        if let Some(coercion) = &assignment.coercion {
            collect_coercion_catalog_identities(coercion, output);
        }
    }
}

fn collect_cardinality_catalog_identities(
    cardinality: &Cardinality,
    output: &mut BTreeSet<String>,
) {
    for evidence in cardinality.proof() {
        match evidence {
            crate::CardinalityEvidence::UniquePredicate {
                constraint_id,
                columns,
            } => {
                output.insert(constraint_id.as_str().to_string());
                output.extend(columns.iter().map(|id| id.as_str().to_string()));
            }
            crate::CardinalityEvidence::RegisteredFunction { callable_id } => {
                output.insert(callable_id.as_str().to_string());
            }
            _ => {}
        }
    }
}

fn collect_nullability_catalog_identities(
    nullability: &crate::Nullability,
    output: &mut BTreeSet<String>,
) {
    for evidence in nullability.evidence() {
        match evidence {
            crate::NullabilityEvidence::BaseColumnNotNull { column_id }
            | crate::NullabilityEvidence::BaseColumnNullable { column_id } => {
                output.insert(column_id.as_str().to_string());
            }
            crate::NullabilityEvidence::CallableContract { callable_id, .. } => {
                output.insert(callable_id.as_str().to_string());
            }
            _ => {}
        }
    }
}

fn collect_distinct_catalog_identities(
    distinct: &crate::SelectDistinct<crate::TypedExpression>,
    output: &mut BTreeSet<String>,
) {
    if let crate::SelectDistinct::On(expressions) = distinct {
        collect_expressions_catalog_identities(expressions, output);
    }
}

fn collect_projections_catalog_identities(
    projections: &[crate::TypedProjection],
    output: &mut BTreeSet<String>,
) {
    for projection in projections {
        collect_expression_catalog_identities(&projection.expression, output);
    }
}

fn collect_expressions_catalog_identities(
    expressions: &[crate::TypedExpression],
    output: &mut BTreeSet<String>,
) {
    for expression in expressions {
        collect_expression_catalog_identities(expression, output);
    }
}

fn collect_expression_option_catalog_identities(
    expression: Option<&crate::TypedExpression>,
    output: &mut BTreeSet<String>,
) {
    if let Some(expression) = expression {
        collect_expression_catalog_identities(expression, output);
    }
}

fn collect_ordering_catalog_identities(
    ordering: &[crate::TypedOrderBy],
    output: &mut BTreeSet<String>,
) {
    for order in ordering {
        collect_expression_catalog_identities(&order.expression, output);
    }
}

fn collect_window_reference_catalog_identities(
    window: &crate::WindowReference<crate::TypedExpression>,
    output: &mut BTreeSet<String>,
) {
    if let crate::WindowReference::Inline(specification) = window {
        collect_window_spec_catalog_identities(specification, output);
    }
}

fn collect_window_spec_catalog_identities(
    specification: &crate::WindowSpec<crate::TypedExpression>,
    output: &mut BTreeSet<String>,
) {
    collect_expressions_catalog_identities(&specification.partition_by, output);
    collect_ordering_catalog_identities(&specification.order_by, output);
    if let Some(frame) = &specification.frame {
        collect_frame_bound_catalog_identities(&frame.start, output);
        if let Some(end) = &frame.end {
            collect_frame_bound_catalog_identities(end, output);
        }
    }
}

fn collect_frame_bound_catalog_identities(
    bound: &crate::FrameBound<crate::TypedExpression>,
    output: &mut BTreeSet<String>,
) {
    match bound {
        crate::FrameBound::Preceding(expression) | crate::FrameBound::Following(expression) => {
            collect_expression_catalog_identities(expression, output);
        }
        crate::FrameBound::UnboundedPreceding
        | crate::FrameBound::CurrentRow
        | crate::FrameBound::UnboundedFollowing => {}
    }
}
