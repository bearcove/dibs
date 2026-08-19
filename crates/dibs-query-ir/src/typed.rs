use dibs_pg_catalog::{
    CallableId, CastId, CollationId, ColumnId, ConstraintId, OperatorId, TableId, TypeId,
};

use crate::{
    AssignmentId, Cardinality, CteId, CteMaterialization, ExpressionId, FieldId, HirAssignment,
    HirCaseBranch, HirConflictAction, HirConflictClause, HirConflictTarget, HirCte, HirDelete,
    HirExpression, HirExpressionKind, HirInsert, HirInsertSource, HirLiteral, HirLockClause,
    HirOrderBy, HirProjection, HirRelation, HirRelationKind, HirSelect, HirStatement,
    HirStatementKind, HirUpdate, Nullability, ParameterId, RelationId, SourceOrigin, StatementId,
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
#[facet(invariants = TypedStatement::is_valid)]
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

impl TypedStatement {
    /// Validates all proof and shape invariants recursively.
    pub fn validate(&self) -> Result<(), TypedShapeError> {
        self.cardinality
            .validate()
            .map_err(|_| TypedShapeError::Cardinality)?;
        match &self.kind {
            TypedStatementKind::Select(select) => select.validate(),
            TypedStatementKind::Insert(insert) => insert.validate(),
            TypedStatementKind::Update(update) => update.validate(),
            TypedStatementKind::Delete(delete) => delete.validate(),
        }
    }
    /// Returns whether this typed statement is a total lowering of the resolved HIR statement.
    #[must_use]
    pub fn corresponds_to_hir(&self, hir: &HirStatement) -> bool {
        self.id == hir.id && typed_statement_kind_corresponds(&self.kind, &hir.kind)
    }

    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

/// Fully typed PostgreSQL statement topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedStatementKind {
    /// Typed `SELECT`/`VALUES` statement.
    Select(Box<TypedSelect>),
    /// `INSERT` statement.
    Insert(Box<TypedInsert>),
    /// `UPDATE` statement.
    Update(Box<TypedUpdate>),
    /// `DELETE` statement.
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

impl TypedSelect {
    fn validate(&self) -> Result<(), TypedShapeError> {
        validate_ctes(&self.ctes)?;
        validate_projections(&self.projections)?;
        for relation in &self.from {
            relation.validate()?;
        }
        validate_expression_option(self.predicate.as_ref())?;
        for expression in &self.group_by {
            expression.validate()?;
        }
        validate_expression_option(self.having.as_ref())?;
        for order in &self.order_by {
            order.expression.validate()?;
        }
        Ok(())
    }
}

/// Typed CTE descriptor whose output IDs exactly match its statement projection arity and order.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = TypedCte::is_valid)]
pub struct TypedCte {
    /// Revision-local CTE identity.
    pub id: CteId,
    /// Materialization policy.
    pub materialization: CteMaterialization,
    /// Typed statement.
    pub statement: Box<TypedStatement>,
    /// Output field identities in order.
    output_fields: Vec<FieldId>,
}

impl TypedCte {
    /// Creates a CTE after checking output field identity and arity.
    pub fn try_new(
        id: CteId,
        materialization: CteMaterialization,
        statement: Box<TypedStatement>,
        output_fields: Vec<FieldId>,
    ) -> Result<Self, TypedShapeError> {
        let value = Self {
            id,
            materialization,
            statement,
            output_fields,
        };
        value
            .is_valid()
            .then_some(value)
            .ok_or(TypedShapeError::CteOutput)
    }

    /// Returns output field identities in projection order.
    #[must_use]
    pub fn output_fields(&self) -> &[FieldId] {
        &self.output_fields
    }

    fn is_valid(&self) -> bool {
        statement_projection_fields(&self.statement).as_slice() == self.output_fields
            && self.statement.validate().is_ok()
    }
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

impl TypedRelation {
    fn validate(&self) -> Result<(), TypedShapeError> {
        self.cardinality
            .validate()
            .map_err(|_| TypedShapeError::Cardinality)?;
        match &self.kind {
            TypedRelationKind::Table { .. } | TypedRelationKind::Cte { .. } => Ok(()),
            TypedRelationKind::Subquery(statement) => statement.validate(),
            TypedRelationKind::Function { arguments, .. } => {
                for argument in arguments {
                    argument.validate()?;
                }
                Ok(())
            }
            TypedRelationKind::Join {
                left,
                right,
                predicate,
                ..
            } => {
                left.validate()?;
                right.validate()?;
                if let Some(predicate) = predicate {
                    predicate.validate()?;
                }
                Ok(())
            }
            TypedRelationKind::Values { rows } => rows.validate(),
            TypedRelationKind::SetOperation { left, right, .. } => {
                left.validate()?;
                right.validate()
            }
        }
    }
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
        /// Validated non-empty rectangular rows.
        rows: TypedValues,
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
#[facet(invariants = TypedExpression::is_valid)]
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

impl TypedExpression {
    /// Validates nullability and nested expression/statement shapes.
    pub fn validate(&self) -> Result<(), TypedShapeError> {
        self.nullability
            .validate()
            .map_err(|_| TypedShapeError::Nullability)?;
        match &self.kind {
            TypedExpressionKind::Literal(_)
            | TypedExpressionKind::Parameter(_)
            | TypedExpressionKind::Column { .. }
            | TypedExpressionKind::CteColumn { .. } => Ok(()),
            TypedExpressionKind::Call { arguments, .. } => validate_arguments(arguments),
            TypedExpressionKind::Operator { operands, .. } => validate_arguments(operands),
            TypedExpressionKind::Cast { expression, .. }
            | TypedExpressionKind::Collate { expression, .. } => expression.validate(),
            TypedExpressionKind::Case {
                operand,
                branches,
                else_expression,
                ..
            } => {
                if let Some(operand) = operand {
                    operand.validate()?;
                }
                for branch in branches {
                    branch.when.validate()?;
                    branch.then.validate()?;
                }
                if let Some(else_expression) = else_expression {
                    else_expression.validate()?;
                }
                Ok(())
            }
            TypedExpressionKind::ScalarSubquery(statement) => statement.validate(),
            TypedExpressionKind::Row(values) => {
                for value in values {
                    value.validate()?;
                }
                Ok(())
            }
            TypedExpressionKind::Array { elements, .. } => {
                for element in elements {
                    element.validate()?;
                }
                Ok(())
            }
        }
    }

    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
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
        /// Paired typed arguments and their optional coercions.
        arguments: Vec<TypedArgument>,
    },
    /// Typed operator application.
    Operator {
        /// Stable operator identity.
        operator_id: OperatorId,
        /// Paired typed operands and their optional coercions.
        operands: Vec<TypedArgument>,
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

/// Typed expression paired with the coercion applied at its use site.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TypedArgument {
    /// Typed expression.
    pub expression: TypedExpression,
    /// Optional coercion applied at this use site.
    pub coercion: Option<TypedCoercion>,
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

/// Validated non-empty rectangular typed VALUES rows.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = TypedValues::is_valid)]
pub struct TypedValues {
    rows: Vec<Vec<TypedExpression>>,
}

impl TypedValues {
    /// Creates non-empty rectangular typed VALUES rows.
    pub fn try_new(rows: Vec<Vec<TypedExpression>>) -> Result<Self, TypedShapeError> {
        let value = Self { rows };
        value
            .is_valid()
            .then_some(value)
            .ok_or(TypedShapeError::Values)
    }

    /// Returns typed rows in authored order.
    #[must_use]
    pub fn rows(&self) -> &[Vec<TypedExpression>] {
        &self.rows
    }

    fn validate(&self) -> Result<(), TypedShapeError> {
        if !self.is_rectangular() {
            return Err(TypedShapeError::Values);
        }
        for row in &self.rows {
            for expression in row {
                expression.validate()?;
            }
        }
        Ok(())
    }

    fn is_rectangular(&self) -> bool {
        let Some(first) = self.rows.first() else {
            return false;
        };
        !first.is_empty() && self.rows.iter().all(|row| row.len() == first.len())
    }

    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
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

impl TypedInsert {
    fn validate(&self) -> Result<(), TypedShapeError> {
        validate_ctes(&self.ctes)?;
        match &self.source {
            TypedInsertSource::Values(values) => values.validate()?,
            TypedInsertSource::Select(statement) => statement.validate()?,
            TypedInsertSource::DefaultValues => {}
        }
        if let Some(conflict) = &self.conflict {
            conflict.validate()?;
        }
        validate_projections(&self.returning)
    }
}

/// Typed insert source.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum TypedInsertSource {
    /// Typed VALUES rows.
    Values(TypedValues),
    /// Typed query source.
    Select(Box<TypedStatement>),
    /// DEFAULT VALUES.
    DefaultValues,
}

/// Typed conflict clause.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = TypedConflictClause::is_valid)]
pub struct TypedConflictClause {
    /// Mutually exclusive PostgreSQL conflict target.
    pub target: ConflictTarget,
    /// Action.
    pub action: TypedConflictAction,
}

impl TypedConflictClause {
    /// Validates PostgreSQL conflict-target/action shape and nested typed expressions.
    pub fn validate(&self) -> Result<(), TypedShapeError> {
        match &self.target {
            ConflictTarget::Inference {
                expressions,
                predicate,
            } => {
                if expressions.is_empty() {
                    return Err(TypedShapeError::Conflict);
                }
                for expression in expressions {
                    expression.validate()?;
                }
                if let Some(predicate) = predicate {
                    predicate.validate()?;
                }
            }
            ConflictTarget::Unspecified
                if matches!(self.action, TypedConflictAction::Update { .. }) =>
            {
                return Err(TypedShapeError::Conflict);
            }
            ConflictTarget::Constraint(_) | ConflictTarget::Unspecified => {}
        }
        if let TypedConflictAction::Update {
            assignments,
            predicate,
        } = &self.action
        {
            if assignments.is_empty() {
                return Err(TypedShapeError::Conflict);
            }
            for assignment in assignments {
                assignment.value.validate()?;
            }
            if let Some(predicate) = predicate {
                predicate.validate()?;
            }
        }
        Ok(())
    }

    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

/// Typed mutually exclusive PostgreSQL conflict target.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum ConflictTarget {
    /// Named `ON CONSTRAINT` target.
    Constraint(ConstraintId),
    /// Inferred expression target with optional predicate.
    Inference {
        /// Ordered target expressions.
        expressions: Vec<TypedExpression>,
        /// Optional target predicate.
        predicate: Option<Box<TypedExpression>>,
    },
    /// No explicit target, valid for `DO NOTHING`.
    Unspecified,
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

impl TypedUpdate {
    fn validate(&self) -> Result<(), TypedShapeError> {
        validate_ctes(&self.ctes)?;
        for assignment in &self.assignments {
            assignment.value.validate()?;
        }
        for relation in &self.from {
            relation.validate()?;
        }
        validate_expression_option(self.predicate.as_ref())?;
        validate_projections(&self.returning)
    }
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

impl TypedDelete {
    fn validate(&self) -> Result<(), TypedShapeError> {
        validate_ctes(&self.ctes)?;
        for relation in &self.using_relations {
            relation.validate()?;
        }
        validate_expression_option(self.predicate.as_ref())?;
        validate_projections(&self.returning)
    }
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

/// Typed topology shape error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedShapeError {
    /// Empty or ragged VALUES rows.
    Values,
    /// CTE output identities differ from nested statement projections.
    CteOutput,
    /// Contradictory cardinality proof.
    Cardinality,
    /// Non-null claim without positive proof.
    Nullability,
    /// Invalid PostgreSQL conflict target/action combination.
    Conflict,
}

impl std::fmt::Display for TypedShapeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Values => {
                formatter.write_str("VALUES must contain non-empty rows with equal arity")
            }
            Self::CteOutput => {
                formatter.write_str("CTE output fields must equal statement projections")
            }
            Self::Cardinality => formatter.write_str("typed node has contradictory cardinality"),
            Self::Nullability => {
                formatter.write_str("typed expression has invalid nullability proof")
            }
            Self::Conflict => {
                formatter.write_str("invalid PostgreSQL conflict target/action shape")
            }
        }
    }
}

impl std::error::Error for TypedShapeError {}

fn statement_projection_fields(statement: &TypedStatement) -> Vec<FieldId> {
    let projections = match &statement.kind {
        TypedStatementKind::Select(select) => &select.projections,
        TypedStatementKind::Insert(insert) => &insert.returning,
        TypedStatementKind::Update(update) => &update.returning,
        TypedStatementKind::Delete(delete) => &delete.returning,
    };
    projections
        .iter()
        .map(|projection| projection.field_id)
        .collect()
}

fn validate_arguments(arguments: &[TypedArgument]) -> Result<(), TypedShapeError> {
    for argument in arguments {
        argument.expression.validate()?;
    }
    Ok(())
}

fn validate_ctes(ctes: &[TypedCte]) -> Result<(), TypedShapeError> {
    for cte in ctes {
        if !cte.is_valid() {
            return Err(TypedShapeError::CteOutput);
        }
    }
    Ok(())
}

fn validate_projections(projections: &[TypedProjection]) -> Result<(), TypedShapeError> {
    for projection in projections {
        projection.expression.validate()?;
    }
    Ok(())
}

fn validate_expression_option(expression: Option<&TypedExpression>) -> Result<(), TypedShapeError> {
    if let Some(expression) = expression {
        expression.validate()?;
    }
    Ok(())
}

fn typed_statement_kind_corresponds(typed: &TypedStatementKind, hir: &HirStatementKind) -> bool {
    match (typed, hir) {
        (TypedStatementKind::Select(typed), HirStatementKind::Select(hir)) => {
            typed_select_corresponds(typed, hir)
        }
        (TypedStatementKind::Insert(typed), HirStatementKind::Insert(hir)) => {
            typed_insert_corresponds(typed, hir)
        }
        (TypedStatementKind::Update(typed), HirStatementKind::Update(hir)) => {
            typed_update_corresponds(typed, hir)
        }
        (TypedStatementKind::Delete(typed), HirStatementKind::Delete(hir)) => {
            typed_delete_corresponds(typed, hir)
        }
        _ => false,
    }
}

fn typed_select_corresponds(typed: &TypedSelect, hir: &HirSelect) -> bool {
    typed_ctes_correspond(&typed.ctes, &hir.ctes)
        && typed_projections_correspond(&typed.projections, &hir.projections)
        && typed_relations_correspond(&typed.from, &hir.from)
        && typed_expression_option_corresponds(typed.predicate.as_ref(), hir.predicate.as_ref())
        && typed_expressions_correspond(&typed.group_by, &hir.group_by)
        && typed_expression_option_corresponds(typed.having.as_ref(), hir.having.as_ref())
        && typed_ordering_corresponds(&typed.order_by, &hir.order_by)
        && typed_limit_corresponds(typed.limit.as_ref(), hir.limit.as_ref())
        && typed_limit_corresponds(typed.offset.as_ref(), hir.offset.as_ref())
        && typed.locks == hir.locks
}

fn typed_ctes_correspond(typed: &[TypedCte], hir: &[HirCte]) -> bool {
    typed.len() == hir.len()
        && typed.iter().zip(hir).all(|(typed, hir)| {
            typed.id == hir.id
                && typed.materialization == hir.materialization
                && typed.statement.corresponds_to_hir(&hir.statement)
        })
}

fn typed_projections_correspond(typed: &[TypedProjection], hir: &[HirProjection]) -> bool {
    typed.len() == hir.len()
        && typed.iter().zip(hir).all(|(typed, hir)| {
            typed.field_id == hir.field_id
                && typed_expression_corresponds(&typed.expression, &hir.expression)
        })
}

fn typed_relations_correspond(typed: &[TypedRelation], hir: &[HirRelation]) -> bool {
    typed.len() == hir.len()
        && typed
            .iter()
            .zip(hir)
            .all(|(typed, hir)| typed_relation_corresponds(typed, hir))
}

fn typed_relation_corresponds(typed: &TypedRelation, hir: &HirRelation) -> bool {
    typed.id == hir.id
        && match (&typed.kind, &hir.kind) {
            (
                TypedRelationKind::Table { table_id: typed },
                HirRelationKind::Table { table_id: hir },
            ) => typed == hir,
            (TypedRelationKind::Cte { cte_id: typed }, HirRelationKind::Cte { cte_id: hir }) => {
                typed == hir
            }
            (TypedRelationKind::Subquery(typed), HirRelationKind::Subquery(hir)) => {
                typed.corresponds_to_hir(hir)
            }
            (
                TypedRelationKind::Function {
                    callable_id: typed_callable,
                    arguments: typed_arguments,
                },
                HirRelationKind::Function {
                    callable_id: hir_callable,
                    arguments: hir_arguments,
                },
            ) => {
                typed_callable == hir_callable
                    && typed_expressions_correspond(typed_arguments, hir_arguments)
            }
            (
                TypedRelationKind::Join {
                    kind: typed_kind,
                    left: typed_left,
                    right: typed_right,
                    predicate: typed_predicate,
                    lateral: typed_lateral,
                },
                HirRelationKind::Join {
                    kind: hir_kind,
                    left: hir_left,
                    right: hir_right,
                    predicate: hir_predicate,
                    lateral: hir_lateral,
                },
            ) => {
                typed_kind == hir_kind
                    && typed_relation_corresponds(typed_left, hir_left)
                    && typed_relation_corresponds(typed_right, hir_right)
                    && typed_boxed_expression_option_corresponds(
                        typed_predicate.as_deref(),
                        hir_predicate.as_deref(),
                    )
                    && typed_lateral == hir_lateral
            }
            (TypedRelationKind::Values { rows: typed }, HirRelationKind::Values { rows: hir }) => {
                typed_value_rows_correspond(typed.rows(), hir.rows())
            }
            (
                TypedRelationKind::SetOperation {
                    kind: typed_kind,
                    all: typed_all,
                    left: typed_left,
                    right: typed_right,
                },
                HirRelationKind::SetOperation {
                    kind: hir_kind,
                    all: hir_all,
                    left: hir_left,
                    right: hir_right,
                },
            ) => {
                typed_kind == hir_kind
                    && typed_all == hir_all
                    && typed_left.corresponds_to_hir(hir_left)
                    && typed_right.corresponds_to_hir(hir_right)
            }
            _ => false,
        }
}

fn typed_expression_corresponds(typed: &TypedExpression, hir: &HirExpression) -> bool {
    typed.id == hir.id
        && match (&typed.kind, &hir.kind) {
            (TypedExpressionKind::Literal(typed), HirExpressionKind::Literal(hir)) => typed == hir,
            (TypedExpressionKind::Parameter(typed), HirExpressionKind::Parameter(hir)) => {
                typed == hir
            }
            (
                TypedExpressionKind::Column {
                    binding: typed_binding,
                    column_id: typed_column,
                },
                HirExpressionKind::Column {
                    binding: hir_binding,
                    column_id: hir_column,
                },
            ) => typed_binding == hir_binding && typed_column == hir_column,
            (
                TypedExpressionKind::Call {
                    callable_id: typed_callable,
                    arguments: typed_arguments,
                },
                HirExpressionKind::Call {
                    callable_id: hir_callable,
                    arguments: hir_arguments,
                },
            ) => {
                typed_callable == hir_callable
                    && typed_arguments.len() == hir_arguments.len()
                    && typed_arguments
                        .iter()
                        .zip(hir_arguments)
                        .all(|(typed, hir)| typed_expression_corresponds(&typed.expression, hir))
            }
            (
                TypedExpressionKind::Operator {
                    operator_id: typed_operator,
                    operands: typed_operands,
                },
                HirExpressionKind::Operator {
                    operator_id: hir_operator,
                    operands: hir_operands,
                },
            ) => {
                typed_operator == hir_operator
                    && typed_operands.len() == hir_operands.len()
                    && typed_operands
                        .iter()
                        .zip(hir_operands)
                        .all(|(typed, hir)| typed_expression_corresponds(&typed.expression, hir))
            }
            (
                TypedExpressionKind::Cast {
                    cast_id: typed_cast,
                    expression: typed_expression,
                    ..
                },
                HirExpressionKind::Cast {
                    cast_id: hir_cast,
                    expression: hir_expression,
                },
            ) => {
                typed_cast == hir_cast
                    && typed_expression_corresponds(typed_expression, hir_expression)
            }
            (
                TypedExpressionKind::Collate {
                    collation_id: typed_collation,
                    expression: typed_expression,
                },
                HirExpressionKind::Collate {
                    collation_id: hir_collation,
                    expression: hir_expression,
                },
            ) => {
                typed_collation == hir_collation
                    && typed_expression_corresponds(typed_expression, hir_expression)
            }
            (
                TypedExpressionKind::Case {
                    operand: typed_operand,
                    branches: typed_branches,
                    else_expression: typed_else,
                    ..
                },
                HirExpressionKind::Case {
                    operand: hir_operand,
                    branches: hir_branches,
                    else_expression: hir_else,
                },
            ) => {
                typed_boxed_expression_option_corresponds(
                    typed_operand.as_deref(),
                    hir_operand.as_deref(),
                ) && typed_case_branches_correspond(typed_branches, hir_branches)
                    && typed_boxed_expression_option_corresponds(
                        typed_else.as_deref(),
                        hir_else.as_deref(),
                    )
            }
            (
                TypedExpressionKind::ScalarSubquery(typed),
                HirExpressionKind::ScalarSubquery(hir),
            ) => typed.corresponds_to_hir(hir),
            (TypedExpressionKind::Row(typed), HirExpressionKind::Row(hir)) => {
                typed_expressions_correspond(typed, hir)
            }
            (
                TypedExpressionKind::Array {
                    elements: typed, ..
                },
                HirExpressionKind::Array(hir),
            ) => typed_expressions_correspond(typed, hir),
            (
                TypedExpressionKind::CteColumn {
                    cte_id: typed_cte,
                    field_id: typed_field,
                },
                HirExpressionKind::CteColumn {
                    cte_id: hir_cte,
                    field_id: hir_field,
                },
            ) => typed_cte == hir_cte && typed_field == hir_field,
            _ => false,
        }
}

fn typed_case_branches_correspond(typed: &[TypedCaseBranch], hir: &[HirCaseBranch]) -> bool {
    typed.len() == hir.len()
        && typed.iter().zip(hir).all(|(typed, hir)| {
            typed_expression_corresponds(&typed.when, &hir.when)
                && typed_expression_corresponds(&typed.then, &hir.then)
        })
}

fn typed_expressions_correspond(typed: &[TypedExpression], hir: &[HirExpression]) -> bool {
    typed.len() == hir.len()
        && typed
            .iter()
            .zip(hir)
            .all(|(typed, hir)| typed_expression_corresponds(typed, hir))
}

fn typed_expression_option_corresponds(
    typed: Option<&TypedExpression>,
    hir: Option<&HirExpression>,
) -> bool {
    match (typed, hir) {
        (Some(typed), Some(hir)) => typed_expression_corresponds(typed, hir),
        (None, None) => true,
        _ => false,
    }
}

fn typed_boxed_expression_option_corresponds(
    typed: Option<&TypedExpression>,
    hir: Option<&HirExpression>,
) -> bool {
    typed_expression_option_corresponds(typed, hir)
}

fn typed_ordering_corresponds(typed: &[TypedOrderBy], hir: &[HirOrderBy]) -> bool {
    typed.len() == hir.len()
        && typed.iter().zip(hir).all(|(typed, hir)| {
            typed.direction == hir.direction
                && typed.nulls == hir.nulls
                && typed_expression_corresponds(&typed.expression, &hir.expression)
        })
}

fn typed_limit_corresponds(typed: Option<&TypedLimit>, hir: Option<&HirExpression>) -> bool {
    match (typed, hir) {
        (None, None) => true,
        (Some(TypedLimit::Parameter(typed)), Some(hir)) => {
            matches!(hir.kind, HirExpressionKind::Parameter(hir) if *typed == hir)
        }
        (Some(TypedLimit::Constant(typed)), Some(hir)) => {
            matches!(&hir.kind, HirExpressionKind::Literal(HirLiteral::Integer(hir)) if hir.parse::<u64>().ok() == Some(*typed))
        }
        _ => false,
    }
}

fn typed_value_rows_correspond(typed: &[Vec<TypedExpression>], hir: &[Vec<HirExpression>]) -> bool {
    typed.len() == hir.len()
        && typed
            .iter()
            .zip(hir)
            .all(|(typed, hir)| typed_expressions_correspond(typed, hir))
}

fn typed_insert_corresponds(typed: &TypedInsert, hir: &HirInsert) -> bool {
    typed_ctes_correspond(&typed.ctes, &hir.ctes)
        && typed.target == hir.target
        && typed.columns == hir.columns
        && typed_insert_source_corresponds(&typed.source, &hir.source)
        && typed_conflict_option_corresponds(typed.conflict.as_ref(), hir.conflict.as_ref())
        && typed_projections_correspond(&typed.returning, &hir.returning)
}

fn typed_insert_source_corresponds(typed: &TypedInsertSource, hir: &HirInsertSource) -> bool {
    match (typed, hir) {
        (TypedInsertSource::Values(typed), HirInsertSource::Values(hir)) => {
            typed_value_rows_correspond(typed.rows(), hir.rows())
        }
        (TypedInsertSource::Select(typed), HirInsertSource::Select(hir)) => {
            typed.corresponds_to_hir(hir)
        }
        (TypedInsertSource::DefaultValues, HirInsertSource::DefaultValues) => true,
        _ => false,
    }
}

fn typed_conflict_option_corresponds(
    typed: Option<&TypedConflictClause>,
    hir: Option<&HirConflictClause>,
) -> bool {
    match (typed, hir) {
        (Some(typed), Some(hir)) => typed_conflict_corresponds(typed, hir),
        (None, None) => true,
        _ => false,
    }
}

fn typed_conflict_corresponds(typed: &TypedConflictClause, hir: &HirConflictClause) -> bool {
    typed_conflict_target_corresponds(&typed.target, &hir.target)
        && typed_conflict_action_corresponds(&typed.action, &hir.action)
}

fn typed_conflict_target_corresponds(typed: &ConflictTarget, hir: &HirConflictTarget) -> bool {
    match (typed, hir) {
        (ConflictTarget::Constraint(typed), HirConflictTarget::Constraint(hir)) => typed == hir,
        (
            ConflictTarget::Inference {
                expressions: typed_expressions,
                predicate: typed_predicate,
            },
            HirConflictTarget::Inference {
                expressions: hir_expressions,
                predicate: hir_predicate,
            },
        ) => {
            typed_expressions_correspond(typed_expressions, hir_expressions)
                && typed_boxed_expression_option_corresponds(
                    typed_predicate.as_deref(),
                    hir_predicate.as_deref(),
                )
        }
        (ConflictTarget::Unspecified, HirConflictTarget::Unspecified) => true,
        _ => false,
    }
}

fn typed_conflict_action_corresponds(typed: &TypedConflictAction, hir: &HirConflictAction) -> bool {
    match (typed, hir) {
        (TypedConflictAction::Nothing, HirConflictAction::Nothing) => true,
        (
            TypedConflictAction::Update {
                assignments: typed_assignments,
                predicate: typed_predicate,
            },
            HirConflictAction::Update {
                assignments: hir_assignments,
                predicate: hir_predicate,
            },
        ) => {
            typed_assignments_correspond(typed_assignments, hir_assignments)
                && typed_expression_option_corresponds(
                    typed_predicate.as_deref(),
                    hir_predicate.as_ref(),
                )
        }
        _ => false,
    }
}

fn typed_assignments_correspond(typed: &[TypedAssignment], hir: &[HirAssignment]) -> bool {
    typed.len() == hir.len()
        && typed.iter().zip(hir).all(|(typed, hir)| {
            typed.id == hir.id
                && typed.target == hir.target
                && typed_expression_corresponds(&typed.value, &hir.value)
        })
}

fn typed_update_corresponds(typed: &TypedUpdate, hir: &HirUpdate) -> bool {
    typed_ctes_correspond(&typed.ctes, &hir.ctes)
        && typed.target == hir.target
        && typed.target_binding == hir.target_binding
        && typed_assignments_correspond(&typed.assignments, &hir.assignments)
        && typed_relations_correspond(&typed.from, &hir.from)
        && typed_expression_option_corresponds(typed.predicate.as_ref(), hir.predicate.as_ref())
        && typed_projections_correspond(&typed.returning, &hir.returning)
}

fn typed_delete_corresponds(typed: &TypedDelete, hir: &HirDelete) -> bool {
    typed_ctes_correspond(&typed.ctes, &hir.ctes)
        && typed.target == hir.target
        && typed.target_binding == hir.target_binding
        && typed_relations_correspond(&typed.using_relations, &hir.using_relations)
        && typed_expression_option_corresponds(typed.predicate.as_ref(), hir.predicate.as_ref())
        && typed_projections_correspond(&typed.returning, &hir.returning)
}
