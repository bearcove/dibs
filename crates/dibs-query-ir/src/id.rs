use std::fmt;

macro_rules! local_id {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
        #[repr(transparent)]
        pub struct $name(u32);

        impl $name {
            /// Creates a revision-local identity from its deterministic numeric value.
            #[must_use]
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            /// Returns the deterministic numeric value.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

local_id!(QueryId, "Revision-local query identity.");
local_id!(ParameterId, "Revision-local parameter identity.");
local_id!(StatementId, "Revision-local statement identity.");
local_id!(RelationId, "Revision-local relation-binding identity.");
local_id!(CteId, "Revision-local common-table-expression identity.");
local_id!(ExpressionId, "Revision-local expression identity.");
local_id!(FieldId, "Revision-local projected-field identity.");
local_id!(ReferenceId, "Revision-local resolved-reference identity.");
local_id!(SqlNodeId, "Revision-local rendered SQL node identity.");
local_id!(LineageNodeId, "Revision-local lineage graph node identity.");
local_id!(AssignmentId, "Revision-local mutation assignment identity.");

/// Any typed IR node that can own a reference or rendered SQL fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum TypedNodeId {
    /// Top-level or nested statement.
    Statement(StatementId),
    /// Relation or relation binding.
    Relation(RelationId),
    /// Common-table expression.
    Cte(CteId),
    /// Scalar expression.
    Expression(ExpressionId),
    /// Projected output field.
    Field(FieldId),
    /// Mutation assignment.
    Assignment(AssignmentId),
}
