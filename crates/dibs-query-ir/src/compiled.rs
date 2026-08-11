use std::collections::BTreeSet;

use dibs_pg_catalog::SchemaFingerprint;

use crate::{
    ArtifactHashes, Cardinality, CompilerVersions, ContentHash, ExecutionIdentity,
    ExecutionIdentityInput, ExecutionParameter, HirQuery, LineageGraph, ManifestIdentity,
    OrderedBind, OutputField, Parameter, ParameterId, PublicContractIdentity, PublicIdentityInput,
    QueryId, QueryManifest, ReadWriteLockManifest, ReferenceIndex, ResultMode, RuntimeAssertion,
    SourceMap, SourceOrigin, TypedStatement, TypedStatementKind, execution_identity,
    public_contract_identity,
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
    /// Resolved HIR and typed IR do not describe the same statement topology.
    HirTypedMismatch,
    /// Result mode, proof, runtime assertions, and output shape disagree.
    ResultModeMismatch,
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
        && manifest.output_fields == query.ordered_output_fields
        && manifest.inferred_cardinality == query.inferred_cardinality
        && manifest.runtime_assertions == query.runtime_assertions
        && manifest.read_write_lock_manifest == query.read_write_lock_manifest
        && manifest.lineage == query.lineage;
    matches
        .then_some(())
        .ok_or(CompiledQueryError::ManifestMismatch)
}
