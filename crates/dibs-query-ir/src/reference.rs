use std::collections::{BTreeMap, BTreeSet};

use dibs_pg_catalog::{
    CallableId, CastId, CollationId, ColumnId, ConstraintId, IndexId, OperatorId, TableId, TypeId,
};

use crate::{
    CteId, ExpressionId, FieldId, LineageNodeId, ParameterId, QueryId, ReferenceId, RelationId,
    SourceOrigin, TargetLanguage, TypedNodeId,
};

/// Semantic role of one resolved source reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ReferenceRole {
    /// Final or intermediate projection.
    Projection,
    /// WHERE/HAVING/filter predicate.
    Predicate,
    /// Join key or join predicate.
    JoinKey,
    /// GROUP BY expression.
    Grouping,
    /// ORDER BY expression.
    Ordering,
    /// Mutation assignment value.
    AssignmentSource,
    /// Mutation assignment destination.
    AssignmentTarget,
    /// INSERT destination.
    InsertTarget,
    /// ON CONFLICT target.
    ConflictTarget,
    /// ON CONFLICT action.
    ConflictAction,
    /// Explicit lock target.
    LockTarget,
    /// Function use.
    FunctionUse,
    /// Operator use.
    OperatorUse,
    /// Cast use.
    CastUse,
    /// CTE dependency.
    CteDependency,
    /// Final RETURNING expression.
    Returning,
}

/// Read/write/lock mode of a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ReferenceAccess {
    /// Semantic read.
    Read,
    /// Mutation write.
    Write,
    /// Row/table lock.
    Lock,
    /// Read followed by write.
    ReadWrite,
}

/// Stable or revision-local target of a resolved reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum ReferenceTarget {
    /// Declared parameter.
    Parameter(ParameterId),
    /// Stable catalog table.
    Table(TableId),
    /// Stable catalog column.
    Column(ColumnId),
    /// Stable catalog constraint.
    Constraint(ConstraintId),
    /// Stable catalog index.
    Index(IndexId),
    /// Stable PostgreSQL type.
    Type(TypeId),
    /// Stable callable.
    Callable(CallableId),
    /// Stable operator.
    Operator(OperatorId),
    /// Stable cast.
    Cast(CastId),
    /// Stable collation.
    Collation(CollationId),
    /// Local relation binding.
    RelationBinding(RelationId),
    /// Local CTE binding.
    Cte(CteId),
    /// Local projected field.
    OutputField(FieldId),
}

/// Kind of generated contract member connected to a semantic reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum GeneratedMemberKind {
    /// Generated parameter member.
    Parameter,
    /// Generated output field member.
    OutputField,
    /// Generated operation/query symbol.
    Operation,
}

/// One generated API member reached from the compiler reference graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct GeneratedContractMember {
    /// Target language.
    pub language: TargetLanguage,
    /// Member kind.
    pub kind: GeneratedMemberKind,
    /// Validated target-language member name.
    pub name: String,
}

/// One compiler-owned resolved reference.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct ResolvedReference {
    /// Revision-local reference identity.
    pub id: ReferenceId,
    /// Enclosing query.
    pub query_id: QueryId,
    /// Exact enclosing semantic node.
    pub enclosing_node: TypedNodeId,
    /// Exact authored origin.
    pub origin: SourceOrigin,
    /// Resolved target identity.
    pub target: ReferenceTarget,
    /// Semantic role.
    pub role: ReferenceRole,
    /// Read/write/lock access.
    pub access: ReferenceAccess,
    /// Optional lineage node representing this use.
    pub lineage_node: Option<LineageNodeId>,
    /// Generated API members connected to this reference.
    pub generated_members: Vec<GeneratedContractMember>,
}

/// Compiler-owned reference index used by every downstream consumer.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct ReferenceIndex {
    /// Canonically ordered reference records.
    pub references: Vec<ResolvedReference>,
}

impl ReferenceIndex {
    /// Creates an index and canonicalizes its semantically unordered records.
    #[must_use]
    pub fn new(mut references: Vec<ResolvedReference>) -> Self {
        canonicalize_references(&mut references);
        Self { references }
    }

    /// Returns references to an exact resolved target.
    #[must_use]
    pub fn references_to(&self, target: &ReferenceTarget) -> Vec<&ResolvedReference> {
        self.references
            .iter()
            .filter(|reference| &reference.target == target)
            .collect()
    }

    /// Returns a canonicalized clone for serialization and identity input.
    #[must_use]
    pub fn canonicalized(&self) -> Self {
        Self::new(self.references.clone())
    }

    /// Reverses the unordered storage to prove consumers canonicalize it.
    #[doc(hidden)]
    pub fn reverse_unordered_for_test(&mut self) {
        self.references.reverse();
    }
}

fn canonicalize_references(references: &mut [ResolvedReference]) {
    for reference in references.iter_mut() {
        reference.generated_members.sort();
        reference.generated_members.dedup();
    }
    references.sort_by(|left, right| {
        (
            &left.target,
            left.role,
            left.access,
            left.query_id,
            left.enclosing_node,
            left.id,
        )
            .cmp(&(
                &right.target,
                right.role,
                right.access,
                right.query_id,
                right.enclosing_node,
                right.id,
            ))
    });
}

/// Value represented by one output-lineage graph node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum LineageValue {
    /// Final output field.
    OutputField(FieldId),
    /// Typed source expression.
    Expression(ExpressionId),
    /// CTE output field.
    CteField {
        /// CTE identity.
        cte_id: CteId,
        /// CTE projected field.
        field_id: FieldId,
    },
    /// Stable base catalog column.
    CatalogColumn(ColumnId),
    /// Generated API member.
    GeneratedMember(GeneratedContractMember),
}

/// One lineage graph node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct LineageNode {
    /// Revision-local node identity.
    pub id: LineageNodeId,
    /// Semantic value represented by the node.
    pub value: LineageValue,
}

/// Kind of lineage edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum LineageEdgeKind {
    /// Target value is derived from the source value.
    DerivedFrom,
    /// Target is a generated API representation of the source.
    Generates,
}

/// One directed lineage edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct LineageEdge {
    /// Source node.
    pub from: LineageNodeId,
    /// Target node.
    pub to: LineageNodeId,
    /// Edge semantics.
    pub kind: LineageEdgeKind,
}

impl LineageEdge {
    /// Creates a semantic derivation edge.
    #[must_use]
    pub const fn derived(from: LineageNodeId, to: LineageNodeId) -> Self {
        Self {
            from,
            to,
            kind: LineageEdgeKind::DerivedFrom,
        }
    }

    /// Creates a generated-contract edge.
    #[must_use]
    pub const fn generated(from: LineageNodeId, to: LineageNodeId) -> Self {
        Self {
            from,
            to,
            kind: LineageEdgeKind::Generates,
        }
    }
}

/// Directed output-lineage graph reaching stable catalog columns and generated members.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct LineageGraph {
    /// Canonically ordered nodes.
    pub nodes: Vec<LineageNode>,
    /// Canonically ordered, semantically unordered edges.
    pub edges: Vec<LineageEdge>,
}

impl LineageGraph {
    /// Creates and canonicalizes a lineage graph.
    #[must_use]
    pub fn new(mut nodes: Vec<LineageNode>, mut edges: Vec<LineageEdge>) -> Self {
        nodes.sort();
        nodes.dedup();
        edges.sort();
        edges.dedup();
        Self { nodes, edges }
    }

    /// Returns a canonicalized clone.
    #[must_use]
    pub fn canonicalized(&self) -> Self {
        Self::new(self.nodes.clone(), self.edges.clone())
    }

    /// Returns every stable catalog column reachable from an output field.
    #[must_use]
    pub fn catalog_columns_for_field(&self, field_id: FieldId) -> Vec<ColumnId> {
        let starts: Vec<_> = self
            .nodes
            .iter()
            .filter(|node| node.value == LineageValue::OutputField(field_id))
            .map(|node| node.id)
            .collect();
        if starts.is_empty() {
            return Vec::new();
        }

        let values: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|node| (node.id, &node.value))
            .collect();
        let adjacency: BTreeMap<_, Vec<_>> = {
            let mut adjacency: BTreeMap<LineageNodeId, Vec<LineageNodeId>> = BTreeMap::new();
            for edge in &self.edges {
                if edge.kind == LineageEdgeKind::DerivedFrom {
                    adjacency.entry(edge.from).or_default().push(edge.to);
                }
            }
            adjacency
        };
        let mut pending = starts;
        let mut visited = BTreeSet::new();
        let mut columns = BTreeSet::new();
        while let Some(node_id) = pending.pop() {
            if !visited.insert(node_id) {
                continue;
            }
            if let Some(LineageValue::CatalogColumn(column_id)) = values.get(&node_id) {
                columns.insert((*column_id).clone());
            }
            if let Some(next) = adjacency.get(&node_id) {
                pending.extend(next.iter().copied());
            }
        }
        columns.into_iter().collect()
    }

    /// Reverses unordered graph storage to prove canonicalization.
    #[doc(hidden)]
    pub fn reverse_unordered_for_test(&mut self) {
        self.nodes.reverse();
        self.edges.reverse();
    }
}
