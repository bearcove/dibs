use crate::{CallableId, Nullability, TypeId};

/// Exact callable result shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableKind {
    /// One scalar result value.
    Scalar,
    /// Set-returning function with named output columns.
    Table,
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
    /// Scalar or table result shape.
    pub kind: CallableKind,
    /// Ordered logical argument types.
    pub arguments: Vec<TypeId>,
    /// Scalar result type, when `kind` is scalar.
    pub scalar_result: Option<TypeId>,
    /// Table result columns, when `kind` is table.
    pub table_columns: Vec<TableOutputColumn>,
    /// PostgreSQL identity-argument rendering used by the live oracle.
    pub postgres_identity_arguments: String,
    /// PostgreSQL result rendering used by the live oracle.
    pub postgres_result_type: String,
    /// Whether this callable belongs to the curated PostgreSQL fixture.
    pub builtin: bool,
}

impl CatalogCallable {
    pub(crate) fn scalar(
        postgres_major: u16,
        signature: ScalarSignature,
        postgres_identity_arguments: String,
        postgres_result_type: String,
        builtin: bool,
    ) -> Self {
        let id = stable_scalar_id(postgres_major, &signature);
        Self {
            id,
            qualified_name: signature.qualified_name,
            kind: CallableKind::Scalar,
            arguments: signature.arguments,
            scalar_result: Some(signature.result),
            table_columns: Vec::new(),
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
        builtin: bool,
    ) -> Self {
        let id = stable_table_id(postgres_major, &signature);
        Self {
            id,
            qualified_name: signature.qualified_name,
            kind: CallableKind::Table,
            arguments: signature.arguments,
            scalar_result: None,
            table_columns: signature.columns,
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
