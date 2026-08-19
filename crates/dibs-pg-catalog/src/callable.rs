use crate::{CallableId, Nullability, TypeId};

/// Exact callable result shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableKind {
    /// One scalar result value.
    Scalar,
    /// Set-returning function with named output columns.
    Table,
}

impl CallableKind {
    pub(crate) const fn id_component(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Table => "table",
        }
    }
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
    CallableId::new(format!(
        "pg{postgres_major}:callable:{}:{}({})->{}",
        CallableKind::Scalar.id_component(),
        signature.qualified_name,
        join_type_ids(&signature.arguments),
        signature.result.as_str()
    ))
}

pub(crate) fn stable_table_id(postgres_major: u16, signature: &TableSignature) -> CallableId {
    let mut result = String::new();
    for (index, column) in signature.columns.iter().enumerate() {
        if index != 0 {
            result.push(',');
        }
        use std::fmt::Write as _;
        let nullability = match column.nullability {
            Nullability::NotNull => "not-null",
            Nullability::Nullable => "nullable",
        };
        let _ = write!(
            result,
            "{}:{}:{}:{}",
            column.name.len(),
            column.name,
            column.type_id.as_str(),
            nullability
        );
    }
    CallableId::new(format!(
        "pg{postgres_major}:callable:{}:{}({})->table({result})",
        CallableKind::Table.id_component(),
        signature.qualified_name,
        join_type_ids(&signature.arguments),
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
