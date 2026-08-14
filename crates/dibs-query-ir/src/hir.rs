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
    /// Whether the WITH list is explicitly recursive.
    pub recursive: bool,
    /// CTEs in authored order.
    pub ctes: Vec<HirCte>,
    /// SELECT duplicate-elimination policy.
    pub distinct: SelectDistinct<HirExpression>,
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
    /// Named WINDOW definitions in authored order.
    pub windows: Vec<HirNamedWindow>,
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
    /// Whether this CTE is self-recursive within a `WITH RECURSIVE` list.
    pub recursive: bool,
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

/// Authored relation alias retained for deterministic SQL rendering.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct RelationAlias {
    /// Alias identifier.
    pub name: String,
    /// Optional table-function/derived-table column alias list in authored order.
    pub column_names: Vec<String>,
}

/// Resolved relation binding with complete recursive topology.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirRelation {
    /// Revision-local binding identity.
    pub id: RelationId,
    /// Relation source origin.
    pub origin: SourceOrigin,
    /// Optional authored alias; never intrinsic object identity.
    pub alias: Option<RelationAlias>,
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
    /// Output field projected by a derived relation.
    DerivedColumn {
        /// Derived relation binding.
        binding: RelationId,
        /// Projected field identity.
        field_id: FieldId,
    },
    /// Resolved function call with all PostgreSQL call modifiers.
    Call(Box<HirCall>),
    /// Resolved unary/binary/postfix operator.
    Operator {
        /// Stable catalog operator identity.
        operator_id: OperatorId,
        /// Operands in semantic order.
        operands: Vec<HirExpression>,
    },
    /// Authored scalar comparison quantified over an array expression.
    QuantifiedComparison {
        /// Stable authored operator identity awaiting semantic selection.
        operator_id: OperatorId,
        /// Scalar left operand.
        left: Box<HirExpression>,
        /// Array-valued right operand.
        right: Box<HirExpression>,
        /// PostgreSQL comparison quantifier.
        quantifier: ComparisonQuantifier,
    },
    /// Authored `[NOT] IN` value-list predicate.
    InList {
        /// Scalar expression evaluated once.
        expression: Box<HirExpression>,
        /// Non-empty values in authored order.
        values: Vec<HirExpression>,
        /// Whether the predicate was authored as `NOT IN`.
        negated: bool,
    },
    /// Explicit or implicit cast node.
    Cast {
        /// Stable catalog cast identity.
        cast_id: CastId,
        /// Source expression.
        expression: Box<HirExpression>,
    },
    /// Authored explicit cast awaiting semantic source-type resolution.
    ExplicitCast {
        /// Authored target type resolved to a stable catalog identity.
        target_type: TypeId,
        /// Authored target typmod, when present.
        target_typmod: Option<crate::Typmod>,
        /// Source expression whose type is established by semantic checking.
        expression: Box<HirExpression>,
    },
    /// Explicit collation.
    Collate {
        /// Stable catalog collation identity.
        collation_id: CollationId,
        /// Source expression.
        expression: Box<HirExpression>,
    },
    /// Boolean existence test over a nested statement.
    Exists(Box<HirStatement>),
    /// `CASE` expression.
    Case {
        /// Optional simple-case operand.
        operand: Option<Box<HirExpression>>,
        /// Ordered branches.
        branches: Vec<HirCaseBranch>,
        /// Optional ELSE expression.
        else_expression: Option<Box<HirExpression>>,
    },
    /// Ordered `COALESCE` arguments. PostgreSQL evaluates each argument at most once.
    Coalesce(Vec<HirExpression>),
    /// PostgreSQL `NULLIF(left, right)` special form.
    NullIf {
        /// Left operand, evaluated once and returned when unequal.
        left: Box<HirExpression>,
        /// Right comparison operand, evaluated once.
        right: Box<HirExpression>,
    },
    /// Ordered `GREATEST` arguments.
    Greatest(Vec<HirExpression>),
    /// Ordered `LEAST` arguments.
    Least(Vec<HirExpression>),
    /// PostgreSQL `EXTRACT(field FROM source)` special form.
    Extract {
        /// Validated extraction field.
        field: ExtractField,
        /// Temporal source expression.
        source: Box<HirExpression>,
    },
    /// PostgreSQL `POSITION(substring IN string)` special form.
    Position {
        /// Substring to search for.
        substring: Box<HirExpression>,
        /// String or byte sequence to search within.
        string: Box<HirExpression>,
    },
    /// Scalar subquery.
    ScalarSubquery(Box<HirStatement>),
    /// Row constructor.
    Row(Vec<HirExpression>),
    /// Array constructor.
    Array(Vec<HirExpression>),
    /// CTE output reference through one exact relation use.
    CteColumn {
        /// Stable local CTE identity.
        cte_id: CteId,
        /// Exact relation binding used by this reference.
        binding: RelationId,
        /// Projected CTE field identity.
        field_id: FieldId,
    },
}

/// PostgreSQL scalar-array comparison quantifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ComparisonQuantifier {
    /// Succeeds when the comparison is true for at least one array element.
    Any,
    /// Succeeds when the comparison is true for every array element.
    All,
}

/// Closed PostgreSQL `EXTRACT` field vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ExtractField {
    /// Calendar century.
    Century,
    /// Day of month or interval day count.
    Day,
    /// Calendar decade.
    Decade,
    /// Sunday-based day of week.
    Dow,
    /// Day of year.
    Doy,
    /// Seconds from the PostgreSQL epoch convention.
    Epoch,
    /// Hour component.
    Hour,
    /// ISO Monday-based day of week.
    IsoDow,
    /// ISO week-numbering year.
    IsoYear,
    /// Julian date.
    Julian,
    /// Microsecond component.
    Microseconds,
    /// Calendar millennium.
    Millennium,
    /// Millisecond component.
    Milliseconds,
    /// Minute component.
    Minute,
    /// Month component.
    Month,
    /// Calendar quarter.
    Quarter,
    /// Second component.
    Second,
    /// Time-zone offset in seconds.
    Timezone,
    /// Time-zone hour component.
    TimezoneHour,
    /// Time-zone minute component.
    TimezoneMinute,
    /// ISO week number.
    Week,
    /// Calendar year.
    Year,
}

impl ExtractField {
    /// Returns PostgreSQL's canonical uppercase spelling.
    #[must_use]
    pub const fn sql(self) -> &'static str {
        match self {
            Self::Century => "CENTURY",
            Self::Day => "DAY",
            Self::Decade => "DECADE",
            Self::Dow => "DOW",
            Self::Doy => "DOY",
            Self::Epoch => "EPOCH",
            Self::Hour => "HOUR",
            Self::IsoDow => "ISODOW",
            Self::IsoYear => "ISOYEAR",
            Self::Julian => "JULIAN",
            Self::Microseconds => "MICROSECONDS",
            Self::Millennium => "MILLENNIUM",
            Self::Milliseconds => "MILLISECONDS",
            Self::Minute => "MINUTE",
            Self::Month => "MONTH",
            Self::Quarter => "QUARTER",
            Self::Second => "SECOND",
            Self::Timezone => "TIMEZONE",
            Self::TimezoneHour => "TIMEZONE_HOUR",
            Self::TimezoneMinute => "TIMEZONE_MINUTE",
            Self::Week => "WEEK",
            Self::Year => "YEAR",
        }
    }
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
    /// PostgreSQL interval literal with optional field and precision qualifiers.
    Interval {
        /// Decoded interval string value.
        value: String,
        /// Optional leading interval field.
        field: Option<String>,
        /// Optional trailing field after `TO`.
        to_field: Option<String>,
        /// Optional authored precision.
        precision: Option<String>,
    },
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
pub type HirOrderBy = OrderBy<HirExpression>;

/// SELECT duplicate-elimination policy.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum SelectDistinct<E> {
    /// Preserve all input rows.
    AllRows,
    /// Eliminate duplicate projected rows.
    Distinct,
    /// Keep the first row for each authored DISTINCT ON key tuple.
    On(Vec<E>),
}

/// One resolved function call with aggregate and window modifiers.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirCall {
    /// Stable catalog callable identity.
    pub callable_id: CallableId,
    /// Arguments in authored semantic order.
    pub arguments: Vec<HirExpression>,
    /// Authored argument names parallel to `arguments`.
    pub argument_names: Vec<Option<String>>,
    /// Whether the argument tuple is duplicate-eliminated.
    pub distinct: bool,
    /// Whether the call uses the `*` argument form.
    pub star: bool,
    /// Aggregate-local ORDER BY terms.
    pub order_by: Vec<HirOrderBy>,
    /// Optional aggregate FILTER predicate.
    pub filter: Option<Box<HirExpression>>,
    /// Ordered-set aggregate WITHIN GROUP ordering.
    pub within_group: Vec<HirOrderBy>,
    /// Optional window application.
    pub over: Option<WindowReference<HirExpression>>,
}

/// One named WINDOW definition.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct HirNamedWindow {
    /// Window identifier.
    pub name: String,
    /// Exact definition origin.
    pub origin: SourceOrigin,
    /// Window specification.
    pub specification: WindowSpec<HirExpression>,
}

/// Named or inline OVER-clause window reference.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum WindowReference<E> {
    /// Reference a WINDOW definition by name.
    Named(String),
    /// Use an inline parenthesized specification.
    Inline(WindowSpec<E>),
}

/// PostgreSQL window specification.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct WindowSpec<E> {
    /// Optional inherited named window.
    pub existing: Option<String>,
    /// PARTITION BY expressions in authored order.
    pub partition_by: Vec<E>,
    /// ORDER BY terms in authored order.
    pub order_by: Vec<OrderBy<E>>,
    /// Optional frame clause.
    pub frame: Option<WindowFrame<E>>,
}

/// Generic authored ordering term shared by resolved and typed window vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct OrderBy<E> {
    /// Ordering expression.
    pub expression: E,
    /// Sort direction.
    pub direction: SortDirection,
    /// Null ordering.
    pub nulls: NullsOrder,
}

/// PostgreSQL window frame clause.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct WindowFrame<E> {
    /// Frame unit.
    pub mode: WindowFrameMode,
    /// Starting bound.
    pub start: FrameBound<E>,
    /// Optional ending bound for BETWEEN form.
    pub end: Option<FrameBound<E>>,
    /// Row exclusion policy.
    pub exclusion: WindowExclusion,
}

/// Window frame unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum WindowFrameMode {
    /// Physical rows.
    Rows,
    /// Ordering-peer range.
    Range,
    /// Ordering peer groups.
    Groups,
}

/// One window frame boundary.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum FrameBound<E> {
    /// UNBOUNDED PRECEDING.
    UnboundedPreceding,
    /// Expression PRECEDING.
    Preceding(E),
    /// CURRENT ROW.
    CurrentRow,
    /// Expression FOLLOWING.
    Following(E),
    /// UNBOUNDED FOLLOWING.
    UnboundedFollowing,
}

/// Window frame exclusion policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum WindowExclusion {
    /// No explicit EXCLUDE clause.
    None,
    /// EXCLUDE CURRENT ROW.
    CurrentRow,
    /// EXCLUDE GROUP.
    Group,
    /// EXCLUDE TIES.
    Ties,
    /// EXCLUDE NO OTHERS.
    NoOthers,
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
    /// Revision-local binding for target-column expressions, conflict actions, and RETURNING.
    pub target_binding: RelationId,
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
    /// Synthetic `EXCLUDED` relation binding available to conflict actions.
    pub excluded_binding: RelationId,
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
