use crate::{CallableId, Nullability, TypeId};

/// PostgreSQL callable kind from `pg_proc.prokind`, plus table-function shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableKind {
    /// Ordinary scalar function.
    Scalar,
    /// Aggregate function.
    Aggregate,
    /// Window function.
    Window,
    /// Set-returning function with named output columns.
    Table,
}

/// PostgreSQL callable volatility from `pg_proc.provolatile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Volatility {
    /// Result depends only on arguments.
    Immutable,
    /// Result is stable within one statement.
    Stable,
    /// Result may change within one statement or have effects.
    Volatile,
}

/// Exact row-production fact owned by the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableCardinality {
    /// One scalar value per invocation.
    ExactlyOne,
    /// One window value per input row.
    OnePerInput,
    /// Set-returning cardinality is not proven by the catalog.
    SetOfUnknown,
}

/// Aggregate result behavior on an empty input set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AggregateEmptyBehavior {
    /// Returns a non-NULL identity value, such as `count(*) = 0`.
    Identity,
    /// Returns SQL NULL on empty input.
    Null,
}

/// Exact application scalar-function signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarSignature {
    /// SQL-qualified function name.
    pub qualified_name: String,
    /// Ordered logical argument types.
    pub arguments: Vec<TypeId>,
    /// Logical result type.
    pub result: TypeId,
}

impl ScalarSignature {
    /// Returns the PostgreSQL 18 identity for this name and ordered input list.
    #[must_use]
    pub fn postgres_18_id(&self) -> CallableId {
        stable_callable_id(18, &self.qualified_name, &self.arguments)
    }
}

/// Explicit semantic facts required for an application scalar function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalarCallableFacts {
    /// PostgreSQL volatility.
    pub volatility: Volatility,
    /// Whether SQL NULL in any input forces SQL NULL output.
    pub strict: bool,
    /// Scalar result nullability contract.
    pub result_nullability: Nullability,
}

/// Exact application table-function signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSignature {
    /// SQL-qualified function name.
    pub qualified_name: String,
    /// Ordered logical argument types.
    pub arguments: Vec<TypeId>,
    /// Ordered named output columns.
    pub columns: Vec<TableOutputColumn>,
}

/// Explicit semantic facts required for an application table function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCallableFacts {
    /// PostgreSQL volatility.
    pub volatility: Volatility,
    /// Whether SQL NULL in any input produces no rows.
    pub strict: bool,
    /// Authoritative registered row-production fact.
    pub cardinality: CallableCardinality,
}

/// One named output column from a table function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOutputColumn {
    /// Output column name.
    pub name: String,
    /// Logical output type.
    pub type_id: TypeId,
    /// SQL nullability of this output.
    pub nullability: Nullability,
}

/// Registered callable entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogCallable {
    /// Stable logical callable identity.
    pub id: CallableId,
    /// SQL-qualified callable name.
    pub qualified_name: String,
    /// Scalar, aggregate, window, or table result behavior.
    pub kind: CallableKind,
    /// Ordered logical argument types.
    pub arguments: Vec<TypeId>,
    /// Scalar result type for scalar, aggregate, and window callables.
    pub scalar_result: Option<TypeId>,
    /// Table result columns, when `kind` is table.
    pub table_columns: Vec<TableOutputColumn>,
    /// PostgreSQL volatility.
    pub volatility: Volatility,
    /// Whether SQL NULL in any input forces a NULL scalar result or no table rows.
    pub strict: bool,
    /// Scalar result nullability contract, when a scalar result exists.
    pub scalar_result_nullability: Option<Nullability>,
    /// Exact catalog-owned row-production fact.
    pub cardinality: CallableCardinality,
    /// Aggregate empty-input behavior, for aggregate callables.
    pub aggregate_empty: Option<AggregateEmptyBehavior>,
    /// PostgreSQL identity-argument rendering used by the live oracle.
    pub postgres_identity_arguments: String,
    /// PostgreSQL result rendering used by the live oracle.
    pub postgres_result_type: String,
    /// Whether this callable belongs to the curated PostgreSQL fixture.
    pub builtin: bool,
}

/// Semantic facts for one curated PostgreSQL callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallableFacts {
    pub(crate) kind: CallableKind,
    pub(crate) volatility: Volatility,
    pub(crate) strict: bool,
    pub(crate) scalar_result_nullability: Option<Nullability>,
    pub(crate) cardinality: CallableCardinality,
    pub(crate) aggregate_empty: Option<AggregateEmptyBehavior>,
}

impl CallableFacts {
    pub(crate) const fn scalar(facts: ScalarCallableFacts) -> Self {
        Self {
            kind: CallableKind::Scalar,
            volatility: facts.volatility,
            strict: facts.strict,
            scalar_result_nullability: Some(facts.result_nullability),
            cardinality: CallableCardinality::ExactlyOne,
            aggregate_empty: None,
        }
    }

    pub(crate) const fn table(facts: TableCallableFacts) -> Self {
        Self {
            kind: CallableKind::Table,
            volatility: facts.volatility,
            strict: facts.strict,
            scalar_result_nullability: None,
            cardinality: facts.cardinality,
            aggregate_empty: None,
        }
    }
}

impl CatalogCallable {
    pub(crate) fn scalar(
        postgres_major: u16,
        signature: ScalarSignature,
        postgres_identity_arguments: String,
        postgres_result_type: String,
        facts: ScalarCallableFacts,
        builtin: bool,
    ) -> Self {
        Self::scalar_with_facts(
            postgres_major,
            signature,
            postgres_identity_arguments,
            postgres_result_type,
            CallableFacts::scalar(facts),
            builtin,
        )
    }

    pub(crate) fn scalar_with_facts(
        postgres_major: u16,
        signature: ScalarSignature,
        postgres_identity_arguments: String,
        postgres_result_type: String,
        facts: CallableFacts,
        builtin: bool,
    ) -> Self {
        let id = stable_scalar_id(postgres_major, &signature);
        Self {
            id,
            qualified_name: signature.qualified_name,
            kind: facts.kind,
            arguments: signature.arguments,
            scalar_result: Some(signature.result),
            table_columns: Vec::new(),
            volatility: facts.volatility,
            strict: facts.strict,
            scalar_result_nullability: facts.scalar_result_nullability,
            cardinality: facts.cardinality,
            aggregate_empty: facts.aggregate_empty,
            postgres_identity_arguments,
            postgres_result_type,
            builtin,
        }
    }

    pub(crate) fn table(
        postgres_major: u16,
        signature: TableSignature,
        postgres_identity_arguments: String,
        postgres_result_type: String,
        facts: TableCallableFacts,
        builtin: bool,
    ) -> Self {
        let id = stable_table_id(postgres_major, &signature);
        let facts = CallableFacts::table(facts);
        Self {
            id,
            qualified_name: signature.qualified_name,
            kind: facts.kind,
            arguments: signature.arguments,
            scalar_result: None,
            table_columns: signature.columns,
            volatility: facts.volatility,
            strict: facts.strict,
            scalar_result_nullability: facts.scalar_result_nullability,
            cardinality: facts.cardinality,
            aggregate_empty: facts.aggregate_empty,
            postgres_identity_arguments,
            postgres_result_type,
            builtin,
        }
    }
}

pub(crate) fn stable_scalar_id(postgres_major: u16, signature: &ScalarSignature) -> CallableId {
    stable_callable_id(
        postgres_major,
        &signature.qualified_name,
        &signature.arguments,
    )
}

pub(crate) fn stable_table_id(postgres_major: u16, signature: &TableSignature) -> CallableId {
    stable_callable_id(
        postgres_major,
        &signature.qualified_name,
        &signature.arguments,
    )
}

fn stable_callable_id(
    postgres_major: u16,
    qualified_name: &str,
    arguments: &[TypeId],
) -> CallableId {
    CallableId::new(format!(
        "pg{postgres_major}:callable:function:{qualified_name}({})",
        join_type_ids(arguments)
    ))
}

fn join_type_ids(types: &[TypeId]) -> String {
    let mut result = String::new();
    for (index, type_id) in types.iter().enumerate() {
        if index != 0 {
            result.push(',');
        }
        result.push_str(type_id.as_str());
    }
    result
}
