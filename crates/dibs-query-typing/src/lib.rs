#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! PostgreSQL 18 semantic type checking and cardinality inference for Dibs HIR.
mod checker;
mod expression;
mod resolution;

use std::collections::{BTreeMap, BTreeSet};

use dibs_pg_catalog::{
    AggregateEmptyBehavior, CallableId, CallableKind, CastContext, CatalogCallable, CatalogCast,
    CatalogOperator, CatalogSnapshot, CatalogTable, CatalogType, ColumnId,
    Nullability as CatalogNullability, OperatorId, PgCodecId, PgTypeCategory, PgTypeKind,
    PolymorphicType, TableId, TypeId, Volatility as CatalogVolatility, WireCodecId,
};
use dibs_query_ir::{
    Cardinality, CardinalityEvidence, CoercionContext, CoercionEvidence, CteId, ExpressionId,
    FieldId, FrameBound, HirCall, HirCaseBranch, HirCte, HirExpression, HirExpressionKind,
    HirLiteral, HirNamedWindow, HirOrderBy, HirParameter, HirProjection, HirQuery, HirRelation,
    HirRelationKind, HirSelect, HirStatement, HirStatementKind, JoinKind, LowerBound, Nullability,
    NullabilityEvidence, ParameterId, RelationId, ResultMode, RuntimeAssertion, SelectDistinct,
    SetOperationKind, SourceOrigin, TypedArgument, TypedCall, TypedCaseBranch, TypedCastStep,
    TypedCte, TypedExpression, TypedExpressionKind, TypedLimit, TypedNamedWindow, TypedOrderBy,
    TypedProjection, TypedRelation, TypedRelationKind, TypedSelect, TypedStatement,
    TypedStatementKind, TypedValues, TypedValuesColumn, Typmod, UpperBound, Volatility,
    WindowFrame, WindowReference, WindowSpec,
};

/// Stable HIR identity for structural SQL `AND`.
pub const SYNTAX_AND_OPERATOR_ID: &str = "pg18:operator:syntax:AND";
/// Stable HIR identity for structural SQL `OR`.
pub const SYNTAX_OR_OPERATOR_ID: &str = "pg18:operator:syntax:OR";
/// Stable HIR identity for structural SQL `NOT`.
pub const SYNTAX_NOT_OPERATOR_ID: &str = "pg18:operator:syntax:NOT";
/// Stable HIR identity for structural SQL `IS NULL`.
pub const SYNTAX_IS_NULL_OPERATOR_ID: &str = "pg18:operator:syntax:IS NULL";
/// Stable HIR identity for structural SQL `IS NOT NULL`.
pub const SYNTAX_IS_NOT_NULL_OPERATOR_ID: &str = "pg18:operator:syntax:IS NOT NULL";
/// Stable HIR identity for structural SQL `IS DISTINCT FROM`.
pub const SYNTAX_IS_DISTINCT_FROM_OPERATOR_ID: &str = "pg18:operator:syntax:IS DISTINCT FROM";
/// Stable HIR identity for structural SQL `IS NOT DISTINCT FROM`.
pub const SYNTAX_IS_NOT_DISTINCT_FROM_OPERATOR_ID: &str =
    "pg18:operator:syntax:IS NOT DISTINCT FROM";

/// One checked parameter in HIR declaration order.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct CheckedParameter {
    /// Revision-local parameter identity.
    pub id: ParameterId,
    /// Zero-based declaration ordinal.
    pub ordinal: u32,
    /// Stable PostgreSQL type identity.
    pub type_id: TypeId,
    /// Resolved typmod.
    pub typmod: Option<Typmod>,
    /// Whether the bind accepts SQL NULL.
    pub nullable: bool,
    /// Catalog-owned PostgreSQL codec identity.
    pub pg_codec_id: PgCodecId,
    /// Catalog-owned wire codec identity.
    pub wire_codec_id: WireCodecId,
}

/// One checked output field in projection order.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct CheckedField {
    /// Revision-local output identity.
    pub id: FieldId,
    /// Zero-based output ordinal.
    pub ordinal: u32,
    /// Exact SQL output label.
    pub sql_label: String,
    /// Stable PostgreSQL type identity.
    pub type_id: TypeId,
    /// Resolved typmod.
    pub typmod: Option<Typmod>,
    /// Proof-bearing SQL nullability.
    pub nullability: Nullability,
    /// Catalog-owned PostgreSQL codec identity.
    pub pg_codec_id: PgCodecId,
    /// Catalog-owned wire codec identity.
    pub wire_codec_id: WireCodecId,
    /// Typed source expression identity.
    pub source_expression: ExpressionId,
}

/// Complete semantic-checking output consumed by the artifact compiler.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct CheckedOutput {
    /// Typed statement preserving the HIR topology and semantic IDs.
    pub statement: TypedStatement,
    /// Inferred final cardinality (equal to `statement.cardinality`).
    pub cardinality: Cardinality,
    /// Runtime checks required for dynamic LIMIT/OFFSET and rowless execution.
    pub runtime_assertions: Vec<RuntimeAssertion>,
    /// Checked parameters in declaration order.
    pub parameters: Vec<CheckedParameter>,
    /// Checked outputs in projection order.
    pub output_fields: Vec<CheckedField>,
}

impl CheckedOutput {
    /// Validates a declared result mode against inferred row-count and output-shape facts.
    pub fn validate_mode(&self, mode: ResultMode) -> Result<(), CardinalityModeError> {
        let has_output = !self.output_fields.is_empty();
        let maximum = match self.cardinality.upper() {
            UpperBound::Zero => Some(0),
            UpperBound::One => Some(1),
            UpperBound::Finite(value) => Some(value),
            UpperBound::Unbounded | UpperBound::Unknown => None,
        };
        let valid = match mode {
            ResultMode::Many => has_output,
            ResultMode::Optional => has_output && maximum.is_some_and(|value| value <= 1),
            ResultMode::One => {
                has_output
                    && self.cardinality.lower() == LowerBound::One
                    && maximum.is_some_and(|value| value <= 1)
            }
            ResultMode::Exec => {
                !has_output
                    && maximum == Some(0)
                    && self.runtime_assertions.contains(&RuntimeAssertion::Rowless)
            }
        };
        valid
            .then_some(())
            .ok_or_else(|| CardinalityModeError::Incompatible {
                mode,
                cardinality: self.cardinality.clone(),
                has_output,
            })
    }
}

/// Declared result mode is incompatible with checked row-count or row-shape facts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CardinalityModeError {
    /// The mode cannot accept this result.
    #[error(
        "declared result mode {mode:?} is incompatible with inferred cardinality {cardinality:?}"
    )]
    Incompatible {
        /// Declared mode.
        mode: ResultMode,
        /// Inferred proof-bearing cardinality.
        cardinality: Cardinality,
        /// Whether the statement produces output columns.
        has_output: bool,
    },
}

/// PostgreSQL type-resolution failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TypeResolutionError {
    /// An operator cannot accept the operand types.
    #[error("operator {operator} cannot accept operand types {operand_types:?}")]
    IncompatibleOperator {
        /// Stable authored/resolved operator identity.
        operator: OperatorId,
        /// Ordered operand types; `None` represents an unresolved unknown/null literal.
        operand_types: Vec<Option<TypeId>>,
    },
    /// Multiple operator candidates remain equally valid.
    #[error("operator {name} is ambiguous for operand types {operand_types:?}")]
    AmbiguousOperator {
        /// Qualified operator name.
        name: String,
        /// Ordered operand types.
        operand_types: Vec<Option<TypeId>>,
        /// Stable candidate identities.
        candidates: Vec<OperatorId>,
    },
    /// No callable accepts the argument types.
    #[error("callable {name} cannot accept argument types {argument_types:?}")]
    IncompatibleCallable {
        /// Qualified callable name.
        name: String,
        /// Ordered argument types.
        argument_types: Vec<Option<TypeId>>,
    },
    /// Multiple callable candidates remain equally valid.
    #[error("callable {name} is ambiguous for argument types {argument_types:?}")]
    AmbiguousCallable {
        /// Qualified callable name.
        name: String,
        /// Ordered argument types; `None` represents an unresolved unknown/null literal.
        argument_types: Vec<Option<TypeId>>,
        /// Stable candidate identities.
        candidates: Vec<CallableId>,
    },
    /// A common-value family has no compatible PostgreSQL type.
    #[error("expressions have no common PostgreSQL type: {types:?}")]
    IncompatibleCommonType {
        /// Ordered resolved input types; unknown/null inputs are `None`.
        types: Vec<Option<TypeId>>,
    },
    /// An empty array has no contextual element type.
    #[error("empty ARRAY requires a contextual type")]
    IndeterminateArrayType,
    /// A stable catalog object required by the HIR is absent.
    #[error("missing catalog fact {kind} {identity}")]
    MissingCatalogFact {
        /// Fact category.
        kind: &'static str,
        /// Stable identity or qualified lookup key.
        identity: String,
    },
}

/// Semantic checking failure with the exact expression or clause origin.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    /// PostgreSQL type resolution failed.
    #[error(transparent)]
    Type(#[from] TypeResolutionError),
    /// A predicate clause did not resolve to Boolean.
    #[error("{clause} expression must be boolean, got {actual}")]
    NonBooleanPredicate {
        /// SQL clause name.
        clause: &'static str,
        /// Actual resolved type.
        actual: TypeId,
        /// Exact source origin.
        origin: SourceOrigin,
    },
    /// Set-operation inputs have different projection counts.
    #[error("set operation has {left} left columns and {right} right columns")]
    SetColumnCountMismatch {
        /// Left projection count.
        left: usize,
        /// Right projection count.
        right: usize,
    },
    /// A referenced HIR parameter is not declared.
    #[error("unknown parameter {parameter_id}")]
    UnknownParameter {
        /// Missing parameter identity.
        parameter_id: ParameterId,
        /// Exact reference origin.
        origin: SourceOrigin,
    },
    /// A referenced column does not exist in the catalog or relation scope.
    #[error("unknown column {column_id} for relation binding {binding}")]
    UnknownColumn {
        /// Relation binding.
        binding: RelationId,
        /// Stable column identity.
        column_id: ColumnId,
        /// Exact reference origin.
        origin: SourceOrigin,
    },
    /// A CTE projected field is not available at the reference site.
    #[error("unknown CTE field {field_id} on CTE {cte_id}")]
    UnknownCteField {
        /// CTE identity.
        cte_id: CteId,
        /// Projected field identity.
        field_id: FieldId,
        /// Exact reference origin.
        origin: SourceOrigin,
    },
    /// LIMIT/OFFSET is neither a non-negative integer constant nor an integer parameter.
    #[error("{clause} requires a non-negative integer constant or integer parameter")]
    InvalidLimit {
        /// LIMIT or OFFSET.
        clause: &'static str,
        /// Exact expression origin.
        origin: SourceOrigin,
    },
    /// The resolved statement category is outside this checker lane.
    #[error("semantic typing for {statement} is not available in this checker lane")]
    UnsupportedStatement {
        /// Stable statement category.
        statement: &'static str,
        /// Exact statement origin.
        origin: SourceOrigin,
    },
    /// Recursive CTE typing needs anchor/step common-row validation not represented by this lane.
    #[error("recursive CTE semantic typing is not supported")]
    UnsupportedRecursiveCte {
        /// Exact SELECT origin.
        origin: SourceOrigin,
    },
    /// A scalar subquery is not statically proven to produce at most one row.
    #[error("scalar subquery is not proven to return at most one row")]
    UnboundedScalarSubquery {
        /// Exact scalar-subquery origin.
        origin: SourceOrigin,
        /// Inferred subquery cardinality.
        cardinality: Cardinality,
    },
    /// Aggregate and nonaggregate projections are mixed without grouping proof.
    #[error("aggregate query contains an ungrouped nonaggregate projection")]
    UngroupedAggregateProjection {
        /// Exact projection origin.
        origin: SourceOrigin,
    },
    /// DISTINCT ON expressions do not match the leading ORDER BY expressions.
    #[error("DISTINCT ON expressions must match the leading ORDER BY expressions")]
    DistinctOnOrderMismatch {
        /// Exact DISTINCT ON expression origin.
        origin: SourceOrigin,
    },
    /// A numeric literal cannot be represented by the contextual PostgreSQL type.
    #[error("numeric literal {value} cannot be represented as {target}")]
    NumericLiteralOutOfRange {
        /// Exact authored literal spelling.
        value: String,
        /// Contextual target type.
        target: TypeId,
        /// Exact literal origin.
        origin: SourceOrigin,
    },
    /// The finalized typed IR rejected the produced proof or topology.
    #[error("typed IR validation failed: {0}")]
    InvalidTypedShape(dibs_query_ir::TypedShapeError),
}

impl From<dibs_query_ir::TypedShapeError> for CheckError {
    fn from(value: dibs_query_ir::TypedShapeError) -> Self {
        Self::InvalidTypedShape(value)
    }
}

#[derive(Clone)]
struct CheckContext<'hir> {
    parameters: BTreeMap<ParameterId, &'hir HirParameter>,
    relations: BTreeMap<RelationId, BTreeMap<RelationField, BoundColumn>>,
    null_extended: BTreeSet<RelationId>,
    ctes: BTreeMap<CteId, BTreeMap<FieldId, TypedExpression>>,
}

impl<'hir> CheckContext<'hir> {
    fn new(parameters: BTreeMap<ParameterId, &'hir HirParameter>) -> Self {
        Self {
            parameters,
            relations: BTreeMap::new(),
            null_extended: BTreeSet::new(),
            ctes: BTreeMap::new(),
        }
    }

    fn bind_table(&mut self, binding: RelationId, table: &CatalogTable) {
        self.relations.insert(
            binding,
            table
                .columns
                .iter()
                .map(|column| {
                    (
                        RelationField::Catalog(column.id.clone()),
                        BoundColumn {
                            type_id: column.type_id.clone(),
                            typmod: None,
                            nullable: column.nullability == CatalogNullability::Nullable,
                            volatility: Volatility::Immutable,
                        },
                    )
                })
                .collect(),
        );
    }

    fn bind_projection(&mut self, binding: RelationId, statement: &TypedStatement) {
        self.relations.insert(
            binding,
            statement_projections(statement)
                .iter()
                .map(|projection| {
                    (
                        RelationField::Derived(projection.field_id),
                        BoundColumn {
                            type_id: projection.output_type_id().clone(),
                            typmod: projection.output_typmod().cloned(),
                            nullable: projection.output_nullability().is_nullable(),
                            volatility: projection.expression.volatility,
                        },
                    )
                })
                .collect(),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RelationField {
    Catalog(ColumnId),
    Derived(FieldId),
}

#[derive(Clone)]
struct BoundColumn {
    type_id: TypeId,
    typmod: Option<Typmod>,
    nullable: bool,
    volatility: Volatility,
}

struct BuiltinTypes {
    boolean: TypeId,
    smallint: TypeId,
    integer: TypeId,
    bigint: TypeId,
    numeric: TypeId,
    text: TypeId,
    bytea: TypeId,
    unknown: TypeId,
}

impl BuiltinTypes {
    fn new(catalog: &CatalogSnapshot) -> Self {
        let resolve = |name: &str| {
            catalog
                .resolve_type(name)
                .map(|ty| ty.id.clone())
                .unwrap_or_else(|_| TypeId::new(format!("pg18:type:pseudo:{name}")))
        };
        Self {
            boolean: resolve("pg_catalog.boolean"),
            smallint: resolve("pg_catalog.smallint"),
            integer: resolve("pg_catalog.integer"),
            bigint: resolve("pg_catalog.bigint"),
            numeric: resolve("pg_catalog.numeric"),
            text: resolve("pg_catalog.text"),
            bytea: resolve("pg_catalog.bytea"),
            unknown: catalog
                .types
                .iter()
                .find(|ty| ty.qualified_name == "pg_catalog.unknown")
                .map(|ty| ty.id.clone())
                .unwrap_or_else(|| TypeId::new("pg18:type:pseudo:pg_catalog.unknown")),
        }
    }
}

#[derive(Clone, Copy)]
enum StructuralOperator {
    And,
    Or,
    Not,
    IsNull,
    IsNotNull,
    IsDistinctFrom,
    IsNotDistinctFrom,
}

#[derive(Debug)]
enum SelectionError<T> {
    None,
    Ambiguous(Vec<T>),
}

struct ResolvedCandidate<T> {
    candidate: T,
    argument_types: Vec<TypeId>,
}

struct PolymorphicResolution {
    argument_types: Vec<TypeId>,
}

/// PostgreSQL 18 semantic checker over finalized resolved HIR.
pub struct SemanticChecker<'catalog> {
    catalog: &'catalog CatalogSnapshot,
    types: BuiltinTypes,
}

impl<'catalog> SemanticChecker<'catalog> {
    /// Creates a checker using an immutable versioned catalog snapshot.
    #[must_use]
    pub fn new(catalog: &'catalog CatalogSnapshot) -> Self {
        Self {
            catalog,
            types: BuiltinTypes::new(catalog),
        }
    }

    /// Checks one fully resolved HIR query.
    pub fn check_query(&self, query: &HirQuery) -> Result<CheckedOutput, CheckError> {
        if self.catalog.postgres_major != 18 {
            return Err(TypeResolutionError::MissingCatalogFact {
                kind: "PostgreSQL-major",
                identity: self.catalog.postgres_major.to_string(),
            }
            .into());
        }
        let parameters = self.check_parameters(&query.parameters)?;
        let parameter_map = query
            .parameters
            .iter()
            .map(|parameter| (parameter.id, parameter))
            .collect();
        let mut context = CheckContext::new(parameter_map);
        let statement = self.check_statement(&query.statement, &mut context)?;
        statement.validate()?;
        let output_fields = self.checked_fields(&statement)?;
        let cardinality = statement.cardinality.clone();
        let runtime_assertions = statement_runtime_assertions(&statement);
        Ok(CheckedOutput {
            statement,
            cardinality,
            runtime_assertions,
            parameters,
            output_fields,
        })
    }

    fn check_parameters(
        &self,
        parameters: &[HirParameter],
    ) -> Result<Vec<CheckedParameter>, CheckError> {
        parameters
            .iter()
            .map(|parameter| {
                let ty = self.catalog.type_by_id(&parameter.type_id).ok_or_else(|| {
                    TypeResolutionError::MissingCatalogFact {
                        kind: "parameter-type",
                        identity: parameter.type_id.to_string(),
                    }
                })?;
                Ok(CheckedParameter {
                    id: parameter.id,
                    ordinal: parameter.ordinal,
                    type_id: parameter.type_id.clone(),
                    typmod: parameter.typmod.clone(),
                    nullable: parameter.nullable,
                    pg_codec_id: ty.pg_codec_id.clone(),
                    wire_codec_id: ty.wire_codec_id.clone(),
                })
            })
            .collect()
    }

    fn checked_fields(&self, statement: &TypedStatement) -> Result<Vec<CheckedField>, CheckError> {
        statement_projections(statement)
            .iter()
            .enumerate()
            .map(|(ordinal, projection)| {
                let output_type_id = projection.output_type_id();
                let ty = self.catalog.type_by_id(output_type_id).ok_or_else(|| {
                    TypeResolutionError::MissingCatalogFact {
                        kind: "output-type",
                        identity: output_type_id.to_string(),
                    }
                })?;
                Ok(CheckedField {
                    id: projection.field_id,
                    ordinal: ordinal as u32,
                    sql_label: projection.sql_label.clone(),
                    type_id: output_type_id.clone(),
                    typmod: projection.output_typmod().cloned(),
                    nullability: projection.output_nullability().clone(),
                    pg_codec_id: ty.pg_codec_id.clone(),
                    wire_codec_id: ty.wire_codec_id.clone(),
                    source_expression: projection.expression.id,
                })
            })
            .collect()
    }

    fn check_statement(
        &self,
        statement: &HirStatement,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedStatement, CheckError> {
        match &statement.kind {
            HirStatementKind::Select(select) => self.check_select(statement, select, context),
            HirStatementKind::Insert(_) => Err(CheckError::UnsupportedStatement {
                statement: "INSERT",
                origin: statement.origin.clone(),
            }),
            HirStatementKind::Update(_) => Err(CheckError::UnsupportedStatement {
                statement: "UPDATE",
                origin: statement.origin.clone(),
            }),
            HirStatementKind::Delete(_) => Err(CheckError::UnsupportedStatement {
                statement: "DELETE",
                origin: statement.origin.clone(),
            }),
        }
    }
}

fn statement_projections(statement: &TypedStatement) -> &[TypedProjection] {
    match &statement.kind {
        TypedStatementKind::Select(select) => &select.projections,
        TypedStatementKind::Insert(insert) => &insert.returning,
        TypedStatementKind::Update(update) => &update.returning,
        TypedStatementKind::Delete(delete) => &delete.returning,
    }
}
fn statement_projections_mut(statement: &mut TypedStatement) -> &mut [TypedProjection] {
    match &mut statement.kind {
        TypedStatementKind::Select(select) => &mut select.projections,
        TypedStatementKind::Insert(insert) => &mut insert.returning,
        TypedStatementKind::Update(update) => &mut update.returning,
        TypedStatementKind::Delete(delete) => &mut delete.returning,
    }
}

fn statement_runtime_assertions(statement: &TypedStatement) -> Vec<RuntimeAssertion> {
    let mut assertions = Vec::new();
    if statement_projections(statement).is_empty() {
        assertions.push(RuntimeAssertion::Rowless);
    }
    if let TypedStatementKind::Select(select) = &statement.kind {
        for bound in [&select.limit, &select.offset] {
            if let Some(TypedLimit::Parameter(parameter_id)) = bound {
                assertions.push(RuntimeAssertion::ValidLimitParameter {
                    parameter_id: *parameter_id,
                });
            }
        }
    }
    assertions
}

fn synthetic_field_column(binding: RelationId, field: FieldId) -> ColumnId {
    ColumnId::new(format!("pg18:column:derived:{binding}:{field}"))
}
