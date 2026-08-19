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

/// One named PostgreSQL domain CHECK constraint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainConstraint {
    /// Constraint name.
    pub name: String,
    /// PostgreSQL CHECK expression over `VALUE`.
    pub expression: String,
}

/// How a domain obtains its collation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainCollation {
    /// Inherit the base type's applicable collation.
    Inherit,
    /// Use no collation; valid only when the base type is noncollatable.
    None,
    /// Use this exact collation; valid only when the base type is collatable.
    Explicit(CollationId),
}

/// Complete defining facts for one PostgreSQL domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainDefinition {
    /// Canonical base logical type.
    pub base_type: TypeId,
    /// Canonical base typmod, if fixed by the domain.
    pub base_typmod: Option<String>,
    /// Domain-level `NOT NULL` requirement.
    pub not_null: bool,
    /// Domain default expression.
    pub default: Option<String>,
    /// Domain collation policy supplied at registration.
    pub collation_policy: DomainCollation,
    /// Effective applicable domain collation.
    pub collation: Option<CollationId>,
    /// Named CHECK constraints in canonical PostgreSQL evaluation order.
    pub constraints: Vec<DomainConstraint>,
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
    /// Complete domain definition for domain types.
    pub domain: Option<DomainDefinition>,
    /// Ordered enum labels for registered enum types.
    pub enum_variants: Vec<String>,
    /// Default or applicable collation identity.
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
        mut domain: DomainDefinition,
    ) -> Self {
        domain.base_type = base.id.clone();
        domain.collation = match &domain.collation_policy {
            DomainCollation::Inherit => base.collation.clone(),
            DomainCollation::None => None,
            DomainCollation::Explicit(collation) => Some(collation.clone()),
        };
        let details = domain_identity_details(&domain);
        Self::with_codec(
            stable_type_id(
                postgres_major,
                qualified_name,
                PgTypeKind::Domain,
                domain.base_typmod.as_deref(),
                Some(&domain.base_type),
                Some(&details),
            ),
            qualified_name,
            qualified_name,
            PgTypeKind::Domain,
            domain.base_typmod.clone(),
            None,
            Some(domain.clone()),
            Vec::new(),
            domain.collation.clone(),
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
        domain: Option<DomainDefinition>,
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
            domain,
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
        /// SQL-qualified canonical base type name.
        base_type: String,
        /// Canonical base typmod fixed by the domain.
        base_typmod: Option<String>,
        /// Domain-level `NOT NULL` requirement.
        not_null: bool,
        /// Domain default expression.
        default: Option<String>,
        /// Applicable collation policy.
        collation: DomainCollation,
        /// Named CHECK constraints; canonicalized by name during registration.
        constraints: Vec<DomainConstraint>,
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

fn domain_identity_details(domain: &DomainDefinition) -> Vec<String> {
    let mut details = vec![format!("not-null={}", domain.not_null)];
    details.push(format!(
        "default={}",
        domain.default.as_deref().unwrap_or("<none>")
    ));
    details.push(format!("collation-policy={:?}", domain.collation_policy));
    details.push(format!(
        "collation={}",
        domain
            .collation
            .as_ref()
            .map_or("<none>", CollationId::as_str)
    ));
    for constraint in &domain.constraints {
        details.push(format!(
            "constraint:{}:{}:{}:{}",
            constraint.name.len(),
            constraint.name,
            constraint.expression.len(),
            constraint.expression
        ));
    }
    details
}

fn append_len_prefixed(output: &mut String, value: &str) {
    use std::fmt::Write as _;
    let _ = write!(output, "{}:{value};", value.len());
}

fn default_collation(postgres_major: u16, qualified_name: &str) -> Option<CollationId> {
    matches!(qualified_name, "pg_catalog.text" | "pg_catalog.text[]")
        .then(|| CollationId::new(format!("pg{postgres_major}:collation:pg_catalog.default")))
}
