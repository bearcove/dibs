use dibs_pg_catalog::SchemaFingerprint;

use crate::{
    ArtifactHashes, Cardinality, CompilerVersions, ExecutionIdentity, ExecutionIdentityInput,
    ExecutionParameter, HirQuery, LineageGraph, ManifestIdentity, OrderedBind, OutputField,
    Parameter, PublicContractIdentity, PublicIdentityInput, QueryId, QueryManifest,
    ReadWriteLockManifest, ReferenceIndex, ResultMode, RuntimeAssertion, SourceMap, SourceOrigin,
    TypedStatement,
};

/// Complete immutable checked query artifact consumed by all backends and runtimes.
///
/// It contains no parser state, catalog OIDs, runtime SQL builder, legacy Styx query model,
/// or backend-specific competing parameter/result/reference representation.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
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
