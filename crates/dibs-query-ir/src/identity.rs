use dibs_pg_catalog::{SchemaFingerprint, TypeId};

use crate::{
    Parameter, ParameterId, QueryManifest, ReadWriteLockManifest, ReferenceIndex, ResultMode,
    TypedStatement, Typmod,
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
        Self {
            version: input.version,
            postgres_major: input.postgres_major,
            statement: SemanticStatement::from(&input.statement),
            parameters: input.parameters.clone(),
            result_mode: input.result_mode,
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
    parameters: Vec<PublicParameter>,
    output_fields: Vec<PublicOutputField>,
    result_mode: ResultMode,
    transport_envelope: Option<String>,
}

impl From<&PublicIdentityInput> for CanonicalPublicIdentityInput {
    fn from(input: &PublicIdentityInput) -> Self {
        Self {
            version: input.version,
            query_name: input.query_name.clone(),
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
    api_types: Vec<crate::ApiTypeMapping>,
    sensitivity: crate::Sensitivity,
}

impl From<&Parameter> for PublicParameter {
    fn from(parameter: &Parameter) -> Self {
        let mut api_types = parameter.api_types.clone();
        api_types.sort();
        api_types.dedup();
        Self {
            ordinal: parameter.ordinal,
            source_name: parameter.source_name.clone(),
            type_id: parameter.type_id.clone(),
            typmod: parameter.typmod.clone(),
            nullable: parameter.nullable,
            pg_codec_id: parameter.pg_codec_id.clone(),
            wire_codec_id: parameter.wire_codec_id.clone(),
            bind_format: parameter.bind_format,
            api_types,
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
    ctes: Vec<SemanticCte>,
    projections: Vec<SemanticProjection>,
    from: Vec<SemanticRelation>,
    predicate: Option<SemanticExpression>,
    group_by: Vec<SemanticExpression>,
    having: Option<SemanticExpression>,
    order_by: Vec<SemanticOrderBy>,
    limit: Option<crate::TypedLimit>,
    offset: Option<crate::TypedLimit>,
    locks: Vec<crate::HirLockClause>,
}

impl From<&crate::TypedSelect> for SemanticSelect {
    fn from(select: &crate::TypedSelect) -> Self {
        Self {
            ctes: select.ctes.iter().map(SemanticCte::from).collect(),
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
            order_by: select.order_by.iter().map(SemanticOrderBy::from).collect(),
            limit: select.limit.clone(),
            offset: select.offset.clone(),
            locks: select.locks.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticCte {
    id: crate::CteId,
    materialization: crate::CteMaterialization,
    statement: Box<SemanticStatement>,
    output_fields: Vec<crate::FieldId>,
}

impl From<&crate::TypedCte> for SemanticCte {
    fn from(cte: &crate::TypedCte) -> Self {
        Self {
            id: cte.id,
            materialization: cte.materialization,
            statement: Box::new(SemanticStatement::from(cte.statement.as_ref())),
            output_fields: cte.output_fields.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticProjection {
    field_id: crate::FieldId,
    expression: SemanticExpression,
}

impl From<&crate::TypedProjection> for SemanticProjection {
    fn from(projection: &crate::TypedProjection) -> Self {
        Self {
            field_id: projection.field_id,
            expression: SemanticExpression::from(&projection.expression),
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
        rows: Vec<Vec<SemanticExpression>>,
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
                    .iter()
                    .map(|row| row.iter().map(SemanticExpression::from).collect())
                    .collect(),
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
    Call {
        callable_id: dibs_pg_catalog::CallableId,
        arguments: Vec<SemanticExpression>,
        coercions: Vec<Option<crate::TypedCoercion>>,
    },
    Operator {
        operator_id: dibs_pg_catalog::OperatorId,
        operands: Vec<SemanticExpression>,
        coercions: Vec<Option<crate::TypedCoercion>>,
    },
    Cast {
        cast_id: dibs_pg_catalog::CastId,
        expression: Box<SemanticExpression>,
        coercion: crate::TypedCoercion,
    },
    Collate {
        collation_id: dibs_pg_catalog::CollationId,
        expression: Box<SemanticExpression>,
    },
    Case {
        operand: Option<Box<SemanticExpression>>,
        branches: Vec<SemanticCaseBranch>,
        else_expression: Option<Box<SemanticExpression>>,
        result_coercion: crate::CoercionEvidence,
    },
    ScalarSubquery(Box<SemanticStatement>),
    Row(Vec<SemanticExpression>),
    Array {
        elements: Vec<SemanticExpression>,
        coercion: crate::CoercionEvidence,
    },
    CteColumn {
        cte_id: crate::CteId,
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
            crate::TypedExpressionKind::Call {
                callable_id,
                arguments,
                coercions,
            } => Self::Call {
                callable_id: callable_id.clone(),
                arguments: arguments.iter().map(SemanticExpression::from).collect(),
                coercions: coercions.clone(),
            },
            crate::TypedExpressionKind::Operator {
                operator_id,
                operands,
                coercions,
            } => Self::Operator {
                operator_id: operator_id.clone(),
                operands: operands.iter().map(SemanticExpression::from).collect(),
                coercions: coercions.clone(),
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
            crate::TypedExpressionKind::Collate {
                collation_id,
                expression,
            } => Self::Collate {
                collation_id: collation_id.clone(),
                expression: Box::new(SemanticExpression::from(expression.as_ref())),
            },
            crate::TypedExpressionKind::Case {
                operand,
                branches,
                else_expression,
                result_coercion,
            } => Self::Case {
                operand: operand
                    .as_ref()
                    .map(|value| Box::new(SemanticExpression::from(value.as_ref()))),
                branches: branches.iter().map(SemanticCaseBranch::from).collect(),
                else_expression: else_expression
                    .as_ref()
                    .map(|value| Box::new(SemanticExpression::from(value.as_ref()))),
                result_coercion: result_coercion.clone(),
            },
            crate::TypedExpressionKind::ScalarSubquery(statement) => {
                Self::ScalarSubquery(Box::new(SemanticStatement::from(statement.as_ref())))
            }
            crate::TypedExpressionKind::Row(values) => {
                Self::Row(values.iter().map(SemanticExpression::from).collect())
            }
            crate::TypedExpressionKind::Array { elements, coercion } => Self::Array {
                elements: elements.iter().map(SemanticExpression::from).collect(),
                coercion: coercion.clone(),
            },
            crate::TypedExpressionKind::CteColumn { cte_id, field_id } => Self::CteColumn {
                cte_id: *cte_id,
                field_id: *field_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticCaseBranch {
    when: SemanticExpression,
    then: SemanticExpression,
}

impl From<&crate::TypedCaseBranch> for SemanticCaseBranch {
    fn from(branch: &crate::TypedCaseBranch) -> Self {
        Self {
            when: SemanticExpression::from(&branch.when),
            then: SemanticExpression::from(&branch.then),
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
struct SemanticInsert {
    ctes: Vec<SemanticCte>,
    target: dibs_pg_catalog::TableId,
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
    Values(Vec<Vec<SemanticExpression>>),
    Select(Box<SemanticStatement>),
    DefaultValues,
}

impl From<&crate::TypedInsertSource> for SemanticInsertSource {
    fn from(source: &crate::TypedInsertSource) -> Self {
        match source {
            crate::TypedInsertSource::Values(rows) => Self::Values(
                rows.iter()
                    .map(|row| row.iter().map(SemanticExpression::from).collect())
                    .collect(),
            ),
            crate::TypedInsertSource::Select(statement) => {
                Self::Select(Box::new(SemanticStatement::from(statement.as_ref())))
            }
            crate::TypedInsertSource::DefaultValues => Self::DefaultValues,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
struct SemanticConflictClause {
    target: Vec<SemanticExpression>,
    predicate: Option<SemanticExpression>,
    action: SemanticConflictAction,
}

impl From<&crate::TypedConflictClause> for SemanticConflictClause {
    fn from(conflict: &crate::TypedConflictClause) -> Self {
        Self {
            target: conflict
                .target
                .iter()
                .map(SemanticExpression::from)
                .collect(),
            predicate: conflict.predicate.as_ref().map(SemanticExpression::from),
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
