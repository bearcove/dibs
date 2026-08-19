use dibs_pg_catalog::{
    CallableId, CastId, CollationId, ColumnId, ConstraintId, IndexId, OperatorId, TableId, TypeId,
};

use crate::{
    AssignmentId, CteId, ExpressionId, FieldId, ParameterId, QueryId, RelationId, SourceOrigin,
    StatementId,
};

/// Fully resolved query declaration with no unresolved semantic names.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirQuery {
    /// Revision-local query identity.
    pub id: QueryId,
    /// Public declaration name.
    pub name: String,
    /// Full declaration origin.
    pub origin: SourceOrigin,
    /// Parameters in declaration order.
    pub parameters: Vec<HirParameter>,
    /// Exactly one resolved PostgreSQL statement.
    pub statement: HirStatement,
}

/// Resolved query parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirParameter {
    /// Revision-local parameter identity.
    pub id: ParameterId,
    /// Zero-based declaration ordinal.
    pub ordinal: u32,
    /// Authored source name.
    pub name: String,
    /// Exact source origin.
    pub origin: SourceOrigin,
    /// Stable catalog type identity.
    pub type_id: TypeId,
    /// PostgreSQL typmod spelling, when declared.
    pub typmod: Option<crate::Typmod>,
    /// Whether the bind accepts SQL `NULL`.
    pub nullable: bool,
}

/// Resolved statement wrapper.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirStatement {
    /// Revision-local statement identity.
    pub id: StatementId,
    /// Statement source origin.
    pub origin: SourceOrigin,
    /// Resolved statement topology.
    pub kind: HirStatementKind,
}

/// Resolved PostgreSQL statement topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum HirStatementKind {
    /// Row-producing `SELECT`/`VALUES` statement.
    Select(Box<HirSelect>),
    /// `INSERT` statement.
    Insert(Box<HirInsert>),
    /// `UPDATE` statement.
    Update(Box<HirUpdate>),
    /// `DELETE` statement.
    Delete(Box<HirDelete>),
}

/// Resolved `SELECT` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirSelect {
    /// CTEs in authored order.
    pub ctes: Vec<HirCte>,
    /// Projection fields in authored output order.
    pub projections: Vec<HirProjection>,
    /// Source relations in authored semantic order.
    pub from: Vec<HirRelation>,
    /// Optional predicate.
    pub predicate: Option<HirExpression>,
    /// Grouping expressions in authored order.
    pub group_by: Vec<HirExpression>,
    /// Optional HAVING expression.
    pub having: Option<HirExpression>,
    /// Ordering terms in authored order.
    pub order_by: Vec<HirOrderBy>,
    /// Optional limit expression.
    pub limit: Option<HirExpression>,
    /// Optional offset expression.
    pub offset: Option<HirExpression>,
    /// Lock clauses in authored order.
    pub locks: Vec<HirLockClause>,
}

/// Resolved CTE.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirCte {
    /// Revision-local CTE identity.
    pub id: CteId,
    /// Authored CTE name retained only as binding presentation.
    pub name: String,
    /// Exact source origin.
    pub origin: SourceOrigin,
    /// Materialization policy.
    pub materialization: CteMaterialization,
    /// CTE statement.
    pub statement: Box<HirStatement>,
}

/// PostgreSQL CTE materialization modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum CteMaterialization {
    /// No explicit modifier.
    Default,
    /// `MATERIALIZED`.
    Materialized,
    /// `NOT MATERIALIZED`.
    NotMaterialized,
}

/// One resolved projection.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirProjection {
    /// Revision-local output field identity.
    pub field_id: FieldId,
    /// Exact output label.
    pub alias: String,
    /// Alias source origin, excluded from execution identity.
    pub alias_origin: SourceOrigin,
    /// Resolved source expression.
    pub expression: HirExpression,
}

/// Resolved relation binding with complete recursive topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirRelation {
    /// Revision-local binding identity.
    pub id: RelationId,
    /// Relation source origin.
    pub origin: SourceOrigin,
    /// Optional authored alias; never intrinsic object identity.
    pub alias: Option<String>,
    /// Resolved relation topology.
    pub kind: HirRelationKind,
}

/// Complete resolved relation vocabulary consumed by typed lowering.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum HirRelationKind {
    /// Stable catalog table.
    Table {
        /// Stable table identity.
        table_id: TableId,
    },
    /// CTE use.
    Cte {
        /// Revision-local CTE identity.
        cte_id: CteId,
    },
    /// Derived table/subquery.
    Subquery(Box<HirStatement>),
    /// Resolved table-function call.
    Function {
        /// Stable callable identity.
        callable_id: CallableId,
        /// Arguments in authored semantic order.
        arguments: Vec<HirExpression>,
    },
    /// Join retaining exact input topology.
    Join {
        /// Join kind.
        kind: crate::JoinKind,
        /// Left relation.
        left: Box<HirRelation>,
        /// Right relation.
        right: Box<HirRelation>,
        /// Optional join predicate.
        predicate: Option<Box<HirExpression>>,
        /// Whether the right relation is lateral.
        lateral: bool,
    },
    /// Rectangular VALUES relation.
    Values {
        /// Ordered rectangular rows.
        rows: HirValues,
    },
    /// Set-operation relation.
    SetOperation {
        /// Set-operation kind.
        kind: crate::SetOperationKind,
        /// Whether duplicates are retained.
        all: bool,
        /// Left input statement.
        left: Box<HirStatement>,
        /// Right input statement.
        right: Box<HirStatement>,
    },
}

/// Validated non-empty rectangular resolved VALUES rows.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = HirValues::is_valid)]
pub struct HirValues {
    rows: Vec<Vec<HirExpression>>,
}

impl HirValues {
    /// Creates non-empty rectangular VALUES rows.
    pub fn try_new(rows: Vec<Vec<HirExpression>>) -> Result<Self, ValuesShapeError> {
        let value = Self { rows };
        value.is_valid().then_some(value).ok_or(ValuesShapeError)
    }

    /// Returns rows in authored order.
    #[must_use]
    pub fn rows(&self) -> &[Vec<HirExpression>] {
        &self.rows
    }

    fn is_valid(&self) -> bool {
        rectangular_rows(&self.rows)
    }
}

/// Invalid empty or ragged VALUES shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValuesShapeError;

impl std::fmt::Display for ValuesShapeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VALUES must contain non-empty rows with equal arity")
    }
}

impl std::error::Error for ValuesShapeError {}

fn rectangular_rows<T>(rows: &[Vec<T>]) -> bool {
    let Some(first) = rows.first() else {
        return false;
    };
    !first.is_empty() && rows.iter().all(|row| row.len() == first.len())
}

/// Resolved scalar expression.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirExpression {
    /// Revision-local expression identity.
    pub id: ExpressionId,
    /// Expression source origin.
    pub origin: SourceOrigin,
    /// Resolved expression topology.
    pub kind: HirExpressionKind,
}

/// Resolved scalar-expression vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum HirExpressionKind {
    /// Literal retaining semantic value, not raw source text.
    Literal(HirLiteral),
    /// Declared bind parameter.
    Parameter(ParameterId),
    /// Column bound to a relation and stable catalog column.
    Column {
        /// Local binding identity.
        binding: RelationId,
        /// Stable catalog column identity.
        column_id: ColumnId,
    },
    /// Resolved function call.
    Call {
        /// Stable catalog callable identity.
        callable_id: CallableId,
        /// Arguments in authored semantic order.
        arguments: Vec<HirExpression>,
    },
    /// Resolved unary/binary/postfix operator.
    Operator {
        /// Stable catalog operator identity.
        operator_id: OperatorId,
        /// Operands in semantic order.
        operands: Vec<HirExpression>,
    },
    /// Explicit or implicit cast node.
    Cast {
        /// Stable catalog cast identity.
        cast_id: CastId,
        /// Source expression.
        expression: Box<HirExpression>,
    },
    /// Explicit collation.
    Collate {
        /// Stable catalog collation identity.
        collation_id: CollationId,
        /// Source expression.
        expression: Box<HirExpression>,
    },
    /// `CASE` expression.
    Case {
        /// Optional simple-case operand.
        operand: Option<Box<HirExpression>>,
        /// Ordered branches.
        branches: Vec<HirCaseBranch>,
        /// Optional ELSE expression.
        else_expression: Option<Box<HirExpression>>,
    },
    /// Scalar subquery.
    ScalarSubquery(Box<HirStatement>),
    /// Row constructor.
    Row(Vec<HirExpression>),
    /// Array constructor.
    Array(Vec<HirExpression>),
    /// CTE output reference.
    CteColumn {
        /// Stable local CTE identity.
        cte_id: CteId,
        /// Projected CTE field identity.
        field_id: FieldId,
    },
}

/// Semantic literal value retained by resolved and typed IR.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum HirLiteral {
    /// SQL `NULL`.
    Null,
    /// Boolean value.
    Boolean(bool),
    /// Integer spelling normalized without separators.
    Integer(String),
    /// Numeric/decimal spelling normalized without separators.
    Numeric(String),
    /// String literal decoded to its value.
    String(String),
    /// Byte string as exact bytes.
    Bytes(Vec<u8>),
}

/// One `CASE WHEN ... THEN ...` branch.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirCaseBranch {
    /// Branch predicate or match expression.
    pub when: HirExpression,
    /// Branch result.
    pub then: HirExpression,
}

/// One resolved ordering term.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirOrderBy {
    /// Ordering expression.
    pub expression: HirExpression,
    /// Sort direction.
    pub direction: SortDirection,
    /// Null ordering.
    pub nulls: NullsOrder,
}

/// SQL sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum SortDirection {
    /// Ascending.
    Ascending,
    /// Descending.
    Descending,
}

/// SQL null ordering policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum NullsOrder {
    /// PostgreSQL default for the direction.
    Default,
    /// `NULLS FIRST`.
    First,
    /// `NULLS LAST`.
    Last,
}

/// One resolved row-lock clause.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirLockClause {
    /// Lock strength.
    pub strength: LockStrength,
    /// Explicit local relation targets, in authored order.
    pub targets: Vec<RelationId>,
    /// Wait policy.
    pub wait: LockWaitPolicy,
}

/// PostgreSQL row-lock strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum LockStrength {
    /// `FOR UPDATE`.
    Update,
    /// `FOR NO KEY UPDATE`.
    NoKeyUpdate,
    /// `FOR SHARE`.
    Share,
    /// `FOR KEY SHARE`.
    KeyShare,
}

/// PostgreSQL lock wait policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum LockWaitPolicy {
    /// Ordinary blocking wait.
    Wait,
    /// `NOWAIT`.
    NoWait,
    /// `SKIP LOCKED`.
    SkipLocked,
}

/// Resolved `INSERT` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirInsert {
    /// CTEs in authored order.
    pub ctes: Vec<HirCte>,
    /// Stable target table identity.
    pub target: TableId,
    /// Ordered target columns.
    pub columns: Vec<ColumnId>,
    /// Insert source.
    pub source: HirInsertSource,
    /// Optional conflict handling.
    pub conflict: Option<HirConflictClause>,
    /// Ordered `RETURNING` projection.
    pub returning: Vec<HirProjection>,
}

/// Resolved insert source.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum HirInsertSource {
    /// Ordered rectangular rows and columns.
    Values(HirValues),
    /// Query source.
    Select(Box<HirStatement>),
    /// `DEFAULT VALUES`.
    DefaultValues,
}

/// Resolved conflict handling.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirConflictClause {
    /// Mutually exclusive PostgreSQL conflict target form.
    pub target: HirConflictTarget,
    /// Conflict action.
    pub action: HirConflictAction,
}

/// Resolved mutually exclusive PostgreSQL conflict target.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum HirConflictTarget {
    /// Named `ON CONSTRAINT` target.
    Constraint(ConstraintId),
    /// Inferred expression target with optional predicate.
    Inference {
        /// Ordered target expressions.
        expressions: Vec<HirExpression>,
        /// Optional target predicate.
        predicate: Option<Box<HirExpression>>,
    },
    /// No explicit target, valid for `DO NOTHING`.
    Unspecified,
}

/// Resolved conflict action.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum HirConflictAction {
    /// `DO NOTHING`.
    Nothing,
    /// `DO UPDATE`.
    Update {
        /// Ordered assignments.
        assignments: Vec<HirAssignment>,
        /// Optional action predicate.
        predicate: Option<HirExpression>,
    },
}

/// Resolved `UPDATE` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirUpdate {
    /// CTEs in authored order.
    pub ctes: Vec<HirCte>,
    /// Stable target table identity.
    pub target: TableId,
    /// Target relation binding.
    pub target_binding: RelationId,
    /// Assignments in authored order.
    pub assignments: Vec<HirAssignment>,
    /// `FROM` relations in authored order.
    pub from: Vec<HirRelation>,
    /// Optional predicate.
    pub predicate: Option<HirExpression>,
    /// Ordered `RETURNING` projection.
    pub returning: Vec<HirProjection>,
}

/// Resolved `DELETE` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirDelete {
    /// CTEs in authored order.
    pub ctes: Vec<HirCte>,
    /// Stable target table identity.
    pub target: TableId,
    /// Target relation binding.
    pub target_binding: RelationId,
    /// `USING` relations in authored order.
    pub using_relations: Vec<HirRelation>,
    /// Optional predicate.
    pub predicate: Option<HirExpression>,
    /// Ordered `RETURNING` projection.
    pub returning: Vec<HirProjection>,
}

/// One resolved mutation assignment.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirAssignment {
    /// Revision-local assignment identity.
    pub id: AssignmentId,
    /// Stable target catalog column.
    pub target: ColumnId,
    /// Assignment value.
    pub value: HirExpression,
}

/// Stable index use retained when a lock/order policy has catalog evidence.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirIndexEvidence {
    /// Stable index identity.
    pub index_id: IndexId,
    /// Exact source origin whose policy is justified.
    pub origin: SourceOrigin,
}
