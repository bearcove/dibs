#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Versioned PostgreSQL catalog identities and lossless Dibs codec mappings.

mod callable;
mod codec;
mod id;
mod snapshot;
mod type_system;

pub use callable::{
    CallableKind, CatalogCallable, ScalarSignature, TableOutputColumn, TableSignature,
};
pub use codec::{ApiLanguage, CodecBinding};
pub use id::{
    ApiTypeId, CallableId, CastId, CollationId, OperatorId, PgCodecId, TypeId, WireCodecId,
};
pub use snapshot::{
    CastContext, CastMethod, CatalogCast, CatalogColumn, CatalogForeignKey, CatalogIndex,
    CatalogIndexColumn, CatalogOperator, CatalogSnapshot, CatalogTable, Nullability, PrimaryKey,
    SchemaFingerprint, UniqueConstraint,
};
pub use type_system::{CatalogType, PgTypeKind, TypeRegistration, TypeRegistrationKind};

/// Catalog construction or exact-registration failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    /// No logical type has this exact SQL-qualified name.
    #[error("unknown PostgreSQL type '{qualified_name}'")]
    UnknownType {
        /// SQL-qualified type name.
        qualified_name: String,
    },
    /// No logical type has this exact stable identity.
    #[error("unknown logical PostgreSQL type identity '{id}'")]
    UnknownTypeId {
        /// Stable logical type identity.
        id: TypeId,
    },
    /// No application table has this exact SQL-qualified name.
    #[error("unknown table '{qualified_name}'")]
    UnknownTable {
        /// SQL-qualified table name.
        qualified_name: String,
    },
    /// A curated or schema type has no declared lossless mapping.
    #[error("unsupported lossless mapping for PostgreSQL type '{qualified_name}'")]
    UnsupportedTypeMapping {
        /// SQL-qualified type name.
        qualified_name: String,
    },
    /// The API identity is not registered for this language.
    #[error("unsupported {language:?} API type '{api_type}'")]
    UnsupportedApiType {
        /// API language.
        language: ApiLanguage,
        /// Language API identity.
        api_type: ApiTypeId,
    },
    /// The API identity maps to multiple logical PostgreSQL types.
    #[error("ambiguous {language:?} API type '{api_type}'")]
    AmbiguousApiType {
        /// API language.
        language: ApiLanguage,
        /// Language API identity.
        api_type: ApiTypeId,
    },
    /// A type with this qualified name already exists.
    #[error("type '{qualified_name}' is already registered")]
    DuplicateType {
        /// SQL-qualified type name.
        qualified_name: String,
    },
    /// A callable with this exact stable signature already exists.
    #[error("callable '{id}' is already registered")]
    DuplicateCallable {
        /// Stable logical callable identity.
        id: CallableId,
    },
    /// PostgreSQL already has a function with this name and ordered input types.
    #[error("callable '{qualified_name}' with input types {arguments:?} is already registered")]
    DuplicateCallableSignature {
        /// SQL-qualified function name.
        qualified_name: String,
        /// Ordered logical input types.
        arguments: Vec<TypeId>,
    },
    /// A registration name did not include exactly one schema qualifier.
    #[error("catalog registration requires a schema-qualified name, got '{name}'")]
    UnqualifiedName {
        /// Invalid supplied name.
        name: String,
    },
    /// PostgreSQL enums must have at least one label.
    #[error("enum '{qualified_name}' must have at least one label")]
    EmptyEnum {
        /// SQL-qualified enum name.
        qualified_name: String,
    },
    /// PostgreSQL enum labels must be unique.
    #[error("enum '{qualified_name}' contains duplicate labels")]
    DuplicateEnumVariant {
        /// SQL-qualified enum name.
        qualified_name: String,
    },
    /// Exact table functions need at least one result column.
    #[error("table function '{qualified_name}' must have at least one output column")]
    EmptyTableResult {
        /// SQL-qualified function name.
        qualified_name: String,
    },
}
