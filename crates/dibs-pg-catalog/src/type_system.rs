use crate::{
    ApiTypeId, CatalogError, CodecBinding, CollationId, PgCodecId, TypeId, WireCodecId,
    codec::{array_codec_for_registered, builtin_codec, enum_codec},
};

/// PostgreSQL type category represented by a stable logical identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PgTypeKind {
    /// Scalar base type.
    Base,
    /// PostgreSQL array type.
    Array,
    /// Registered enum type.
    Enum,
    /// Registered domain type.
    Domain,
}

impl PgTypeKind {
    pub(crate) const fn id_component(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Array => "array",
            Self::Enum => "enum",
            Self::Domain => "domain",
        }
    }
}

/// Versioned catalog type with separate storage, wire, and API identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogType {
    /// Stable logical type identity.
    pub id: TypeId,
    /// SQL-qualified canonical type name.
    pub qualified_name: String,
    /// Qualified internal `pg_type.typname` used by the live oracle.
    pub internal_qualified_name: String,
    /// PostgreSQL type kind.
    pub kind: PgTypeKind,
    /// Canonical type modifier, when this logical type fixes one.
    pub typmod: Option<String>,
    /// Array element type relationship.
    pub element_type: Option<TypeId>,
    /// Domain base type relationship.
    pub base_type: Option<TypeId>,
    /// Ordered enum labels for registered enum types.
    pub enum_variants: Vec<String>,
    /// Default collation identity, where applicable.
    pub collation: Option<CollationId>,
    /// PostgreSQL storage codec identity.
    pub pg_codec_id: PgCodecId,
    /// Client wire codec identity.
    pub wire_codec_id: WireCodecId,
    /// Rust generated API identity.
    pub rust_api_type: ApiTypeId,
    /// TypeScript generated API identity.
    pub typescript_api_type: ApiTypeId,
    /// Whether this row belongs to the curated PostgreSQL fixture.
    pub builtin: bool,
}

impl CatalogType {
    pub(crate) fn builtin(
        postgres_major: u16,
        qualified_name: &str,
        internal_name: &str,
        kind: PgTypeKind,
        element_type: Option<TypeId>,
    ) -> Result<Self, CatalogError> {
        let canonical_name = qualified_name
            .strip_prefix("pg_catalog.")
            .expect("builtin names are pg_catalog-qualified");
        let codec = builtin_codec(canonical_name)?;
        Ok(Self::with_codec(
            stable_type_id(
                postgres_major,
                qualified_name,
                kind,
                None,
                element_type.as_ref(),
                None::<&[&str]>,
            ),
            qualified_name,
            internal_name,
            kind,
            None,
            element_type,
            None,
            Vec::new(),
            default_collation(postgres_major, qualified_name),
            codec,
            true,
        ))
    }

    pub(crate) fn registered_enum(
        postgres_major: u16,
        qualified_name: &str,
        variants: Vec<String>,
    ) -> Self {
        let codec = enum_codec(qualified_name);
        Self::with_codec(
            stable_type_id(
                postgres_major,
                qualified_name,
                PgTypeKind::Enum,
                None,
                None,
                Some(variants.as_slice()),
            ),
            qualified_name,
            qualified_name,
            PgTypeKind::Enum,
            None,
            None,
            None,
            variants,
            default_collation(postgres_major, qualified_name),
            codec,
            false,
        )
    }

    pub(crate) fn registered_domain(
        postgres_major: u16,
        qualified_name: &str,
        base: &CatalogType,
    ) -> Self {
        let details = [base.id.as_str()];
        Self::with_codec(
            stable_type_id(
                postgres_major,
                qualified_name,
                PgTypeKind::Domain,
                None,
                None,
                Some(&details),
            ),
            qualified_name,
            qualified_name,
            PgTypeKind::Domain,
            None,
            None,
            Some(base.id.clone()),
            Vec::new(),
            base.collation.clone(),
            CodecBinding {
                pg_codec_id: PgCodecId::new(format!(
                    "pg{postgres_major}:pg-codec:domain<{}>",
                    base.pg_codec_id.as_str()
                )),
                wire_codec_id: base.wire_codec_id.clone(),
                rust_api_type: base.rust_api_type.clone(),
                typescript_api_type: base.typescript_api_type.clone(),
            },
            false,
        )
    }

    pub(crate) fn registered_array(
        postgres_major: u16,
        qualified_name: &str,
        element: &CatalogType,
    ) -> Self {
        Self::with_codec(
            stable_type_id(
                postgres_major,
                qualified_name,
                PgTypeKind::Array,
                None,
                Some(&element.id),
                None::<&[&str]>,
            ),
            qualified_name,
            qualified_name,
            PgTypeKind::Array,
            None,
            Some(element.id.clone()),
            None,
            Vec::new(),
            None,
            array_codec_for_registered(
                postgres_major,
                &CodecBinding {
                    pg_codec_id: element.pg_codec_id.clone(),
                    wire_codec_id: element.wire_codec_id.clone(),
                    rust_api_type: element.rust_api_type.clone(),
                    typescript_api_type: element.typescript_api_type.clone(),
                },
            ),
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn with_codec(
        id: TypeId,
        qualified_name: &str,
        internal_qualified_name: &str,
        kind: PgTypeKind,
        typmod: Option<String>,
        element_type: Option<TypeId>,
        base_type: Option<TypeId>,
        enum_variants: Vec<String>,
        collation: Option<CollationId>,
        codec: CodecBinding,
        builtin: bool,
    ) -> Self {
        Self {
            id,
            qualified_name: qualified_name.to_string(),
            internal_qualified_name: internal_qualified_name.to_string(),
            kind,
            typmod,
            element_type,
            base_type,
            enum_variants,
            collation,
            pg_codec_id: codec.pg_codec_id,
            wire_codec_id: codec.wire_codec_id,
            rust_api_type: codec.rust_api_type,
            typescript_api_type: codec.typescript_api_type,
            builtin,
        }
    }
}

/// Registration request for an application-defined PostgreSQL type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRegistration {
    /// SQL-qualified type name.
    pub qualified_name: String,
    /// Type relationship and codec derivation policy.
    pub kind: TypeRegistrationKind,
}

/// Supported application-defined PostgreSQL type kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRegistrationKind {
    /// Enum with an exact ordered list of labels.
    Enum {
        /// Labels in PostgreSQL sort order.
        variants: Vec<String>,
    },
    /// Domain whose lossless codecs are inherited from its base type.
    Domain {
        /// SQL-qualified base type name.
        base_type: String,
    },
    /// Array whose lossless codecs are derived from its element type.
    Array {
        /// SQL-qualified element type name.
        element_type: String,
    },
}

pub(crate) fn stable_type_id<T: AsRef<str>>(
    postgres_major: u16,
    qualified_name: &str,
    kind: PgTypeKind,
    typmod: Option<&str>,
    relationship: Option<&TypeId>,
    details: Option<&[T]>,
) -> TypeId {
    let mut value = format!(
        "pg{postgres_major}:type:{}:{qualified_name}",
        kind.id_component()
    );
    if let Some(typmod) = typmod {
        value.push_str(";typmod=");
        append_len_prefixed(&mut value, typmod);
    }
    if let Some(relationship) = relationship {
        value.push_str(";relationship=");
        append_len_prefixed(&mut value, relationship.as_str());
    }
    if let Some(details) = details {
        value.push_str(";details=");
        for detail in details {
            append_len_prefixed(&mut value, detail.as_ref());
        }
    }
    TypeId::new(value)
}

fn append_len_prefixed(output: &mut String, value: &str) {
    use std::fmt::Write as _;
    let _ = write!(output, "{}:{value};", value.len());
}

fn default_collation(postgres_major: u16, qualified_name: &str) -> Option<CollationId> {
    matches!(qualified_name, "pg_catalog.text" | "pg_catalog.text[]")
        .then(|| CollationId::new(format!("pg{postgres_major}:collation:pg_catalog.default")))
}
