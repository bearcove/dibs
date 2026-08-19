use dibs_pg_catalog::{CallableId, CastId, CollationId, ColumnId, OperatorId, TableId, TypeId};

use crate::{
    AssignmentId, Cardinality, CteId, CteMaterialization, ExpressionId, FieldId, HirLiteral,
    HirLockClause, Nullability, ParameterId, RelationId, SourceOrigin, StatementId,
};

/// PostgreSQL typmod retained as canonical semantic spelling.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(transparent)]
pub struct Typmod(String);

impl Typmod {
    /// Creates a canonical typmod descriptor.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the canonical typmod descriptor.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// PostgreSQL expression volatility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum Volatility {
    /// Immutable across all rows and statements for equal inputs.
    Immutable,
    /// Stable within one statement.
    Stable,
    /// May change within one statement or cause effects.
    Volatile,
}

/// Coercion context matching PostgreSQL semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum CoercionContext {
    /// Implicit expression coercion.
    Implicit,
    /// Assignment-only coercion.
    Assignment,
    /// Explicit cast.
    Explicit,
}

/// Why a typed expression has its final PostgreSQL type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum CoercionEvidence {
    /// Source already has the exact target type.
    Exact,
    /// Stable catalog cast was selected.
    CatalogCast {
        /// Resolved cast identity.
        cast_id: CastId,
        /// Resolution context.
        context: CoercionContext,
    },
    /// Domain was flattened to its base during candidate selection.
    DomainBase {
        /// Original domain identity.
        domain: TypeId,
        /// Canonical base identity.
        base: TypeId,
    },
    /// Unknown literal was resolved by expression context.
    UnknownLiteral {
        /// Final resolved type.
        resolved: TypeId,
    },
    /// PostgreSQL common-type selection for CASE/VALUES/set/array forms.
    CommonType {
        /// Final common type.
        resolved: TypeId,
        /// Candidate input types in semantic order.
        inputs: Vec<TypeId>,
    },
    /// Polymorphic family was bound to concrete types.
    Polymorphic {
        /// Stable callable identity providing the family.
        callable_id: CallableId,
        /// Concrete bound types in parameter order.
        bound_types: Vec<TypeId>,
    },
}

/// One explicit typed coercion node.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedCoercion {
    /// Source type.
    pub source_type: TypeId,
    /// Target type.
    pub target_type: TypeId,
    /// Target typmod.
    pub target_typmod: Option<Typmod>,
    /// Resolution proof.
    pub evidence: CoercionEvidence,
}

/// Typed statement wrapper with proof-bearing cardinality.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedStatement {
    /// Revision-local statement identity.
    pub id: StatementId,
    /// Source origin.
    pub origin: SourceOrigin,
    /// Inferred relation cardinality.
    pub cardinality: Cardinality,
    /// Fully typed topology.
    pub kind: TypedStatementKind,
}

/// Fully typed PostgreSQL statement topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedStatementKind {
    /// Typed `SELECT`/`VALUES` statement.
    Select(Box<TypedSelect>),
    /// Typed `INSERT` statement.
    Insert(Box<TypedInsert>),
    /// Typed `UPDATE` statement.
    Update(Box<TypedUpdate>),
    /// Typed `DELETE` statement.
    Delete(Box<TypedDelete>),
}

/// Typed `SELECT` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedSelect {
    /// CTEs in authored order.
    pub ctes: Vec<TypedCte>,
    /// Output projections in semantic order.
    pub projections: Vec<TypedProjection>,
    /// Source relations in semantic order.
    pub from: Vec<TypedRelation>,
    /// Optional Boolean predicate.
    pub predicate: Option<TypedExpression>,
    /// Grouping expressions in authored order.
    pub group_by: Vec<TypedExpression>,
    /// Optional Boolean HAVING expression.
    pub having: Option<TypedExpression>,
    /// Ordering terms in authored order.
    pub order_by: Vec<TypedOrderBy>,
    /// Optional typed limit.
    pub limit: Option<TypedLimit>,
    /// Optional typed offset.
    pub offset: Option<TypedLimit>,
    /// Lock clauses attached to this exact select node.
    pub locks: Vec<HirLockClause>,
}

/// Typed CTE descriptor.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedCte {
    /// Revision-local CTE identity.
    pub id: CteId,
    /// Materialization policy.
    pub materialization: CteMaterialization,
    /// Typed statement.
    pub statement: Box<TypedStatement>,
    /// Output field identities in order.
    pub output_fields: Vec<FieldId>,
}

/// Typed projection.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedProjection {
    /// Stable local output field identity.
    pub field_id: FieldId,
    /// Typed expression.
    pub expression: TypedExpression,
}

/// Typed relation.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedRelation {
    /// Revision-local relation identity.
    pub id: RelationId,
    /// Source origin.
    pub origin: SourceOrigin,
    /// Inferred cardinality.
    pub cardinality: Cardinality,
    /// Relation topology.
    pub kind: TypedRelationKind,
}

/// Typed relation vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedRelationKind {
    /// Stable catalog table.
    Table {
        /// Table identity.
        table_id: TableId,
    },
    /// CTE use.
    Cte {
        /// CTE identity.
        cte_id: CteId,
    },
    /// Subquery relation.
    Subquery(Box<TypedStatement>),
    /// Table function.
    Function {
        /// Stable callable identity.
        callable_id: CallableId,
        /// Typed arguments.
        arguments: Vec<TypedExpression>,
    },
    /// Join retaining exact input topology.
    Join {
        /// Join kind.
        kind: JoinKind,
        /// Left input.
        left: Box<TypedRelation>,
        /// Right input.
        right: Box<TypedRelation>,
        /// Optional join predicate.
        predicate: Option<Box<TypedExpression>>,
        /// Whether the right side is lateral.
        lateral: bool,
    },
    /// VALUES relation.
    Values {
        /// Rows and fields in authored order.
        rows: Vec<Vec<TypedExpression>>,
    },
    /// Set operation.
    SetOperation {
        /// Operation kind.
        kind: SetOperationKind,
        /// Whether duplicates are retained.
        all: bool,
        /// Left input.
        left: Box<TypedStatement>,
        /// Right input.
        right: Box<TypedStatement>,
    },
}

/// SQL join kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum JoinKind {
    /// Inner join.
    Inner,
    /// Left outer join.
    Left,
    /// Right outer join.
    Right,
    /// Full outer join.
    Full,
    /// Cross join.
    Cross,
}

/// SQL set operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum SetOperationKind {
    /// `UNION`.
    Union,
    /// `INTERSECT`.
    Intersect,
    /// `EXCEPT`.
    Except,
}

/// Fully typed scalar expression.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedExpression {
    /// Revision-local expression identity.
    pub id: ExpressionId,
    /// Source origin.
    pub origin: SourceOrigin,
    /// Stable resolved PostgreSQL type identity.
    pub type_id: TypeId,
    /// Resolved typmod.
    pub typmod: Option<Typmod>,
    /// Conservative proof-bearing nullability.
    pub nullability: Nullability,
    /// PostgreSQL volatility.
    pub volatility: Volatility,
    /// Typed expression topology.
    pub kind: TypedExpressionKind,
}

/// Fully typed expression vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedExpressionKind {
    /// Typed literal.
    Literal(HirLiteral),
    /// Typed parameter.
    Parameter(ParameterId),
    /// Typed column reference.
    Column {
        /// Relation binding.
        binding: RelationId,
        /// Stable column identity.
        column_id: ColumnId,
    },
    /// Typed function call.
    Call {
        /// Stable callable identity.
        callable_id: CallableId,
        /// Typed arguments in semantic order.
        arguments: Vec<TypedExpression>,
        /// Per-argument coercions in matching order.
        coercions: Vec<Option<TypedCoercion>>,
    },
    /// Typed operator application.
    Operator {
        /// Stable operator identity.
        operator_id: OperatorId,
        /// Typed operands in semantic order.
        operands: Vec<TypedExpression>,
        /// Per-operand coercions in matching order.
        coercions: Vec<Option<TypedCoercion>>,
    },
    /// Explicit typed cast.
    Cast {
        /// Stable cast identity.
        cast_id: CastId,
        /// Typed source expression.
        expression: Box<TypedExpression>,
        /// Cast proof.
        coercion: TypedCoercion,
    },
    /// Explicit collation.
    Collate {
        /// Stable collation identity.
        collation_id: CollationId,
        /// Typed source expression.
        expression: Box<TypedExpression>,
    },
    /// Typed `CASE`.
    Case {
        /// Optional simple-case operand.
        operand: Option<Box<TypedExpression>>,
        /// Ordered branches.
        branches: Vec<TypedCaseBranch>,
        /// Optional ELSE expression.
        else_expression: Option<Box<TypedExpression>>,
        /// Common result coercion proof.
        result_coercion: CoercionEvidence,
    },
    /// Scalar subquery.
    ScalarSubquery(Box<TypedStatement>),
    /// Row constructor.
    Row(Vec<TypedExpression>),
    /// Array constructor with common-element proof.
    Array {
        /// Elements in authored order.
        elements: Vec<TypedExpression>,
        /// Common element coercion proof.
        coercion: CoercionEvidence,
    },
    /// Typed CTE field.
    CteColumn {
        /// CTE identity.
        cte_id: CteId,
        /// Output field identity.
        field_id: FieldId,
    },
}

/// One typed CASE branch.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedCaseBranch {
    /// Boolean/match expression.
    pub when: TypedExpression,
    /// Result expression.
    pub then: TypedExpression,
}

/// One typed ordering term.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedOrderBy {
    /// Typed ordering expression.
    pub expression: TypedExpression,
    /// Direction.
    pub direction: crate::SortDirection,
    /// Null ordering.
    pub nulls: crate::NullsOrder,
}

/// Constant or parameter-driven LIMIT/OFFSET.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedLimit {
    /// Constant non-negative row count.
    Constant(u64),
    /// Declared parameter whose runtime value supplies the bound.
    Parameter(ParameterId),
}

/// Typed `INSERT` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedInsert {
    /// CTEs in authored order.
    pub ctes: Vec<TypedCte>,
    /// Stable target table.
    pub target: TableId,
    /// Target columns in semantic order.
    pub columns: Vec<ColumnId>,
    /// Insert source.
    pub source: TypedInsertSource,
    /// Optional typed conflict handling.
    pub conflict: Option<TypedConflictClause>,
    /// Ordered typed RETURNING projection.
    pub returning: Vec<TypedProjection>,
}

/// Typed insert source.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedInsertSource {
    /// Typed VALUES rows.
    Values(Vec<Vec<TypedExpression>>),
    /// Typed query source.
    Select(Box<TypedStatement>),
    /// DEFAULT VALUES.
    DefaultValues,
}

/// Typed conflict clause.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedConflictClause {
    /// Resolved target expressions.
    pub target: Vec<TypedExpression>,
    /// Optional predicate.
    pub predicate: Option<TypedExpression>,
    /// Action.
    pub action: TypedConflictAction,
}

/// Typed conflict action.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedConflictAction {
    /// DO NOTHING.
    Nothing,
    /// DO UPDATE.
    Update {
        /// Assignments in authored order.
        assignments: Vec<TypedAssignment>,
        /// Optional action predicate.
        predicate: Option<Box<TypedExpression>>,
    },
}

/// Typed `UPDATE` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedUpdate {
    /// CTEs in authored order.
    pub ctes: Vec<TypedCte>,
    /// Stable target table.
    pub target: TableId,
    /// Target binding.
    pub target_binding: RelationId,
    /// Typed assignments.
    pub assignments: Vec<TypedAssignment>,
    /// FROM relations.
    pub from: Vec<TypedRelation>,
    /// Optional predicate.
    pub predicate: Option<TypedExpression>,
    /// RETURNING projection.
    pub returning: Vec<TypedProjection>,
}

/// Typed `DELETE` topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedDelete {
    /// CTEs in authored order.
    pub ctes: Vec<TypedCte>,
    /// Stable target table.
    pub target: TableId,
    /// Target binding.
    pub target_binding: RelationId,
    /// USING relations.
    pub using_relations: Vec<TypedRelation>,
    /// Optional predicate.
    pub predicate: Option<TypedExpression>,
    /// RETURNING projection.
    pub returning: Vec<TypedProjection>,
}

/// Typed mutation assignment.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedAssignment {
    /// Revision-local assignment identity.
    pub id: AssignmentId,
    /// Stable target column identity.
    pub target: ColumnId,
    /// Typed value.
    pub value: TypedExpression,
    /// Assignment coercion proof.
    pub coercion: Option<TypedCoercion>,
}
