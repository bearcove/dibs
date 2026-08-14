use dibs_pg_catalog::{SchemaFingerprint, TypeId};

use crate::{
    ApiOperationName, ApiResultTypeName, Parameter, ParameterId, QueryManifest,
    ReadWriteLockManifest, ReferenceIndex, ResultMode, TypedStatement, Typmod,
};

macro_rules! hash_identity {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
        #[repr(transparent)]
        pub struct $name(String);

        impl $name {
            fn hash_bytes(bytes: &[u8]) -> Self {
                Self(blake3::hash(bytes).to_hex().to_string())
            }

            /// Returns lowercase BLAKE3 hex.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

hash_identity!(
    ExecutionIdentity,
    "Versioned BLAKE3 identity of execution semantics only."
);
hash_identity!(
    PublicContractIdentity,
    "Versioned BLAKE3 identity of the public operation contract."
);
hash_identity!(
    ManifestIdentity,
    "Versioned BLAKE3 identity of the canonical emitted manifest."
);

/// Execution-relevant parameter shape with names and bind positions erased.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct ExecutionParameter {
    /// Revision-local parameter identity used by typed references.
    pub id: ParameterId,
    /// PostgreSQL type.
    pub type_id: TypeId,
    /// Typmod.
    pub typmod: Option<Typmod>,
    /// Bind nullability.
    pub nullable: bool,
}

/// Canonical input to the execution-semantics identity.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct ExecutionIdentityInput {
    /// Canonical identity format version.
    pub version: u16,
    /// PostgreSQL major semantics.
    pub postgres_major: u16,
    /// Typed statement topology without aliases or source spans.
    pub statement: TypedStatement,
    /// Parameters in declaration/semantic order, with names erased.
    pub parameters: Vec<ExecutionParameter>,
    /// Runtime result mode when it affects execution enforcement.
    pub result_mode: ResultMode,
    /// Runtime row/value assertions affecting execution enforcement.
    pub runtime_assertions: Vec<crate::RuntimeAssertion>,
    /// Stable resolved semantic references.
    pub references: ReferenceIndex,
    /// Reads/writes/locks/volatility/mutation topology.
    pub read_write_lock_manifest: ReadWriteLockManifest,
    /// Schema/catalog truth used for resolution.
    pub catalog_schema_fingerprint: SchemaFingerprint,
}

/// Canonical input to the public-contract identity.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct PublicIdentityInput {
    /// Canonical identity format version.
    pub version: u16,
    /// Public declaration name.
    pub query_name: String,
    /// Validated target-language operation names as a semantic set.
    pub operation_names: Vec<ApiOperationName>,
    /// Validated target-language result type names as a semantic set.
    pub result_type_names: Vec<ApiResultTypeName>,
    /// Parameters in public declaration order.
    pub parameters: Vec<Parameter>,
    /// Fields in public output order.
    pub output_fields: Vec<crate::OutputField>,
    /// Declared runtime result mode.
    pub result_mode: ResultMode,
    /// Optional transport-envelope identity.
    pub transport_envelope: Option<String>,
}

/// Computes the execution-semantics identity after erasing presentation metadata.
#[must_use]
pub fn execution_identity(input: &ExecutionIdentityInput) -> ExecutionIdentity {
    let canonical = CanonicalExecutionIdentityInput::from(input);
    let bytes = facet_json::to_vec(&canonical)
        .expect("execution identity input is fully representable as Facet JSON");
    ExecutionIdentity::hash_bytes(&bytes)
}

/// Computes the public-contract identity while preserving public semantic order.
#[must_use]
pub fn public_contract_identity(input: &PublicIdentityInput) -> PublicContractIdentity {
    let canonical = CanonicalPublicIdentityInput::from(input);
    let bytes = facet_json::to_vec(&canonical)
        .expect("public identity input is fully representable as Facet JSON");
    PublicContractIdentity::hash_bytes(&bytes)
}

/// Serializes a canonical manifest with facet-json.
pub fn canonical_manifest_json(manifest: &QueryManifest) -> Result<Vec<u8>, String> {
    facet_json::to_vec(&manifest.canonicalized()).map_err(|error| error.to_string())
}

impl ManifestIdentity {
    /// Computes the identity of a canonicalized manifest.
    pub fn from_manifest(manifest: &QueryManifest) -> Result<Self, String> {
        canonical_manifest_json(manifest).map(|bytes| Self::hash_bytes(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct CanonicalExecutionIdentityInput {
    version: u16,
    postgres_major: u16,
    statement: SemanticStatement,
    parameters: Vec<ExecutionParameter>,
    result_mode: ResultMode,
    runtime_assertions: Vec<crate::RuntimeAssertion>,
    references: Vec<SemanticReference>,
    read_write_lock_manifest: ReadWriteLockManifest,
    catalog_schema_fingerprint: SchemaFingerprint,
}

impl From<&ExecutionIdentityInput> for CanonicalExecutionIdentityInput {
    fn from(input: &ExecutionIdentityInput) -> Self {
        let references = input
            .references
            .canonicalized()
            .references
            .into_iter()
            .map(|reference| SemanticReference {
                target: reference.target,
                role: reference.role,
                access: reference.access,
            })
            .collect();
        let mut runtime_assertions = input.runtime_assertions.clone();
        runtime_assertions.sort();
        runtime_assertions.dedup();
        Self {
            version: input.version,
            postgres_major: input.postgres_major,
            statement: SemanticStatement::from(&input.statement),
            parameters: input.parameters.clone(),
            result_mode: input.result_mode,
            runtime_assertions,
            references,
            read_write_lock_manifest: input.read_write_lock_manifest.canonicalized(),
            catalog_schema_fingerprint: input.catalog_schema_fingerprint.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticReference {
    target: crate::ReferenceTarget,
    role: crate::ReferenceRole,
    access: crate::ReferenceAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct CanonicalPublicIdentityInput {
    version: u16,
    query_name: String,
    operation_names: Vec<ApiOperationName>,
    result_type_names: Vec<ApiResultTypeName>,
    parameters: Vec<PublicParameter>,
    output_fields: Vec<PublicOutputField>,
    result_mode: ResultMode,
    transport_envelope: Option<String>,
}

impl From<&PublicIdentityInput> for CanonicalPublicIdentityInput {
    fn from(input: &PublicIdentityInput) -> Self {
        let mut operation_names = input.operation_names.clone();
        operation_names.sort();
        operation_names.dedup();
        let mut result_type_names = input.result_type_names.clone();
        result_type_names.sort();
        result_type_names.dedup();
        Self {
            version: input.version,
            query_name: input.query_name.clone(),
            operation_names,
            result_type_names,
            parameters: input.parameters.iter().map(PublicParameter::from).collect(),
            output_fields: input
                .output_fields
                .iter()
                .map(PublicOutputField::from)
                .collect(),
            result_mode: input.result_mode,
            transport_envelope: input.transport_envelope.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct PublicParameter {
    ordinal: u32,
    source_name: String,
    type_id: TypeId,
    typmod: Option<Typmod>,
    nullable: bool,
    pg_codec_id: dibs_pg_catalog::PgCodecId,
    wire_codec_id: dibs_pg_catalog::WireCodecId,
    bind_format: crate::BindFormat,
    api_contracts: Vec<crate::ParameterApiContract>,
    sensitivity: crate::Sensitivity,
}

impl From<&Parameter> for PublicParameter {
    fn from(parameter: &Parameter) -> Self {
        let mut api_contracts = parameter.api_contracts.clone();
        api_contracts.sort();
        api_contracts.dedup();
        Self {
            ordinal: parameter.ordinal,
            source_name: parameter.source_name.clone(),
            type_id: parameter.type_id.clone(),
            typmod: parameter.typmod.clone(),
            nullable: parameter.nullable,
            pg_codec_id: parameter.pg_codec_id.clone(),
            wire_codec_id: parameter.wire_codec_id.clone(),
            bind_format: parameter.bind_format,
            api_contracts,
            sensitivity: parameter.sensitivity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct PublicOutputField {
    ordinal: u32,
    sql_label: String,
    public_name: String,
    type_id: TypeId,
    typmod: Option<Typmod>,
    nullability: crate::Nullability,
    pg_codec_id: dibs_pg_catalog::PgCodecId,
    wire_codec_id: dibs_pg_catalog::WireCodecId,
    api_types: Vec<crate::ApiTypeMapping>,
    api_names: Vec<crate::ApiFieldName>,
    sensitivity: crate::Sensitivity,
}

impl From<&crate::OutputField> for PublicOutputField {
    fn from(field: &crate::OutputField) -> Self {
        let mut api_types = field.api_types.clone();
        api_types.sort();
        api_types.dedup();
        let mut api_names = field.api_names.clone();
        api_names.sort();
        api_names.dedup();
        Self {
            ordinal: field.ordinal,
            sql_label: field.sql_label.clone(),
            public_name: field.public_name.clone(),
            type_id: field.type_id.clone(),
            typmod: field.typmod.clone(),
            nullability: field.nullability.clone(),
            pg_codec_id: field.pg_codec_id.clone(),
            wire_codec_id: field.wire_codec_id.clone(),
            api_types,
            api_names,
            sensitivity: field.sensitivity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticStatement {
    id: crate::StatementId,
    cardinality: crate::Cardinality,
    kind: SemanticStatementKind,
}

impl From<&TypedStatement> for SemanticStatement {
    fn from(statement: &TypedStatement) -> Self {
        Self {
            id: statement.id,
            cardinality: statement.cardinality.clone(),
            kind: SemanticStatementKind::from(&statement.kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticStatementKind {
    Select(Box<SemanticSelect>),
    Insert(Box<SemanticInsert>),
    Update(Box<SemanticUpdate>),
    Delete(Box<SemanticDelete>),
}

impl From<&crate::TypedStatementKind> for SemanticStatementKind {
    fn from(kind: &crate::TypedStatementKind) -> Self {
        match kind {
            crate::TypedStatementKind::Select(select) => {
                Self::Select(Box::new(SemanticSelect::from(select.as_ref())))
            }
            crate::TypedStatementKind::Insert(insert) => {
                Self::Insert(Box::new(SemanticInsert::from(insert.as_ref())))
            }
            crate::TypedStatementKind::Update(update) => {
                Self::Update(Box::new(SemanticUpdate::from(update.as_ref())))
            }
            crate::TypedStatementKind::Delete(delete) => {
                Self::Delete(Box::new(SemanticDelete::from(delete.as_ref())))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticSelect {
    recursive: bool,
    ctes: Vec<SemanticCte>,
    distinct: SemanticSelectDistinct,
    projections: Vec<SemanticProjection>,
    from: Vec<SemanticRelation>,
    predicate: Option<SemanticExpression>,
    group_by: Vec<SemanticExpression>,
    having: Option<SemanticExpression>,
    windows: Vec<SemanticNamedWindow>,
    order_by: Vec<SemanticOrderBy>,
    limit: Option<crate::TypedLimit>,
    offset: Option<crate::TypedLimit>,
    locks: Vec<crate::HirLockClause>,
}

impl From<&crate::TypedSelect> for SemanticSelect {
    fn from(select: &crate::TypedSelect) -> Self {
        Self {
            recursive: select.recursive,
            ctes: select.ctes.iter().map(SemanticCte::from).collect(),
            distinct: SemanticSelectDistinct::from(&select.distinct),
            projections: select
                .projections
                .iter()
                .map(SemanticProjection::from)
                .collect(),
            from: select.from.iter().map(SemanticRelation::from).collect(),
            predicate: select.predicate.as_ref().map(SemanticExpression::from),
            group_by: select
                .group_by
                .iter()
                .map(SemanticExpression::from)
                .collect(),
            having: select.having.as_ref().map(SemanticExpression::from),
            windows: select
                .windows
                .iter()
                .map(SemanticNamedWindow::from)
                .collect(),
            order_by: select.order_by.iter().map(SemanticOrderBy::from).collect(),
            limit: select.limit.clone(),
            offset: select.offset.clone(),
            locks: select.locks.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticSelectDistinct {
    AllRows,
    Distinct,
    On(Vec<SemanticExpression>),
}

impl From<&crate::SelectDistinct<crate::TypedExpression>> for SemanticSelectDistinct {
    fn from(distinct: &crate::SelectDistinct<crate::TypedExpression>) -> Self {
        match distinct {
            crate::SelectDistinct::AllRows => Self::AllRows,
            crate::SelectDistinct::Distinct => Self::Distinct,
            crate::SelectDistinct::On(expressions) => {
                Self::On(expressions.iter().map(SemanticExpression::from).collect())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticNamedWindow {
    name: String,
    specification: SemanticWindowSpec,
}

impl From<&crate::TypedNamedWindow> for SemanticNamedWindow {
    fn from(window: &crate::TypedNamedWindow) -> Self {
        Self {
            name: window.name.clone(),
            specification: SemanticWindowSpec::from(&window.specification),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticWindowReference {
    Named(String),
    Inline(Box<SemanticWindowSpec>),
}

impl From<&crate::WindowReference<crate::TypedExpression>> for SemanticWindowReference {
    fn from(window: &crate::WindowReference<crate::TypedExpression>) -> Self {
        match window {
            crate::WindowReference::Named(name) => Self::Named(name.clone()),
            crate::WindowReference::Inline(specification) => {
                Self::Inline(Box::new(SemanticWindowSpec::from(specification)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticWindowSpec {
    existing: Option<String>,
    partition_by: Vec<SemanticExpression>,
    order_by: Vec<SemanticOrderBy>,
    frame: Option<SemanticWindowFrame>,
}

impl From<&crate::WindowSpec<crate::TypedExpression>> for SemanticWindowSpec {
    fn from(specification: &crate::WindowSpec<crate::TypedExpression>) -> Self {
        Self {
            existing: specification.existing.clone(),
            partition_by: specification
                .partition_by
                .iter()
                .map(SemanticExpression::from)
                .collect(),
            order_by: specification
                .order_by
                .iter()
                .map(SemanticOrderBy::from)
                .collect(),
            frame: specification.frame.as_ref().map(SemanticWindowFrame::from),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticWindowFrame {
    mode: crate::WindowFrameMode,
    start: SemanticFrameBound,
    end: Option<SemanticFrameBound>,
    exclusion: crate::WindowExclusion,
}

impl From<&crate::WindowFrame<crate::TypedExpression>> for SemanticWindowFrame {
    fn from(frame: &crate::WindowFrame<crate::TypedExpression>) -> Self {
        Self {
            mode: frame.mode,
            start: SemanticFrameBound::from(&frame.start),
            end: frame.end.as_ref().map(SemanticFrameBound::from),
            exclusion: frame.exclusion,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticFrameBound {
    UnboundedPreceding,
    Preceding(SemanticExpression),
    CurrentRow,
    Following(SemanticExpression),
    UnboundedFollowing,
}

impl From<&crate::FrameBound<crate::TypedExpression>> for SemanticFrameBound {
    fn from(bound: &crate::FrameBound<crate::TypedExpression>) -> Self {
        match bound {
            crate::FrameBound::UnboundedPreceding => Self::UnboundedPreceding,
            crate::FrameBound::Preceding(expression) => {
                Self::Preceding(SemanticExpression::from(expression))
            }
            crate::FrameBound::CurrentRow => Self::CurrentRow,
            crate::FrameBound::Following(expression) => {
                Self::Following(SemanticExpression::from(expression))
            }
            crate::FrameBound::UnboundedFollowing => Self::UnboundedFollowing,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticCte {
    id: crate::CteId,
    recursive: bool,
    materialization: crate::CteMaterialization,
    statement: Box<SemanticStatement>,
    output_fields: Vec<crate::FieldId>,
}

impl From<&crate::TypedCte> for SemanticCte {
    fn from(cte: &crate::TypedCte) -> Self {
        Self {
            id: cte.id,
            recursive: cte.recursive,
            materialization: cte.materialization,
            statement: Box::new(SemanticStatement::from(cte.statement.as_ref())),
            output_fields: cte.output_fields().to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticProjection {
    field_id: crate::FieldId,
    expression: SemanticExpression,
    coercion: Option<crate::TypedCoercion>,
}

impl From<&crate::TypedProjection> for SemanticProjection {
    fn from(projection: &crate::TypedProjection) -> Self {
        Self {
            field_id: projection.field_id,
            expression: SemanticExpression::from(&projection.expression),
            coercion: projection.coercion.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticRelation {
    id: crate::RelationId,
    cardinality: crate::Cardinality,
    kind: SemanticRelationKind,
}

impl From<&crate::TypedRelation> for SemanticRelation {
    fn from(relation: &crate::TypedRelation) -> Self {
        Self {
            id: relation.id,
            cardinality: relation.cardinality.clone(),
            kind: SemanticRelationKind::from(&relation.kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticRelationKind {
    Table {
        table_id: dibs_pg_catalog::TableId,
    },
    Cte {
        cte_id: crate::CteId,
    },
    Subquery(Box<SemanticStatement>),
    Function {
        callable_id: dibs_pg_catalog::CallableId,
        arguments: Vec<SemanticExpression>,
    },
    Join {
        kind: crate::JoinKind,
        left: Box<SemanticRelation>,
        right: Box<SemanticRelation>,
        predicate: Option<Box<SemanticExpression>>,
        lateral: bool,
    },
    Values {
        rows: Vec<Vec<SemanticArgument>>,
        columns: Vec<crate::TypedValuesColumn>,
    },
    SetOperation {
        kind: crate::SetOperationKind,
        all: bool,
        left: Box<SemanticStatement>,
        right: Box<SemanticStatement>,
    },
}

impl From<&crate::TypedRelationKind> for SemanticRelationKind {
    fn from(kind: &crate::TypedRelationKind) -> Self {
        match kind {
            crate::TypedRelationKind::Table { table_id } => Self::Table {
                table_id: table_id.clone(),
            },
            crate::TypedRelationKind::Cte { cte_id } => Self::Cte { cte_id: *cte_id },
            crate::TypedRelationKind::Subquery(statement) => {
                Self::Subquery(Box::new(SemanticStatement::from(statement.as_ref())))
            }
            crate::TypedRelationKind::Function {
                callable_id,
                arguments,
            } => Self::Function {
                callable_id: callable_id.clone(),
                arguments: arguments.iter().map(SemanticExpression::from).collect(),
            },
            crate::TypedRelationKind::Join {
                kind,
                left,
                right,
                predicate,
                lateral,
            } => Self::Join {
                kind: *kind,
                left: Box::new(SemanticRelation::from(left.as_ref())),
                right: Box::new(SemanticRelation::from(right.as_ref())),
                predicate: predicate
                    .as_ref()
                    .map(|value| Box::new(SemanticExpression::from(value.as_ref()))),
                lateral: *lateral,
            },
            crate::TypedRelationKind::Values { rows } => Self::Values {
                rows: rows
                    .rows()
                    .iter()
                    .map(|row| row.iter().map(SemanticArgument::from).collect())
                    .collect(),
                columns: rows.columns().to_vec(),
            },
            crate::TypedRelationKind::SetOperation {
                kind,
                all,
                left,
                right,
            } => Self::SetOperation {
                kind: *kind,
                all: *all,
                left: Box::new(SemanticStatement::from(left.as_ref())),
                right: Box::new(SemanticStatement::from(right.as_ref())),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticExpression {
    id: crate::ExpressionId,
    type_id: TypeId,
    typmod: Option<Typmod>,
    nullability: crate::Nullability,
    volatility: crate::Volatility,
    kind: SemanticExpressionKind,
}

impl From<&crate::TypedExpression> for SemanticExpression {
    fn from(expression: &crate::TypedExpression) -> Self {
        Self {
            id: expression.id,
            type_id: expression.type_id.clone(),
            typmod: expression.typmod.clone(),
            nullability: expression.nullability.clone(),
            volatility: expression.volatility,
            kind: SemanticExpressionKind::from(&expression.kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticExpressionKind {
    Literal(crate::HirLiteral),
    Parameter(ParameterId),
    Column {
        binding: crate::RelationId,
        column_id: dibs_pg_catalog::ColumnId,
    },
    DerivedColumn {
        binding: crate::RelationId,
        field_id: crate::FieldId,
    },
    Call {
        callable_id: dibs_pg_catalog::CallableId,
        arguments: Vec<SemanticExpression>,
        coercions: Vec<Option<crate::TypedCoercion>>,
        distinct: bool,
        star: bool,
        order_by: Vec<SemanticOrderBy>,
        filter: Option<Box<SemanticExpression>>,
        within_group: Vec<SemanticWithinGroupOrderBy>,
        over: Option<SemanticWindowReference>,
    },
    Operator {
        operator_id: dibs_pg_catalog::OperatorId,
        operands: Vec<SemanticExpression>,
        coercions: Vec<Option<crate::TypedCoercion>>,
    },
    NullIf {
        operator_id: dibs_pg_catalog::OperatorId,
        left: Box<SemanticArgument>,
        right: Box<SemanticArgument>,
    },
    QuantifiedComparison {
        operator_id: dibs_pg_catalog::OperatorId,
        left: Box<SemanticArgument>,
        right: Box<SemanticArgument>,
        quantifier: crate::ComparisonQuantifier,
    },
    InList {
        expression: Box<SemanticArgument>,
        values: Vec<SemanticArgument>,
        negated: bool,
        coercion: crate::CoercionEvidence,
    },
    Cast {
        cast_id: dibs_pg_catalog::CastId,
        expression: Box<SemanticExpression>,
        coercion: crate::TypedCoercion,
    },
    ExplicitCast {
        expression: Box<SemanticExpression>,
        coercion: Option<crate::TypedCoercion>,
    },
    Collate {
        collation_id: dibs_pg_catalog::CollationId,
        expression: Box<SemanticExpression>,
    },
    Exists(Box<SemanticStatement>),
    Case {
        operand: Option<Box<SemanticExpression>>,
        branches: Vec<SemanticCaseBranch>,
        else_expression: Option<Box<SemanticArgument>>,
        implicit_else_type: Option<dibs_pg_catalog::TypeId>,
        result_coercion: crate::CoercionEvidence,
    },
    Coalesce {
        arguments: Vec<SemanticArgument>,
        coercion: crate::CoercionEvidence,
    },
    Greatest {
        arguments: Vec<SemanticArgument>,
        coercion: crate::CoercionEvidence,
    },
    Least {
        arguments: Vec<SemanticArgument>,
        coercion: crate::CoercionEvidence,
    },
    Extract {
        field: crate::ExtractField,
        source: Box<SemanticExpression>,
    },
    Position {
        substring: Box<SemanticArgument>,
        string: Box<SemanticArgument>,
        input_type: TypeId,
    },
    ScalarSubquery(Box<SemanticStatement>),
    Row(Vec<SemanticExpression>),
    Array {
        elements: Vec<SemanticArgument>,
        coercion: crate::CoercionEvidence,
    },
    CteColumn {
        cte_id: crate::CteId,
        binding: crate::RelationId,
        field_id: crate::FieldId,
    },
}

impl From<&crate::TypedExpressionKind> for SemanticExpressionKind {
    fn from(kind: &crate::TypedExpressionKind) -> Self {
        match kind {
            crate::TypedExpressionKind::Literal(literal) => Self::Literal(literal.clone()),
            crate::TypedExpressionKind::Parameter(parameter) => Self::Parameter(*parameter),
            crate::TypedExpressionKind::Column { binding, column_id } => Self::Column {
                binding: *binding,
                column_id: column_id.clone(),
            },
            crate::TypedExpressionKind::DerivedColumn { binding, field_id } => {
                Self::DerivedColumn {
                    binding: *binding,
                    field_id: *field_id,
                }
            }
            crate::TypedExpressionKind::Call(call) => Self::Call {
                callable_id: call.callable_id.clone(),
                arguments: call
                    .arguments
                    .iter()
                    .map(|argument| SemanticExpression::from(&argument.expression))
                    .collect(),
                coercions: call
                    .arguments
                    .iter()
                    .map(|argument| argument.coercion.clone())
                    .collect(),
                distinct: call.distinct,
                star: call.star,
                order_by: call.order_by.iter().map(SemanticOrderBy::from).collect(),
                filter: call
                    .filter
                    .as_ref()
                    .map(|expression| Box::new(SemanticExpression::from(expression.as_ref()))),
                within_group: call
                    .within_group
                    .iter()
                    .map(SemanticWithinGroupOrderBy::from)
                    .collect(),
                over: call.over.as_ref().map(SemanticWindowReference::from),
            },
            crate::TypedExpressionKind::Extract { field, source } => Self::Extract {
                field: *field,
                source: Box::new(SemanticExpression::from(source.as_ref())),
            },
            crate::TypedExpressionKind::Position {
                substring,
                string,
                input_type,
            } => Self::Position {
                substring: Box::new(SemanticArgument::from(substring.as_ref())),
                string: Box::new(SemanticArgument::from(string.as_ref())),
                input_type: input_type.clone(),
            },
            crate::TypedExpressionKind::Operator {
                operator_id,
                operands,
                ..
            } => Self::Operator {
                operator_id: operator_id.clone(),
                operands: operands
                    .iter()
                    .map(|operand| SemanticExpression::from(&operand.expression))
                    .collect(),
                coercions: operands
                    .iter()
                    .map(|operand| operand.coercion.clone())
                    .collect(),
            },
            crate::TypedExpressionKind::NullIf {
                operator_id,
                left,
                right,
                ..
            } => Self::NullIf {
                operator_id: operator_id.clone(),
                left: Box::new(SemanticArgument::from(left.as_ref())),
                right: Box::new(SemanticArgument::from(right.as_ref())),
            },
            crate::TypedExpressionKind::QuantifiedComparison {
                operator_id,
                left,
                right,
                quantifier,
                ..
            } => Self::QuantifiedComparison {
                operator_id: operator_id.clone(),
                left: Box::new(SemanticArgument::from(left.as_ref())),
                right: Box::new(SemanticArgument::from(right.as_ref())),
                quantifier: *quantifier,
            },
            crate::TypedExpressionKind::InList {
                expression,
                values,
                negated,
                coercion,
            } => Self::InList {
                expression: Box::new(SemanticArgument::from(expression.as_ref())),
                values: values.iter().map(SemanticArgument::from).collect(),
                negated: *negated,
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::Cast {
                cast_id,
                expression,
                coercion,
            } => Self::Cast {
                cast_id: cast_id.clone(),
                expression: Box::new(SemanticExpression::from(expression.as_ref())),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::ExplicitCast {
                expression,
                coercion,
            } => Self::ExplicitCast {
                expression: Box::new(SemanticExpression::from(expression.as_ref())),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::Collate {
                collation_id,
                expression,
            } => Self::Collate {
                collation_id: collation_id.clone(),
                expression: Box::new(SemanticExpression::from(expression.as_ref())),
            },
            crate::TypedExpressionKind::Exists(statement) => {
                Self::Exists(Box::new(SemanticStatement::from(statement.as_ref())))
            }
            crate::TypedExpressionKind::Case {
                operand,
                branches,
                else_expression,
                implicit_else_type,
                result_coercion,
            } => Self::Case {
                operand: operand
                    .as_ref()
                    .map(|value| Box::new(SemanticExpression::from(value.as_ref()))),
                branches: branches.iter().map(SemanticCaseBranch::from).collect(),
                else_expression: else_expression
                    .as_ref()
                    .map(|value| Box::new(SemanticArgument::from(value.as_ref()))),
                implicit_else_type: implicit_else_type.clone(),
                result_coercion: result_coercion.clone(),
            },
            crate::TypedExpressionKind::Coalesce {
                arguments,
                coercion,
            } => Self::Coalesce {
                arguments: arguments.iter().map(SemanticArgument::from).collect(),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::Greatest {
                arguments,
                coercion,
            } => Self::Greatest {
                arguments: arguments.iter().map(SemanticArgument::from).collect(),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::Least {
                arguments,
                coercion,
            } => Self::Least {
                arguments: arguments.iter().map(SemanticArgument::from).collect(),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::ScalarSubquery(statement) => {
                Self::ScalarSubquery(Box::new(SemanticStatement::from(statement.as_ref())))
            }
            crate::TypedExpressionKind::Row(values) => {
                Self::Row(values.iter().map(SemanticExpression::from).collect())
            }
            crate::TypedExpressionKind::Array { elements, coercion } => Self::Array {
                elements: elements.iter().map(SemanticArgument::from).collect(),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::CteColumn {
                cte_id,
                binding,
                field_id,
            } => Self::CteColumn {
                cte_id: *cte_id,
                binding: *binding,
                field_id: *field_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticArgument {
    expression: SemanticExpression,
    coercion: Option<crate::TypedCoercion>,
}

impl From<&crate::TypedArgument> for SemanticArgument {
    fn from(argument: &crate::TypedArgument) -> Self {
        Self {
            expression: SemanticExpression::from(&argument.expression),
            coercion: argument.coercion.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticCaseBranch {
    when: SemanticExpression,
    then: SemanticArgument,
}

impl From<&crate::TypedCaseBranch> for SemanticCaseBranch {
    fn from(branch: &crate::TypedCaseBranch) -> Self {
        Self {
            when: SemanticExpression::from(&branch.when),
            then: SemanticArgument::from(&branch.then),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticOrderBy {
    expression: SemanticExpression,
    direction: crate::SortDirection,
    nulls: crate::NullsOrder,
}

impl From<&crate::TypedOrderBy> for SemanticOrderBy {
    fn from(order: &crate::TypedOrderBy) -> Self {
        Self {
            expression: SemanticExpression::from(&order.expression),
            direction: order.direction,
            nulls: order.nulls,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticWithinGroupOrderBy {
    argument: SemanticArgument,
    direction: crate::SortDirection,
    nulls: crate::NullsOrder,
}

impl From<&crate::TypedWithinGroupOrderBy> for SemanticWithinGroupOrderBy {
    fn from(order: &crate::TypedWithinGroupOrderBy) -> Self {
        Self {
            argument: SemanticArgument::from(&order.expression),
            direction: order.direction,
            nulls: order.nulls,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticInsert {
    ctes: Vec<SemanticCte>,
    target: dibs_pg_catalog::TableId,
    target_binding: crate::RelationId,
    columns: Vec<dibs_pg_catalog::ColumnId>,
    source: SemanticInsertSource,
    conflict: Option<SemanticConflictClause>,
    returning: Vec<SemanticProjection>,
}

impl From<&crate::TypedInsert> for SemanticInsert {
    fn from(insert: &crate::TypedInsert) -> Self {
        Self {
            ctes: insert.ctes.iter().map(SemanticCte::from).collect(),
            target: insert.target.clone(),
            target_binding: insert.target_binding,
            columns: insert.columns.clone(),
            source: SemanticInsertSource::from(&insert.source),
            conflict: insert.conflict.as_ref().map(SemanticConflictClause::from),
            returning: insert
                .returning
                .iter()
                .map(SemanticProjection::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticInsertSource {
    Values {
        rows: Vec<Vec<SemanticArgument>>,
        columns: Vec<crate::TypedValuesColumn>,
    },
    Select(Box<SemanticStatement>),
    DefaultValues,
}

impl From<&crate::TypedInsertSource> for SemanticInsertSource {
    fn from(source: &crate::TypedInsertSource) -> Self {
        match source {
            crate::TypedInsertSource::Values(values) => Self::Values {
                rows: values
                    .rows()
                    .iter()
                    .map(|row| row.iter().map(SemanticArgument::from).collect())
                    .collect(),
                columns: values.columns().to_vec(),
            },
            crate::TypedInsertSource::Select(statement) => {
                Self::Select(Box::new(SemanticStatement::from(statement.as_ref())))
            }
            crate::TypedInsertSource::DefaultValues => Self::DefaultValues,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticConflictClause {
    target: SemanticConflictTarget,
    action: SemanticConflictAction,
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticConflictTarget {
    Constraint(dibs_pg_catalog::ConstraintId),
    Inference {
        expressions: Vec<SemanticExpression>,
        predicate: Option<Box<SemanticExpression>>,
    },
    Unspecified,
}

impl From<&crate::TypedConflictClause> for SemanticConflictClause {
    fn from(conflict: &crate::TypedConflictClause) -> Self {
        let target = match &conflict.target {
            crate::ConflictTarget::Constraint(constraint) => {
                SemanticConflictTarget::Constraint(constraint.clone())
            }
            crate::ConflictTarget::Inference {
                expressions,
                predicate,
            } => SemanticConflictTarget::Inference {
                expressions: expressions.iter().map(SemanticExpression::from).collect(),
                predicate: predicate
                    .as_ref()
                    .map(|value| Box::new(SemanticExpression::from(value.as_ref()))),
            },
            crate::ConflictTarget::Unspecified => SemanticConflictTarget::Unspecified,
        };
        Self {
            target,
            action: SemanticConflictAction::from(&conflict.action),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
enum SemanticConflictAction {
    Nothing,
    Update {
        assignments: Vec<SemanticAssignment>,
        predicate: Option<Box<SemanticExpression>>,
    },
}

impl From<&crate::TypedConflictAction> for SemanticConflictAction {
    fn from(action: &crate::TypedConflictAction) -> Self {
        match action {
            crate::TypedConflictAction::Nothing => Self::Nothing,
            crate::TypedConflictAction::Update {
                assignments,
                predicate,
            } => Self::Update {
                assignments: assignments.iter().map(SemanticAssignment::from).collect(),
                predicate: predicate
                    .as_ref()
                    .map(|value| Box::new(SemanticExpression::from(value.as_ref()))),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticUpdate {
    ctes: Vec<SemanticCte>,
    target: dibs_pg_catalog::TableId,
    target_binding: crate::RelationId,
    assignments: Vec<SemanticAssignment>,
    from: Vec<SemanticRelation>,
    predicate: Option<SemanticExpression>,
    returning: Vec<SemanticProjection>,
}

impl From<&crate::TypedUpdate> for SemanticUpdate {
    fn from(update: &crate::TypedUpdate) -> Self {
        Self {
            ctes: update.ctes.iter().map(SemanticCte::from).collect(),
            target: update.target.clone(),
            target_binding: update.target_binding,
            assignments: update
                .assignments
                .iter()
                .map(SemanticAssignment::from)
                .collect(),
            from: update.from.iter().map(SemanticRelation::from).collect(),
            predicate: update.predicate.as_ref().map(SemanticExpression::from),
            returning: update
                .returning
                .iter()
                .map(SemanticProjection::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticDelete {
    ctes: Vec<SemanticCte>,
    target: dibs_pg_catalog::TableId,
    target_binding: crate::RelationId,
    using_relations: Vec<SemanticRelation>,
    predicate: Option<SemanticExpression>,
    returning: Vec<SemanticProjection>,
}

impl From<&crate::TypedDelete> for SemanticDelete {
    fn from(delete: &crate::TypedDelete) -> Self {
        Self {
            ctes: delete.ctes.iter().map(SemanticCte::from).collect(),
            target: delete.target.clone(),
            target_binding: delete.target_binding,
            using_relations: delete
                .using_relations
                .iter()
                .map(SemanticRelation::from)
                .collect(),
            predicate: delete.predicate.as_ref().map(SemanticExpression::from),
            returning: delete
                .returning
                .iter()
                .map(SemanticProjection::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticAssignment {
    id: crate::AssignmentId,
    target: dibs_pg_catalog::ColumnId,
    value: SemanticExpression,
    coercion: Option<crate::TypedCoercion>,
}

impl From<&crate::TypedAssignment> for SemanticAssignment {
    fn from(assignment: &crate::TypedAssignment) -> Self {
        Self {
            id: assignment.id,
            target: assignment.target.clone(),
            value: SemanticExpression::from(&assignment.value),
            coercion: assignment.coercion.clone(),
        }
    }
}
