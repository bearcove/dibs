use dibs_pg_catalog::{CallableId, ColumnId, ConstraintId};

use crate::{CteId, ExpressionId, RelationId};

/// Proven lower bound for relational row count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum LowerBound {
    /// Zero rows are possible.
    Zero,
    /// At least one row is proven.
    One,
}

/// Proven upper bound for relational row count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum UpperBound {
    /// Exactly zero rows.
    Zero,
    /// No more than one row.
    One,
    /// More than one row is possible but some finite bound is proven.
    Finite(u64),
    /// No finite upper bound is proven.
    Unbounded,
    /// Analysis cannot currently characterize the upper bound soundly.
    Unknown,
}

/// One proof step contributing to an inferred row-count bound.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum CardinalityEvidence {
    /// No stronger proof is available.
    Conservative,
    /// A predicate contradicts itself or a relation is statically empty.
    EmptyRelation,
    /// Complete primary/unique-key equality constrains the relation.
    UniquePredicate {
        /// Stable catalog constraint used by the proof.
        constraint_id: ConstraintId,
        /// Stable catalog columns covered in constraint order.
        columns: Vec<ColumnId>,
    },
    /// Scalar aggregate without grouping produces one row.
    ScalarAggregate {
        /// Aggregate expression carrying the proof.
        expression: ExpressionId,
    },
    /// Scalar subquery semantics constrain the result to zero or one row.
    ScalarSubquery {
        /// Relation evaluated as a scalar subquery.
        relation: RelationId,
    },
    /// Exact authored `VALUES` row count.
    ValuesRowCount {
        /// Number of rows in the values relation.
        rows: u64,
    },
    /// `LIMIT` or `FETCH` constrains only the upper bound.
    Limit {
        /// Constant bound.
        limit: u64,
    },
    /// Join composition propagated relational bounds.
    Join {
        /// Join relation carrying the proof.
        relation: RelationId,
    },
    /// Set-operation composition propagated relational bounds.
    SetOperation {
        /// Relation carrying the set operation.
        relation: RelationId,
    },
    /// CTE output bound was propagated to a use site.
    CtePropagation {
        /// CTE carrying the proven bound.
        cte: CteId,
    },
    /// Mutation `RETURNING` inherits the mutation's affected-row bound.
    MutationReturning,
    /// Registered function contract supplies an authoritative bound.
    RegisteredFunction {
        /// Stable callable identity.
        callable_id: CallableId,
    },
}

/// Proof-bearing relational row-count range.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct Cardinality {
    /// Proven lower bound.
    pub lower: LowerBound,
    /// Proven upper bound.
    pub upper: UpperBound,
    /// Ordered proof chain.
    pub proof: Vec<CardinalityEvidence>,
}

impl Cardinality {
    /// Creates a cardinality with explicit proof.
    #[must_use]
    pub fn new(lower: LowerBound, upper: UpperBound, proof: Vec<CardinalityEvidence>) -> Self {
        Self {
            lower,
            upper,
            proof,
        }
    }

    /// Zero rows.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(
            LowerBound::Zero,
            UpperBound::Zero,
            vec![CardinalityEvidence::EmptyRelation],
        )
    }

    /// Zero or one rows.
    #[must_use]
    pub fn at_most_one() -> Self {
        Self::at_most_one_with(CardinalityEvidence::Limit { limit: 1 })
    }

    /// Zero or one rows with a named proof step.
    #[must_use]
    pub fn at_most_one_with(evidence: CardinalityEvidence) -> Self {
        Self::new(LowerBound::Zero, UpperBound::One, vec![evidence])
    }

    /// Exactly one row, proven by an upper limit plus an independent lower bound.
    #[must_use]
    pub fn exactly_one() -> Self {
        Self::new(
            LowerBound::One,
            UpperBound::One,
            vec![CardinalityEvidence::Limit { limit: 1 }],
        )
    }

    /// Zero or more rows with no finite upper bound.
    #[must_use]
    pub fn many() -> Self {
        Self::new(LowerBound::Zero, UpperBound::Unbounded, Vec::new())
    }

    /// One or more rows with no finite upper bound.
    #[must_use]
    pub fn at_least_one() -> Self {
        Self::new(LowerBound::One, UpperBound::Unbounded, Vec::new())
    }

    /// Bounds not currently characterized by the compiler.
    #[must_use]
    pub fn unknown() -> Self {
        Self::new(
            LowerBound::Zero,
            UpperBound::Unknown,
            vec![CardinalityEvidence::Conservative],
        )
    }

    /// Applies a constant `LIMIT`/`FETCH` without inventing a lower-bound proof.
    #[must_use]
    pub fn limit(&self, limit: u64) -> Self {
        let upper = match (self.upper, limit) {
            (_, 0) => UpperBound::Zero,
            (UpperBound::Zero, _) => UpperBound::Zero,
            (UpperBound::One, _) => UpperBound::One,
            (UpperBound::Finite(existing), value) => UpperBound::Finite(existing.min(value)),
            (UpperBound::Unbounded | UpperBound::Unknown, 1) => UpperBound::One,
            (UpperBound::Unbounded | UpperBound::Unknown, value) => UpperBound::Finite(value),
        };
        let lower = if upper == UpperBound::Zero {
            LowerBound::Zero
        } else {
            self.lower
        };
        let mut proof = self.proof.clone();
        proof.push(CardinalityEvidence::Limit { limit });
        Self::new(lower, upper, proof)
    }
}

/// One proof step contributing to expression nullability.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum NullabilityEvidence {
    /// Conservative nullable fallback when non-nullness cannot be proven.
    Conservative,
    /// Catalog `NOT NULL` fact.
    BaseColumnNotNull {
        /// Stable catalog column identity.
        column_id: ColumnId,
    },
    /// A nullable base catalog column.
    BaseColumnNullable {
        /// Stable catalog column identity.
        column_id: ColumnId,
    },
    /// Typed SQL `NULL` literal.
    NullLiteral,
    /// Outer join null-extends one relation binding.
    OuterJoinNullExtension {
        /// Null-extended binding.
        binding: RelationId,
    },
    /// Scalar subquery may produce zero rows.
    ScalarSubqueryZeroRows {
        /// Scalar subquery relation.
        relation: RelationId,
    },
    /// Aggregate result may be null on empty input.
    AggregateEmptyInput {
        /// Aggregate expression.
        expression: ExpressionId,
    },
    /// `CASE` branch or missing `ELSE` permits null.
    CaseBranch,
    /// Function/operator result contract.
    CallableContract {
        /// Stable resolved callable identity.
        callable_id: CallableId,
        /// Whether its registered contract proves a non-null result.
        proves_non_null: bool,
    },
    /// Cast/coercion propagation.
    CastPropagation,
    /// CTE propagation.
    CtePropagation {
        /// CTE carrying the proof.
        cte: CteId,
    },
    /// Set-operation common output propagation.
    SetOperationPropagation,
    /// Mutation `RETURNING` propagation.
    MutationReturning,
}

/// Conservative, proof-bearing expression nullability.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct Nullability {
    /// Whether SQL `NULL` is possible.
    pub nullable: bool,
    /// Ordered proof chain. Non-null values require positive evidence.
    pub evidence: Vec<NullabilityEvidence>,
}

impl Nullability {
    /// Creates a nullable value with evidence.
    #[must_use]
    pub fn nullable(evidence: NullabilityEvidence) -> Self {
        Self {
            nullable: true,
            evidence: vec![evidence],
        }
    }

    /// Creates a proven non-null value.
    #[must_use]
    pub fn not_null(evidence: NullabilityEvidence) -> Self {
        Self {
            nullable: false,
            evidence: vec![evidence],
        }
    }

    /// Returns whether SQL `NULL` is possible.
    #[must_use]
    pub const fn is_nullable(&self) -> bool {
        self.nullable
    }

    /// Returns whether positive evidence accompanies a non-null claim.
    #[must_use]
    pub fn has_non_null_proof(&self) -> bool {
        !self.nullable
            && self.evidence.iter().any(|evidence| {
                matches!(
                    evidence,
                    NullabilityEvidence::BaseColumnNotNull { .. }
                        | NullabilityEvidence::CallableContract {
                            proves_non_null: true,
                            ..
                        }
                )
            })
    }
}
