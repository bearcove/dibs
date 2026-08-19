use dibs_pg_catalog::{ApiTypeId, PgCodecId, SchemaFingerprint, TableId, TypeId, WireCodecId};

use crate::{
    Cardinality, CteId, ExecutionIdentity, ExpressionId, FieldId, LineageGraph, Nullability,
    ParameterId, PublicContractIdentity, QueryId, RelationId, SourceOrigin, Typmod, Volatility,
};

/// Runtime result contract declared by a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ResultMode {
    /// Return every row.
    Many,
    /// Accept zero or one row.
    Optional,
    /// Require exactly one row.
    One,
    /// Require a rowless statement and return affected-row count.
    Exec,
}

/// Generated API target language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum TargetLanguage {
    /// Rust API.
    Rust,
    /// TypeScript API.
    TypeScript,
    /// Swift API.
    Swift,
}

/// Compatibility alias emphasizing API-language use sites.
pub type ApiLanguage = TargetLanguage;

/// Mapping from a logical field/parameter to a target-language type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct ApiTypeMapping {
    /// Target language.
    pub language: TargetLanguage,
    /// Lossless API type identity.
    pub type_id: ApiTypeId,
}

/// Validated target-language field name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct ApiFieldName {
    /// Target language.
    pub language: TargetLanguage,
    /// Validated member name.
    pub name: String,
}

/// Validated target-language generated operation name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[facet(invariants = ApiOperationName::is_valid)]
pub struct ApiOperationName {
    /// Target language.
    pub language: TargetLanguage,
    /// Exact validated generated function/method name.
    pub name: String,
}

impl ApiOperationName {
    /// Creates a validated target-language operation name.
    pub fn try_new(
        language: TargetLanguage,
        name: impl Into<String>,
    ) -> Result<Self, ApiContractError> {
        let value = Self {
            language,
            name: name.into(),
        };
        valid_target_identifier(value.language, &value.name)
            .then_some(value)
            .ok_or(ApiContractError::InvalidIdentifier)
    }

    fn is_valid(&self) -> bool {
        valid_target_identifier(self.language, &self.name)
    }
}
/// Target-language parameter ownership/borrowing contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ParameterPassing {
    /// Pass the API value by value.
    Owned,
    /// Borrow the API value immutably.
    SharedReference,
    /// Borrow string contents as `str` rather than the owned string type.
    StringSlice,
    /// Borrow byte contents as a byte slice rather than the owned byte container.
    ByteSlice,
}

/// How a generated target parameter becomes a PostgreSQL bind value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ParameterBindAdapter {
    /// The declared API value implements the PostgreSQL bind contract directly.
    Direct,
    /// Borrowed wrapper/newtype dereferences to the bind value.
    Deref,
    /// Serialize the application Facet value into a PostgreSQL JSONB adapter.
    FacetJsonb,
    /// Convert a typed array/container into the PostgreSQL array adapter.
    PgArray,
    /// Convert an application model through a named lossless adapter type.
    Named(ApiTypeId),
}

/// Complete target-language bind contract for one parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[facet(invariants = ParameterApiContract::is_valid)]
pub struct ParameterApiContract {
    /// Target language.
    pub language: TargetLanguage,
    /// Validated generated parameter name.
    pub name: String,
    /// Public target-language parameter type.
    pub api_type: ApiTypeId,
    /// Ownership or borrowing at the generated API boundary.
    pub passing: ParameterPassing,
    /// Lossless conversion into the PostgreSQL bind representation.
    pub bind_adapter: ParameterBindAdapter,
}

impl ParameterApiContract {
    /// Creates a validated target-language parameter bind contract.
    pub fn try_new(
        language: TargetLanguage,
        name: impl Into<String>,
        api_type: ApiTypeId,
        passing: ParameterPassing,
        bind_adapter: ParameterBindAdapter,
    ) -> Result<Self, ApiContractError> {
        let value = Self {
            language,
            name: name.into(),
            api_type,
            passing,
            bind_adapter,
        };
        value
            .is_valid()
            .then_some(value)
            .ok_or(ApiContractError::IncoherentBindContract)
    }

    fn is_valid(&self) -> bool {
        if !valid_target_identifier(self.language, &self.name) {
            return false;
        }
        match self.bind_adapter {
            ParameterBindAdapter::Direct => true,
            ParameterBindAdapter::Deref
            | ParameterBindAdapter::FacetJsonb
            | ParameterBindAdapter::PgArray
            | ParameterBindAdapter::Named(_) => {
                matches!(self.passing, ParameterPassing::SharedReference)
            }
        }
    }
}

/// Invalid target-language generated API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiContractError {
    /// Target name is not a valid non-keyword identifier.
    InvalidIdentifier,
    /// Ownership/borrowing and bind adapter cannot be applied coherently.
    IncoherentBindContract,
}

impl std::fmt::Display for ApiContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier => formatter.write_str("invalid generated target identifier"),
            Self::IncoherentBindContract => {
                formatter.write_str("incoherent generated parameter bind contract")
            }
        }
    }
}

impl std::error::Error for ApiContractError {}

fn valid_target_identifier(language: TargetLanguage, name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    let lexical = (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    lexical && !target_keywords(language).contains(&name)
}

fn target_keywords(language: TargetLanguage) -> &'static [&'static str] {
    match language {
        TargetLanguage::Rust => &[
            "Self", "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
            "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match",
            "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super",
            "trait", "true", "type", "union", "unsafe", "use", "where", "while",
        ],
        TargetLanguage::TypeScript => &[
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "enum",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "new",
            "null",
            "return",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "var",
            "void",
            "while",
            "with",
            "yield",
        ],
        TargetLanguage::Swift => &[
            "associatedtype",
            "class",
            "deinit",
            "enum",
            "extension",
            "fileprivate",
            "func",
            "import",
            "init",
            "inout",
            "internal",
            "let",
            "open",
            "operator",
            "private",
            "protocol",
            "public",
            "rethrows",
            "static",
            "struct",
            "subscript",
            "typealias",
            "var",
            "break",
            "case",
            "continue",
            "default",
            "defer",
            "do",
            "else",
            "fallthrough",
            "for",
            "guard",
            "if",
            "in",
            "repeat",
            "return",
            "switch",
            "where",
            "while",
        ],
    }
}

fn valid_render_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('\0')
}

fn valid_target_identifier_set(names: &[ApiFieldName]) -> bool {
    canonical_unique_languages(
        names,
        |name| name.language,
        |name| valid_target_identifier(name.language, &name.name),
    )
}

fn canonical_unique_languages<T>(
    values: &[T],
    language: impl Fn(&T) -> TargetLanguage,
    valid: impl Fn(&T) -> bool,
) -> bool {
    values
        .windows(2)
        .all(|pair| language(&pair[0]) < language(&pair[1]))
        && values.iter().all(valid)
}

/// PostgreSQL bind format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum BindFormat {
    /// PostgreSQL text format.
    Text,
    /// PostgreSQL binary format.
    Binary,
}

/// Sensitivity classification for parameters, outputs, and lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum Sensitivity {
    /// Safe for ordinary public contracts and diagnostics.
    Public,
    /// Internal non-secret value.
    Internal,
    /// Confidential application data requiring redaction.
    Confidential,
    /// Secret value never included in traces or generated diagnostics.
    Secret,
}

/// Immutable ordered parameter contract.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = Parameter::is_valid)]
pub struct Parameter {
    /// Stable revision-local parameter identity.
    pub id: ParameterId,
    /// Zero-based declaration ordinal.
    pub ordinal: u32,
    /// Authored public source name.
    pub source_name: String,
    /// Exact declaration origin.
    pub origin: SourceOrigin,
    /// Stable PostgreSQL type identity.
    pub type_id: TypeId,
    /// Resolved typmod.
    pub typmod: Option<Typmod>,
    /// Whether the bind accepts SQL NULL.
    pub nullable: bool,
    /// PostgreSQL storage codec identity.
    pub pg_codec_id: PgCodecId,
    /// Application wire codec identity.
    pub wire_codec_id: WireCodecId,
    /// PostgreSQL bind format.
    pub bind_format: BindFormat,
    /// Target-language bind contracts, canonically ordered by language.
    pub api_contracts: Vec<ParameterApiContract>,
    /// Redaction classification.
    pub sensitivity: Sensitivity,
}

impl Parameter {
    fn is_valid(&self) -> bool {
        canonical_unique_languages(
            &self.api_contracts,
            |contract| contract.language,
            ParameterApiContract::is_valid,
        )
    }
}

/// Immutable ordered output-field contract.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = OutputField::is_valid)]
pub struct OutputField {
    /// Stable revision-local output identity.
    pub id: FieldId,
    /// Zero-based output ordinal.
    pub ordinal: u32,
    /// Exact SQL wire label.
    pub sql_label: String,
    /// Stable public/wire name.
    pub public_name: String,
    /// Stable PostgreSQL type identity.
    pub type_id: TypeId,
    /// Resolved typmod.
    pub typmod: Option<Typmod>,
    /// Conservative proof-bearing nullability.
    pub nullability: Nullability,
    /// PostgreSQL storage codec identity.
    pub pg_codec_id: PgCodecId,
    /// Application wire codec identity.
    pub wire_codec_id: WireCodecId,
    /// Target-language type mappings, canonically ordered by language.
    pub api_types: Vec<ApiTypeMapping>,
    /// Target-language validated member names, canonically ordered by language.
    pub api_names: Vec<ApiFieldName>,
    /// Typed source expression.
    pub source_expression: ExpressionId,
    /// Root node in the result-lineage graph.
    pub lineage_root: crate::LineageNodeId,
    /// Security classification.
    pub sensitivity: Sensitivity,
}

impl OutputField {
    fn is_valid(&self) -> bool {
        valid_render_name(&self.sql_label)
            && valid_target_identifier_set(&self.api_names)
            && canonical_unique_languages(&self.api_types, |mapping| mapping.language, |_| true)
    }
}

/// One PostgreSQL positional bind in deterministic execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct OrderedBind {
    /// One-based PostgreSQL bind position.
    pub position: u32,
    /// Declared parameter reused at this position.
    pub parameter_id: ParameterId,
}

/// Runtime assertion required when static proof cannot establish the declared mode.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum RuntimeAssertion {
    /// Result must contain at most this many rows.
    AtMostRows {
        /// Maximum allowed row count.
        maximum: u64,
    },
    /// Result must contain at least this many rows.
    AtLeastRows {
        /// Minimum required row count.
        minimum: u64,
    },
    /// Final statement must return no rows.
    Rowless,
    /// Dynamic limit parameter must be non-negative and fit PostgreSQL semantics.
    ValidLimitParameter {
        /// Parameter carrying the limit.
        parameter_id: ParameterId,
    },
}

/// Exact table lock retained by the artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct LockManifestEntry {
    /// Stable table identity.
    pub table_id: TableId,
    /// Lock strength.
    pub strength: crate::LockStrength,
    /// Wait policy.
    pub wait: crate::LockWaitPolicy,
}

/// Mutation topology summary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum MutationManifest {
    /// INSERT target.
    Insert {
        /// Stable target table.
        target: TableId,
    },
    /// UPDATE target.
    Update {
        /// Stable target table.
        target: TableId,
        /// Whether a predicate is present.
        has_predicate: bool,
    },
    /// DELETE target.
    Delete {
        /// Stable target table.
        target: TableId,
        /// Whether a predicate is present.
        has_predicate: bool,
    },
}

/// Read/write/lock and volatility summary used by reviews and runtime policy.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct ReadWriteLockManifest {
    /// Stable table reads as a semantic set.
    pub reads: Vec<TableId>,
    /// Stable table writes as a semantic set.
    pub writes: Vec<TableId>,
    /// Stable lock set.
    pub locks: Vec<LockManifestEntry>,
    /// Maximum statement volatility.
    pub volatility: Volatility,
    /// Optional mutation topology.
    pub mutation: Option<MutationManifest>,
}

impl ReadWriteLockManifest {
    /// Returns a clone with only semantically unordered collections canonicalized.
    #[must_use]
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.reads.sort();
        value.reads.dedup();
        value.writes.sort();
        value.writes.dedup();
        value.locks.sort();
        value.locks.dedup();
        value
    }
}

/// Parent/child relation edge retained for analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct RelationEdge {
    /// Parent relation.
    pub parent: RelationId,
    /// Child relation.
    pub child: RelationId,
}

/// CTE dependency edge retained for analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct CteDependency {
    /// CTE that depends on another.
    pub from: CteId,
    /// Dependency CTE.
    pub to: CteId,
}

/// Opaque boundary the compiler cannot analyze soundly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct OpaqueAnalysisBoundary {
    /// Stable category/code.
    pub code: String,
    /// Human review message.
    pub message: String,
    /// Optional source origin.
    pub origin: Option<SourceOrigin>,
}

/// Hash of a generated target output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct GeneratedOutputHash {
    /// Target language.
    pub language: TargetLanguage,
    /// Generator version.
    pub generator_version: String,
    /// BLAKE3 content hash.
    pub hash: ContentHash,
}

/// Lowercase BLAKE3 content digest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    /// Hashes exact bytes using BLAKE3.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    /// Hashes one Facet value after deterministic facet-json serialization.
    pub fn of_json<'facet, T: facet::Facet<'facet> + ?Sized>(value: &T) -> Result<Self, String> {
        facet_json::to_vec(value)
            .map(|bytes| Self::of_bytes(&bytes))
            .map_err(|error| error.to_string())
    }

    /// Returns lowercase BLAKE3 hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Version fields embedded into every artifact and identity input.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct CompilerVersions {
    /// Compiled artifact schema version.
    pub artifact_schema_version: u32,
    /// Compiler semantic version.
    pub compiler_semantic_version: String,
    /// Query language grammar/semantic version.
    pub query_language_version: u16,
    /// Supported PostgreSQL major version.
    pub supported_postgres_major: u16,
    /// Execution identity canonical format version.
    pub execution_identity_format_version: u16,
    /// Public identity canonical format version.
    pub public_identity_format_version: u16,
    /// Manifest canonical format version.
    pub manifest_format_version: u16,
}

/// Machine-readable query review/observability manifest.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct QueryManifest {
    /// Manifest canonical format version.
    pub manifest_format_version: u16,
    /// Revision-local query identity.
    pub query_id: QueryId,
    /// Stable execution semantics identity.
    pub execution_semantics_id: ExecutionIdentity,
    /// Stable public contract identity.
    pub public_contract_id: PublicContractIdentity,
    /// Compiler/artifact/language/PostgreSQL versions.
    pub compiler_versions: CompilerVersions,
    /// Stable schema/catalog fingerprint.
    pub catalog_schema_fingerprint: SchemaFingerprint,
    /// Validated target-language operation names as a semantic set.
    pub operation_names: Vec<ApiOperationName>,
    /// Hash of deterministic normalized SQL.
    pub normalized_sql_hash: ContentHash,
    /// Hash of authored source content.
    pub source_hash: ContentHash,
    /// Hash of the source map artifact.
    pub source_map_hash: ContentHash,
    /// Generated output hashes as a semantic set.
    pub generated_output_hashes: Vec<GeneratedOutputHash>,
    /// Ordered parameter contract.
    pub parameters: Vec<Parameter>,
    /// Ordered output contract.
    pub output_fields: Vec<OutputField>,
    /// Inferred cardinality and proof.
    pub inferred_cardinality: Cardinality,
    /// Required runtime assertions in semantic order.
    pub runtime_assertions: Vec<RuntimeAssertion>,
    /// Parent/child relation edges as a semantic set.
    pub relation_edges: Vec<RelationEdge>,
    /// CTE dependency edges as a semantic set.
    pub cte_dependencies: Vec<CteDependency>,
    /// Reads, writes, locks, volatility, mutation.
    pub read_write_lock_manifest: ReadWriteLockManifest,
    /// Result lineage graph.
    pub lineage: LineageGraph,
    /// Explicit opaque analysis boundaries as a semantic set.
    pub opaque_analysis_boundaries: Vec<OpaqueAnalysisBoundary>,
    /// Optional plan baseline identity.
    pub plan_baseline_identity: Option<String>,
}

impl QueryManifest {
    /// Returns a clone with only semantically unordered collections canonicalized.
    #[must_use]
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.generated_output_hashes.sort_by(|left, right| {
            (&left.language, &left.generator_version, &left.hash).cmp(&(
                &right.language,
                &right.generator_version,
                &right.hash,
            ))
        });
        value.generated_output_hashes.dedup();
        value.relation_edges.sort();
        value.relation_edges.dedup();
        value.cte_dependencies.sort();
        value.cte_dependencies.dedup();
        value.read_write_lock_manifest = value.read_write_lock_manifest.canonicalized();
        value.lineage = value.lineage.canonicalized();
        value.opaque_analysis_boundaries.sort();
        value.opaque_analysis_boundaries.dedup();
        value.operation_names.sort();
        value.operation_names.dedup();
        for parameter in &mut value.parameters {
            parameter.api_contracts.sort();
            parameter.api_contracts.dedup();
        }
        for field in &mut value.output_fields {
            field.api_types.sort();
            field.api_types.dedup();
            field.api_names.sort();
            field.api_names.dedup();
        }
        value
    }
}

/// All immutable artifact content hashes.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct ArtifactHashes {
    /// Deterministic SQL hash.
    pub normalized_sql: ContentHash,
    /// Source hash.
    pub source: ContentHash,
    /// Source map hash.
    pub source_map: ContentHash,
    /// Canonical manifest hash.
    pub manifest: ContentHash,
    /// Generated output hashes as a semantic set.
    pub generated_outputs: Vec<GeneratedOutputHash>,
}
