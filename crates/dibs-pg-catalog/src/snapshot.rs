use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use dibs_db_schema::{
    CheckConstraint, Index, IndexColumn, NullsOrder, PgType, Schema, SortOrder,
    TriggerCheckConstraint,
};

use crate::{
    ApiLanguage, ApiTypeId, CallableId, CastId, CatalogCallable, CatalogError, CatalogType,
    CollationId, OperatorId, PgTypeKind, ScalarSignature, TableOutputColumn, TableSignature,
    TypeId, TypeRegistration, TypeRegistrationKind,
};

const POSTGRES_MAJOR: u16 = 18;

/// Deterministic fingerprint of Dibs schema truth.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaFingerprint(String);

impl SchemaFingerprint {
    /// Returns the lowercase BLAKE3 hex digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SchemaFingerprint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// SQL nullability retained independently from the logical type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Nullability {
    /// `NOT NULL` column or output.
    NotNull,
    /// SQL-nullable column or output.
    Nullable,
}

/// One application schema column in a catalog snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogColumn {
    /// Column name.
    pub name: String,
    /// Stable logical type identity.
    pub type_id: TypeId,
    /// SQL nullability.
    pub nullability: Nullability,
    /// Default SQL expression.
    pub default: Option<String>,
    /// Primary-key membership.
    pub primary_key: bool,
    /// Single-column unique constraint.
    pub unique: bool,
    /// Dibs auto-generation policy.
    pub auto_generated: bool,
}

/// Exact primary-key column sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryKey {
    /// Columns in key order.
    pub columns: Vec<String>,
}

/// Exact single- or multi-column unique constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueConstraint {
    /// Stable constraint name in the snapshot.
    pub name: String,
    /// Columns in constraint order.
    pub columns: Vec<String>,
}

/// Exact foreign-key relationship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogForeignKey {
    /// Local columns in key order.
    pub columns: Vec<String>,
    /// Qualified referenced table name.
    pub references_table: String,
    /// Referenced columns in key order.
    pub references_columns: Vec<String>,
}

/// Exact index column with PostgreSQL ordering facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogIndexColumn {
    /// Column name.
    pub name: String,
    /// Sort direction.
    pub order: SortOrder,
    /// Null ordering.
    pub nulls: NullsOrder,
}

/// Exact application index definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogIndex {
    /// Index name.
    pub name: String,
    /// Ordered index columns.
    pub columns: Vec<CatalogIndexColumn>,
    /// Uniqueness.
    pub unique: bool,
    /// Partial-index predicate.
    pub where_clause: Option<String>,
}

/// One application table in a catalog snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogTable {
    /// SQL-qualified table name.
    pub qualified_name: String,
    /// Ordered columns from Dibs schema truth.
    pub columns: Vec<CatalogColumn>,
    /// Exact primary key.
    pub primary_key: PrimaryKey,
    /// Exact unique constraints represented by Dibs column truth.
    pub unique_constraints: Vec<UniqueConstraint>,
    /// Check constraints.
    pub check_constraints: Vec<CheckConstraint>,
    /// Trigger-enforced checks.
    pub trigger_checks: Vec<TriggerCheckConstraint>,
    /// Foreign keys.
    pub foreign_keys: Vec<CatalogForeignKey>,
    /// Indexes.
    pub indexes: Vec<CatalogIndex>,
}

impl CatalogTable {
    /// Resolves a column by its exact name.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<&CatalogColumn> {
        self.columns.iter().find(|column| column.name == name)
    }
}

/// PostgreSQL cast context from `pg_cast.castcontext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CastContext {
    /// Implicit expression coercion.
    Implicit,
    /// Assignment-only coercion.
    Assignment,
    /// Explicit cast only.
    Explicit,
}

/// PostgreSQL cast implementation from `pg_cast.castmethod`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CastMethod {
    /// Cast function.
    Function,
    /// Binary-coercible cast.
    Binary,
    /// Text input/output cast.
    InOut,
}

/// Curated PostgreSQL operator signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogOperator {
    /// Stable logical operator identity.
    pub id: OperatorId,
    /// SQL-qualified operator name.
    pub qualified_name: String,
    /// Left operand type, absent for prefix operators.
    pub left: Option<TypeId>,
    /// Right operand type, absent for postfix operators.
    pub right: Option<TypeId>,
    /// Result type.
    pub result: TypeId,
    /// Whether this belongs to the PostgreSQL fixture.
    pub builtin: bool,
}

impl CatalogOperator {
    /// Renders the OID-independent signature returned by the live oracle query.
    pub fn live_signature(
        &self,
        catalog: &CatalogSnapshot,
    ) -> Result<(String, String, String, String), CatalogError> {
        let left = self
            .left
            .as_ref()
            .map(|id| render_type_id(catalog, id))
            .transpose()?
            .unwrap_or_default();
        let right = self
            .right
            .as_ref()
            .map(|id| render_type_id(catalog, id))
            .transpose()?
            .unwrap_or_default();
        let result = render_type_id(catalog, &self.result)?;
        Ok((self.qualified_name.clone(), left, right, result))
    }
}

/// Curated PostgreSQL cast signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogCast {
    /// Stable logical cast identity.
    pub id: CastId,
    /// Source type.
    pub source: TypeId,
    /// Target type.
    pub target: TypeId,
    /// Coercion context.
    pub context: CastContext,
    /// Cast implementation method.
    pub method: CastMethod,
    /// Whether this belongs to the PostgreSQL fixture.
    pub builtin: bool,
}

impl CatalogCast {
    /// Renders the OID-independent signature returned by the live oracle query.
    pub fn live_signature(
        &self,
        catalog: &CatalogSnapshot,
    ) -> Result<(String, String, String, String), CatalogError> {
        let source = render_type_id(catalog, &self.source)?;
        let target = render_type_id(catalog, &self.target)?;
        let context = match self.context {
            CastContext::Implicit => "i",
            CastContext::Assignment => "a",
            CastContext::Explicit => "e",
        };
        let method = match self.method {
            CastMethod::Function => "f",
            CastMethod::Binary => "b",
            CastMethod::InOut => "i",
        };
        Ok((source, target, context.to_string(), method.to_string()))
    }
}

/// Versioned PostgreSQL catalog plus application schema truth.
#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    /// PostgreSQL major version used in every logical identity.
    pub postgres_major: u16,
    /// Deterministic application schema fingerprint.
    pub schema_fingerprint: SchemaFingerprint,
    /// Curated and registered collation identities.
    pub collations: Vec<CollationId>,
    /// Curated and registered logical types.
    pub types: Vec<CatalogType>,
    /// Curated and registered functions.
    pub callables: Vec<CatalogCallable>,
    /// Curated operators.
    pub operators: Vec<CatalogOperator>,
    /// Curated casts.
    pub casts: Vec<CatalogCast>,
    /// Application tables converted from Dibs schema truth.
    pub tables: Vec<CatalogTable>,
}

impl CatalogSnapshot {
    /// Constructs the curated PostgreSQL 18 fixture without application tables.
    #[must_use]
    pub fn postgres_18_fixture() -> Self {
        Self::fixture().expect("curated PostgreSQL 18 catalog must be internally valid")
    }

    /// Constructs a PostgreSQL 18 snapshot from Dibs schema truth.
    pub fn from_schema_postgres_18(schema: &Schema) -> Result<Self, CatalogError> {
        let mut snapshot = Self::fixture()?;
        snapshot.add_schema(schema)?;
        Ok(snapshot)
    }

    /// Returns the current deterministic schema fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> &SchemaFingerprint {
        &self.schema_fingerprint
    }

    /// Resolves a type by its exact SQL-qualified canonical name.
    pub fn resolve_type(&self, qualified_name: &str) -> Result<&CatalogType, CatalogError> {
        self.types
            .iter()
            .find(|ty| ty.qualified_name == qualified_name)
            .ok_or_else(|| CatalogError::UnknownType {
                qualified_name: qualified_name.to_string(),
            })
    }

    /// Resolves a type by stable logical identity.
    #[must_use]
    pub fn type_by_id(&self, id: &TypeId) -> Option<&CatalogType> {
        self.types.iter().find(|ty| &ty.id == id)
    }

    /// Resolves a callable by stable logical identity.
    #[must_use]
    pub fn callable_by_id(&self, id: &CallableId) -> Option<&CatalogCallable> {
        self.callables.iter().find(|callable| &callable.id == id)
    }

    /// Resolves an application table by exact qualified name.
    pub fn resolve_table(&self, qualified_name: &str) -> Result<&CatalogTable, CatalogError> {
        self.tables
            .iter()
            .find(|table| table.qualified_name == qualified_name)
            .ok_or_else(|| CatalogError::UnknownTable {
                qualified_name: qualified_name.to_string(),
            })
    }

    /// Resolves a collation by exact stable identity.
    #[must_use]
    pub fn collation_by_id(&self, id: &CollationId) -> Option<&CollationId> {
        self.collations.iter().find(|collation| *collation == id)
    }

    /// Resolves an API type only when it identifies exactly one logical type.
    pub fn resolve_api_type(
        &self,
        language: ApiLanguage,
        api_type: impl Into<ApiTypeId>,
    ) -> Result<&CatalogType, CatalogError> {
        let api_type = api_type.into();
        let mut matches = self.types.iter().filter(|ty| match language {
            ApiLanguage::Rust => ty.rust_api_type == api_type,
            ApiLanguage::TypeScript => ty.typescript_api_type == api_type,
        });
        let first = matches
            .next()
            .ok_or_else(|| CatalogError::UnsupportedApiType {
                language,
                api_type: api_type.clone(),
            })?;
        if matches.next().is_some() {
            return Err(CatalogError::AmbiguousApiType { language, api_type });
        }
        Ok(first)
    }

    /// Registers one enum, domain, or array with lossless codec derivation.
    pub fn register_type(
        &mut self,
        registration: TypeRegistration,
    ) -> Result<TypeId, CatalogError> {
        validate_qualified_name(&registration.qualified_name)?;
        if self
            .types
            .iter()
            .any(|ty| ty.qualified_name == registration.qualified_name)
        {
            return Err(CatalogError::DuplicateType {
                qualified_name: registration.qualified_name,
            });
        }
        let ty = match registration.kind {
            TypeRegistrationKind::Enum { variants } => {
                if variants.is_empty() {
                    return Err(CatalogError::EmptyEnum {
                        qualified_name: registration.qualified_name,
                    });
                }
                let unique: BTreeSet<_> = variants.iter().collect();
                if unique.len() != variants.len() {
                    return Err(CatalogError::DuplicateEnumVariant {
                        qualified_name: registration.qualified_name,
                    });
                }
                CatalogType::registered_enum(
                    self.postgres_major,
                    &registration.qualified_name,
                    variants,
                )
            }
            TypeRegistrationKind::Domain {
                base_type,
                base_typmod,
                not_null,
                default,
                collation,
                constraints,
            } => {
                let base = self.resolve_type(&base_type)?.clone();
                let mut constraints = constraints;
                constraints.sort_by(|left, right| left.name.cmp(&right.name));
                for duplicate in constraints.windows(2) {
                    if duplicate[0].name == duplicate[1].name {
                        return Err(CatalogError::DuplicateDomainConstraintName {
                            qualified_name: registration.qualified_name,
                            constraint: duplicate[0].name.clone(),
                        });
                    }
                }
                let collation_valid = match &collation {
                    crate::DomainCollation::Inherit => true,
                    crate::DomainCollation::None => base.collation.is_none(),
                    crate::DomainCollation::Explicit(id) => {
                        if self.collation_by_id(id).is_none() {
                            return Err(CatalogError::UnknownCollation { id: id.clone() });
                        }
                        base.collation.is_some()
                    }
                };
                if !collation_valid {
                    return Err(CatalogError::InvalidDomainCollation {
                        qualified_name: registration.qualified_name,
                        base_type,
                    });
                }
                CatalogType::registered_domain(
                    self.postgres_major,
                    &registration.qualified_name,
                    &base,
                    crate::DomainDefinition {
                        base_type: base.id.clone(),
                        base_typmod,
                        not_null,
                        default,
                        collation_policy: collation,
                        collation: None,
                        constraints,
                    },
                )
            }
            TypeRegistrationKind::Array { element_type } => {
                let element = self.resolve_type(&element_type)?.clone();
                CatalogType::registered_array(
                    self.postgres_major,
                    &registration.qualified_name,
                    &element,
                )
            }
        };
        let id = ty.id.clone();
        self.types.push(ty);
        self.types
            .sort_by(|left, right| left.qualified_name.cmp(&right.qualified_name));
        self.refresh_fingerprint();
        Ok(id)
    }

    /// Registers an exact application scalar function.
    pub fn register_scalar(
        &mut self,
        signature: ScalarSignature,
    ) -> Result<CallableId, CatalogError> {
        validate_qualified_name(&signature.qualified_name)?;
        self.validate_type_ids(signature.arguments.iter())?;
        self.validate_type_ids(std::iter::once(&signature.result))?;
        let arguments = render_identity_arguments(self, &signature.arguments)?;
        let result = render_type_id(self, &signature.result)?;
        let callable =
            CatalogCallable::scalar(self.postgres_major, signature, arguments, result, false);
        self.insert_callable(callable)
    }

    /// Registers an exact application table function.
    pub fn register_table(
        &mut self,
        signature: TableSignature,
    ) -> Result<CallableId, CatalogError> {
        validate_qualified_name(&signature.qualified_name)?;
        if signature.columns.is_empty() {
            return Err(CatalogError::EmptyTableResult {
                qualified_name: signature.qualified_name,
            });
        }
        let mut output_names = BTreeSet::new();
        for column in &signature.columns {
            if !valid_identifier(&column.name) {
                return Err(CatalogError::InvalidOutputColumnName {
                    qualified_name: signature.qualified_name,
                    column: column.name.clone(),
                });
            }
            if !output_names.insert(column.name.as_str()) {
                return Err(CatalogError::DuplicateOutputColumnName {
                    qualified_name: signature.qualified_name,
                    column: column.name.clone(),
                });
            }
        }
        self.validate_type_ids(signature.arguments.iter())?;
        self.validate_type_ids(signature.columns.iter().map(|column| &column.type_id))?;
        let arguments = render_identity_arguments(self, &signature.arguments)?;
        let callable = CatalogCallable::table(
            self.postgres_major,
            signature,
            arguments,
            "record".to_string(),
            false,
        );
        self.insert_callable(callable)
    }

    /// Iterates the curated PostgreSQL types.
    pub fn builtin_types(&self) -> impl Iterator<Item = &CatalogType> {
        self.types.iter().filter(|ty| ty.builtin)
    }

    /// Iterates the curated PostgreSQL functions.
    pub fn builtin_callables(&self) -> impl Iterator<Item = &CatalogCallable> {
        self.callables.iter().filter(|callable| callable.builtin)
    }

    /// Iterates the curated PostgreSQL operators.
    pub fn builtin_operators(&self) -> impl Iterator<Item = &CatalogOperator> {
        self.operators.iter().filter(|operator| operator.builtin)
    }

    /// Iterates the curated PostgreSQL casts.
    pub fn builtin_casts(&self) -> impl Iterator<Item = &CatalogCast> {
        self.casts.iter().filter(|cast| cast.builtin)
    }

    /// Internal `pg_type.typname` values used by the live PostgreSQL oracle.
    #[must_use]
    pub fn live_type_internal_names(&self) -> Vec<&str> {
        self.builtin_types()
            .filter_map(|ty| {
                ty.internal_qualified_name
                    .split_once('.')
                    .map(|(_, name)| name)
            })
            .collect()
    }

    /// Function names used by the live PostgreSQL oracle.
    #[must_use]
    pub fn live_callable_names(&self) -> Vec<&str> {
        unique_unqualified_names(
            self.builtin_callables()
                .map(|item| item.qualified_name.as_str()),
        )
    }

    /// Operator names used by the live PostgreSQL oracle.
    #[must_use]
    pub fn live_operator_names(&self) -> Vec<&str> {
        unique_unqualified_names(
            self.builtin_operators()
                .map(|item| item.qualified_name.as_str()),
        )
    }

    /// Source type names used by the live PostgreSQL cast oracle.
    #[must_use]
    pub fn live_cast_source_names(&self) -> Vec<&str> {
        unique_display_names(self, self.builtin_casts().map(|cast| &cast.source))
    }

    /// Target type names used by the live PostgreSQL cast oracle.
    #[must_use]
    pub fn live_cast_target_names(&self) -> Vec<&str> {
        unique_display_names(self, self.builtin_casts().map(|cast| &cast.target))
    }

    fn fixture() -> Result<Self, CatalogError> {
        let mut snapshot = Self {
            postgres_major: POSTGRES_MAJOR,
            schema_fingerprint: empty_fingerprint(),
            collations: vec![CollationId::new(format!(
                "pg{}:collation:pg_catalog.default",
                POSTGRES_MAJOR
            ))],
            types: Vec::new(),
            callables: Vec::new(),
            operators: Vec::new(),
            casts: Vec::new(),
            tables: Vec::new(),
        };
        snapshot.install_builtin_types()?;
        snapshot.install_builtin_callables()?;
        snapshot.install_builtin_operators()?;
        snapshot.install_builtin_casts()?;
        snapshot.refresh_fingerprint();
        Ok(snapshot)
    }

    fn install_builtin_types(&mut self) -> Result<(), CatalogError> {
        const BASE_TYPES: &[(&str, &str)] = &[
            ("boolean", "bool"),
            ("smallint", "int2"),
            ("integer", "int4"),
            ("bigint", "int8"),
            ("real", "float4"),
            ("double precision", "float8"),
            ("numeric", "numeric"),
            ("text", "text"),
            ("bytea", "bytea"),
            ("uuid", "uuid"),
            ("date", "date"),
            ("time without time zone", "time"),
            ("timestamp without time zone", "timestamp"),
            ("timestamp with time zone", "timestamptz"),
            ("jsonb", "jsonb"),
        ];
        for (canonical, internal) in BASE_TYPES {
            self.types.push(CatalogType::builtin(
                self.postgres_major,
                &format!("pg_catalog.{canonical}"),
                &format!("pg_catalog.{internal}"),
                PgTypeKind::Base,
                None,
            )?);
        }
        const ARRAY_TYPES: &[(&str, &str, &str)] = &[
            ("boolean[]", "_bool", "boolean"),
            ("smallint[]", "_int2", "smallint"),
            ("integer[]", "_int4", "integer"),
            ("bigint[]", "_int8", "bigint"),
            ("numeric[]", "_numeric", "numeric"),
            ("text[]", "_text", "text"),
            ("bytea[]", "_bytea", "bytea"),
            ("uuid[]", "_uuid", "uuid"),
            ("date[]", "_date", "date"),
            (
                "time without time zone[]",
                "_time",
                "time without time zone",
            ),
            (
                "timestamp without time zone[]",
                "_timestamp",
                "timestamp without time zone",
            ),
            (
                "timestamp with time zone[]",
                "_timestamptz",
                "timestamp with time zone",
            ),
            ("jsonb[]", "_jsonb", "jsonb"),
        ];
        for (canonical, internal, element) in ARRAY_TYPES {
            let element = self
                .resolve_type(&format!("pg_catalog.{element}"))?
                .id
                .clone();
            self.types.push(CatalogType::builtin(
                self.postgres_major,
                &format!("pg_catalog.{canonical}"),
                &format!("pg_catalog.{internal}"),
                PgTypeKind::Array,
                Some(element),
            )?);
        }
        self.types
            .sort_by(|left, right| left.qualified_name.cmp(&right.qualified_name));
        Ok(())
    }

    fn install_builtin_callables(&mut self) -> Result<(), CatalogError> {
        self.add_builtin_scalar("pg_catalog.abs", &["bigint"], "bigint")?;
        self.add_builtin_scalar("pg_catalog.lower", &["text"], "text")?;
        self.add_builtin_scalar("pg_catalog.length", &["text"], "integer")?;
        self.add_builtin_scalar("pg_catalog.jsonb_array_length", &["jsonb"], "integer")?;
        self.add_builtin_table(
            "pg_catalog.generate_series",
            &["bigint", "bigint"],
            "bigint",
            "bigint",
        )?;
        Ok(())
    }

    fn add_builtin_scalar(
        &mut self,
        qualified_name: &str,
        argument_names: &[&str],
        result_name: &str,
    ) -> Result<(), CatalogError> {
        let arguments = argument_names
            .iter()
            .map(|name| {
                self.resolve_type(&format!("pg_catalog.{name}"))
                    .map(|ty| ty.id.clone())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result = self
            .resolve_type(&format!("pg_catalog.{result_name}"))?
            .id
            .clone();
        let signature = ScalarSignature {
            qualified_name: qualified_name.to_string(),
            arguments,
            result,
        };
        self.callables.push(CatalogCallable::scalar(
            self.postgres_major,
            signature,
            argument_names.join(", "),
            result_name.to_string(),
            true,
        ));
        Ok(())
    }

    fn add_builtin_table(
        &mut self,
        qualified_name: &str,
        argument_names: &[&str],
        output_name: &str,
        postgres_result: &str,
    ) -> Result<(), CatalogError> {
        let arguments = argument_names
            .iter()
            .map(|name| {
                self.resolve_type(&format!("pg_catalog.{name}"))
                    .map(|ty| ty.id.clone())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let output_type = self
            .resolve_type(&format!("pg_catalog.{output_name}"))?
            .id
            .clone();
        let signature = TableSignature {
            qualified_name: qualified_name.to_string(),
            arguments,
            columns: vec![TableOutputColumn {
                name: unqualified_name(qualified_name).to_string(),
                type_id: output_type,
                nullability: Nullability::NotNull,
            }],
        };
        self.callables.push(CatalogCallable::table(
            self.postgres_major,
            signature,
            argument_names.join(", "),
            postgres_result.to_string(),
            true,
        ));
        Ok(())
    }

    fn install_builtin_operators(&mut self) -> Result<(), CatalogError> {
        self.add_operator("=", "bigint", "bigint", "boolean")?;
        self.add_operator("+", "bigint", "bigint", "bigint")?;
        self.add_operator("||", "text", "text", "text")?;
        self.add_operator("@>", "jsonb", "jsonb", "boolean")?;
        Ok(())
    }

    fn add_operator(
        &mut self,
        name: &str,
        left_name: &str,
        right_name: &str,
        result_name: &str,
    ) -> Result<(), CatalogError> {
        let left = self
            .resolve_type(&format!("pg_catalog.{left_name}"))?
            .id
            .clone();
        let right = self
            .resolve_type(&format!("pg_catalog.{right_name}"))?
            .id
            .clone();
        let result = self
            .resolve_type(&format!("pg_catalog.{result_name}"))?
            .id
            .clone();
        let qualified_name = format!("pg_catalog.{name}");
        let id = OperatorId::new(format!(
            "pg{}:operator:{qualified_name}({},{})/{}",
            self.postgres_major,
            left.as_str(),
            right.as_str(),
            result.as_str()
        ));
        self.operators.push(CatalogOperator {
            id,
            qualified_name,
            left: Some(left),
            right: Some(right),
            result,
            builtin: true,
        });
        Ok(())
    }

    fn install_builtin_casts(&mut self) -> Result<(), CatalogError> {
        self.add_cast(
            "smallint",
            "integer",
            CastContext::Implicit,
            CastMethod::Function,
        )?;
        self.add_cast(
            "integer",
            "bigint",
            CastContext::Implicit,
            CastMethod::Function,
        )?;
        self.add_cast(
            "bigint",
            "numeric",
            CastContext::Implicit,
            CastMethod::Function,
        )?;
        self.add_cast(
            "bigint",
            "integer",
            CastContext::Assignment,
            CastMethod::Function,
        )?;
        Ok(())
    }

    fn add_cast(
        &mut self,
        source_name: &str,
        target_name: &str,
        context: CastContext,
        method: CastMethod,
    ) -> Result<(), CatalogError> {
        let source = self
            .resolve_type(&format!("pg_catalog.{source_name}"))?
            .id
            .clone();
        let target = self
            .resolve_type(&format!("pg_catalog.{target_name}"))?
            .id
            .clone();
        let id = CastId::new(format!(
            "pg{}:cast:{}->{}:{context:?}:{method:?}",
            self.postgres_major,
            source.as_str(),
            target.as_str()
        ));
        self.casts.push(CatalogCast {
            id,
            source,
            target,
            context,
            method,
            builtin: true,
        });
        Ok(())
    }

    fn add_schema(&mut self, schema: &Schema) -> Result<(), CatalogError> {
        let mut table_names: Vec<_> = schema.tables.keys().cloned().collect();
        table_names.sort();
        for table_name in table_names {
            let table = schema
                .tables
                .get(&table_name)
                .expect("table name came from schema map");
            let mut columns = Vec::with_capacity(table.columns.len());
            for column in &table.columns {
                let type_id = self.resolve_type(pg_type_name(column.pg_type))?.id.clone();
                columns.push(CatalogColumn {
                    name: column.name.clone(),
                    type_id,
                    nullability: if column.nullable {
                        Nullability::Nullable
                    } else {
                        Nullability::NotNull
                    },
                    default: column.default.clone(),
                    primary_key: column.primary_key,
                    unique: column.unique,
                    auto_generated: column.auto_generated,
                });
            }
            let primary_key = PrimaryKey {
                columns: table
                    .columns
                    .iter()
                    .filter(|column| column.primary_key)
                    .map(|column| column.name.clone())
                    .collect(),
            };
            let unique_constraints = table
                .columns
                .iter()
                .filter(|column| column.unique && !column.primary_key)
                .map(|column| UniqueConstraint {
                    name: format!("uq_{}_{}", table.name, column.name),
                    columns: vec![column.name.clone()],
                })
                .collect();
            let foreign_keys = table
                .foreign_keys
                .iter()
                .map(|foreign_key| CatalogForeignKey {
                    columns: foreign_key.columns.clone(),
                    references_table: qualify_public(&foreign_key.references_table),
                    references_columns: foreign_key.references_columns.clone(),
                })
                .collect();
            let indexes = table.indices.iter().map(convert_index).collect();
            self.tables.push(CatalogTable {
                qualified_name: qualify_public(&table.name),
                columns,
                primary_key,
                unique_constraints,
                check_constraints: table.check_constraints.clone(),
                trigger_checks: table.trigger_checks.clone(),
                foreign_keys,
                indexes,
            });
        }
        self.tables
            .sort_by(|left, right| left.qualified_name.cmp(&right.qualified_name));
        self.refresh_fingerprint();
        Ok(())
    }

    fn validate_type_ids<'a>(
        &self,
        types: impl IntoIterator<Item = &'a TypeId>,
    ) -> Result<(), CatalogError> {
        for type_id in types {
            if self.type_by_id(type_id).is_none() {
                return Err(CatalogError::UnknownTypeId {
                    id: type_id.clone(),
                });
            }
        }
        Ok(())
    }

    fn insert_callable(&mut self, callable: CatalogCallable) -> Result<CallableId, CatalogError> {
        if self.callables.iter().any(|existing| {
            existing.qualified_name == callable.qualified_name
                && existing.arguments == callable.arguments
        }) {
            return Err(CatalogError::DuplicateCallableSignature {
                qualified_name: callable.qualified_name,
                arguments: callable.arguments,
            });
        }
        let id = callable.id.clone();
        self.callables.push(callable);
        self.callables.sort_by(|left, right| left.id.cmp(&right.id));
        self.refresh_fingerprint();
        Ok(id)
    }

    fn refresh_fingerprint(&mut self) {
        self.schema_fingerprint = fingerprint_snapshot(self);
    }
}

fn pg_type_name(pg_type: PgType) -> &'static str {
    match pg_type {
        PgType::SmallInt => "pg_catalog.smallint",
        PgType::Integer => "pg_catalog.integer",
        PgType::BigInt => "pg_catalog.bigint",
        PgType::Real => "pg_catalog.real",
        PgType::DoublePrecision => "pg_catalog.double precision",
        PgType::Numeric => "pg_catalog.numeric",
        PgType::Boolean => "pg_catalog.boolean",
        PgType::Text => "pg_catalog.text",
        PgType::Bytea => "pg_catalog.bytea",
        PgType::Timestamptz => "pg_catalog.timestamp with time zone",
        PgType::Date => "pg_catalog.date",
        PgType::Time => "pg_catalog.time without time zone",
        PgType::Uuid => "pg_catalog.uuid",
        PgType::Jsonb => "pg_catalog.jsonb",
        PgType::TextArray => "pg_catalog.text[]",
        PgType::BigIntArray => "pg_catalog.bigint[]",
        PgType::IntegerArray => "pg_catalog.integer[]",
    }
}

fn convert_index(index: &Index) -> CatalogIndex {
    CatalogIndex {
        name: index.name.clone(),
        columns: index.columns.iter().map(convert_index_column).collect(),
        unique: index.unique,
        where_clause: index.where_clause.clone(),
    }
}

fn convert_index_column(column: &IndexColumn) -> CatalogIndexColumn {
    CatalogIndexColumn {
        name: column.name.clone(),
        order: column.order,
        nulls: column.nulls,
    }
}

fn qualify_public(name: &str) -> String {
    if name.contains('.') {
        name.to_string()
    } else {
        format!("public.{name}")
    }
}

fn validate_qualified_name(qualified_name: &str) -> Result<(), CatalogError> {
    let valid = qualified_name
        .split_once('.')
        .is_some_and(|(schema, name)| {
            !name.contains('.') && valid_identifier(schema) && valid_identifier(name)
        });
    if valid {
        Ok(())
    } else {
        Err(CatalogError::UnqualifiedName {
            name: qualified_name.to_string(),
        })
    }
}

fn valid_identifier(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_lowercase())
        && characters.all(|character| {
            character == '_'
                || character == '$'
                || character.is_ascii_lowercase()
                || character.is_ascii_digit()
        })
}

fn render_identity_arguments(
    catalog: &CatalogSnapshot,
    arguments: &[TypeId],
) -> Result<String, CatalogError> {
    arguments
        .iter()
        .map(|type_id| render_type_id(catalog, type_id))
        .collect::<Result<Vec<_>, _>>()
        .map(|arguments| arguments.join(", "))
}

fn render_type_id(catalog: &CatalogSnapshot, type_id: &TypeId) -> Result<String, CatalogError> {
    catalog
        .type_by_id(type_id)
        .map(|ty| postgres_display_name(&ty.qualified_name).to_string())
        .ok_or_else(|| CatalogError::UnknownTypeId {
            id: type_id.clone(),
        })
}

fn postgres_display_name(qualified_name: &str) -> &str {
    qualified_name
        .strip_prefix("pg_catalog.")
        .unwrap_or(qualified_name)
}

fn unqualified_name(qualified_name: &str) -> &str {
    qualified_name
        .split_once('.')
        .map_or(qualified_name, |(_, name)| name)
}

fn unique_unqualified_names<'a>(values: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    values
        .map(unqualified_name)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn unique_display_names<'a>(
    catalog: &'a CatalogSnapshot,
    ids: impl Iterator<Item = &'a TypeId>,
) -> Vec<&'a str> {
    ids.filter_map(|id| catalog.type_by_id(id))
        .map(|ty| postgres_display_name(&ty.qualified_name))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn empty_fingerprint() -> SchemaFingerprint {
    SchemaFingerprint(blake3::hash(b"").to_hex().to_string())
}

fn fingerprint_snapshot(snapshot: &CatalogSnapshot) -> SchemaFingerprint {
    let mut canonical = String::new();
    let _ = writeln!(canonical, "postgres-major:{}", snapshot.postgres_major);

    let mut types: Vec<_> = snapshot.types.iter().filter(|ty| !ty.builtin).collect();
    types.sort_by(|left, right| left.id.cmp(&right.id));
    for ty in types {
        write_value(&mut canonical, "type-id", ty.id.as_str());
        write_value(&mut canonical, "type-name", &ty.qualified_name);
        let _ = writeln!(canonical, "type-kind:{:?}", ty.kind);
        write_optional(&mut canonical, "type-typmod", ty.typmod.as_deref());
        write_optional(
            &mut canonical,
            "type-element",
            ty.element_type.as_ref().map(TypeId::as_str),
        );
        if let Some(domain) = &ty.domain {
            write_value(
                &mut canonical,
                "type-domain-base",
                domain.base_type.as_str(),
            );
            write_optional(
                &mut canonical,
                "type-domain-typmod",
                domain.base_typmod.as_deref(),
            );
            let _ = writeln!(canonical, "type-domain-not-null:{}", domain.not_null);
            write_optional(
                &mut canonical,
                "type-domain-default",
                domain.default.as_deref(),
            );
            let _ = writeln!(
                canonical,
                "type-domain-collation-policy:{:?}",
                domain.collation_policy
            );
            write_optional(
                &mut canonical,
                "type-domain-collation",
                domain.collation.as_ref().map(crate::CollationId::as_str),
            );
            for constraint in &domain.constraints {
                write_value(&mut canonical, "type-domain-check-name", &constraint.name);
                write_value(
                    &mut canonical,
                    "type-domain-check-expression",
                    &constraint.expression,
                );
            }
        }
        for variant in &ty.enum_variants {
            write_value(&mut canonical, "type-enum", variant);
        }
        write_value(&mut canonical, "type-pg-codec", ty.pg_codec_id.as_str());
        write_value(&mut canonical, "type-wire-codec", ty.wire_codec_id.as_str());
        write_value(&mut canonical, "type-rust-api", ty.rust_api_type.as_str());
        write_value(
            &mut canonical,
            "type-typescript-api",
            ty.typescript_api_type.as_str(),
        );
    }

    let mut callables: Vec<_> = snapshot
        .callables
        .iter()
        .filter(|callable| !callable.builtin)
        .collect();
    callables.sort_by(|left, right| left.id.cmp(&right.id));
    for callable in callables {
        write_value(&mut canonical, "callable-id", callable.id.as_str());
        write_value(&mut canonical, "callable-name", &callable.qualified_name);
        let _ = writeln!(canonical, "callable-kind:{:?}", callable.kind);
        for argument in &callable.arguments {
            write_value(&mut canonical, "callable-argument", argument.as_str());
        }
        write_optional(
            &mut canonical,
            "callable-scalar-result",
            callable.scalar_result.as_ref().map(TypeId::as_str),
        );
        for column in &callable.table_columns {
            write_value(&mut canonical, "callable-column-name", &column.name);
            write_value(
                &mut canonical,
                "callable-column-type",
                column.type_id.as_str(),
            );
            let _ = writeln!(
                canonical,
                "callable-column-nullability:{:?}",
                column.nullability
            );
        }
        write_value(
            &mut canonical,
            "callable-postgres-arguments",
            &callable.postgres_identity_arguments,
        );
        write_value(
            &mut canonical,
            "callable-postgres-result",
            &callable.postgres_result_type,
        );
    }

    let tables: BTreeMap<_, _> = snapshot
        .tables
        .iter()
        .map(|table| (table.qualified_name.as_str(), table))
        .collect();
    for (name, table) in tables {
        write_value(&mut canonical, "table", name);
        for column in &table.columns {
            write_value(&mut canonical, "column", &column.name);
            write_value(&mut canonical, "column-type", column.type_id.as_str());
            let _ = writeln!(canonical, "column-nullability:{:?}", column.nullability);
            write_optional(&mut canonical, "column-default", column.default.as_deref());
            let _ = writeln!(canonical, "column-pk:{}", column.primary_key);
            let _ = writeln!(canonical, "column-unique:{}", column.unique);
            let _ = writeln!(canonical, "column-generated:{}", column.auto_generated);
        }
        for column in &table.primary_key.columns {
            write_value(&mut canonical, "primary-key", column);
        }
        let mut unique_constraints = table.unique_constraints.clone();
        unique_constraints.sort_by(|left, right| left.name.cmp(&right.name));
        for constraint in unique_constraints {
            write_value(&mut canonical, "unique", &constraint.name);
            for column in constraint.columns {
                write_value(&mut canonical, "unique-column", &column);
            }
        }
        let mut checks = table.check_constraints.clone();
        checks.sort_by(|left, right| left.name.cmp(&right.name));
        for check in checks {
            write_value(&mut canonical, "check-name", &check.name);
            write_value(&mut canonical, "check-expr", &check.expr);
        }
        let mut triggers = table.trigger_checks.clone();
        triggers.sort_by(|left, right| left.name.cmp(&right.name));
        for trigger in triggers {
            write_value(&mut canonical, "trigger-name", &trigger.name);
            write_value(&mut canonical, "trigger-expr", &trigger.expr);
            write_optional(
                &mut canonical,
                "trigger-message",
                trigger.message.as_deref(),
            );
        }
        let mut foreign_keys = table.foreign_keys.clone();
        foreign_keys.sort_by(|left, right| {
            (
                &left.references_table,
                &left.columns,
                &left.references_columns,
            )
                .cmp(&(
                    &right.references_table,
                    &right.columns,
                    &right.references_columns,
                ))
        });
        for foreign_key in foreign_keys {
            for column in foreign_key.columns {
                write_value(&mut canonical, "fk-column", &column);
            }
            write_value(
                &mut canonical,
                "fk-references-table",
                &foreign_key.references_table,
            );
            for column in foreign_key.references_columns {
                write_value(&mut canonical, "fk-references-column", &column);
            }
        }
        let mut indexes = table.indexes.clone();
        indexes.sort_by(|left, right| left.name.cmp(&right.name));
        for index in indexes {
            write_value(&mut canonical, "index", &index.name);
            let _ = writeln!(canonical, "index-unique:{}", index.unique);
            write_optional(&mut canonical, "index-where", index.where_clause.as_deref());
            for column in index.columns {
                write_value(&mut canonical, "index-column", &column.name);
                let _ = writeln!(canonical, "index-order:{:?}", column.order);
                let _ = writeln!(canonical, "index-nulls:{:?}", column.nulls);
            }
        }
    }

    SchemaFingerprint(blake3::hash(canonical.as_bytes()).to_hex().to_string())
}

fn write_value(output: &mut String, key: &str, value: &str) {
    let _ = writeln!(output, "{key}:{}:{value}", value.len());
}

fn write_optional(output: &mut String, key: &str, value: Option<&str>) {
    match value {
        Some(value) => write_value(output, key, value),
        None => {
            let _ = writeln!(output, "{key}:none");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_includes_callable_oracle_facts() {
        let mut first = CatalogSnapshot::postgres_18_fixture();
        let bigint = first.resolve_type("pg_catalog.bigint").unwrap().id.clone();
        let callable_id = first
            .register_scalar(ScalarSignature {
                qualified_name: "app.identity".to_string(),
                arguments: vec![bigint.clone()],
                result: bigint,
            })
            .unwrap();
        let mut second = first.clone();
        second
            .callables
            .iter_mut()
            .find(|callable| callable.id == callable_id)
            .unwrap()
            .postgres_result_type = "text".to_string();
        second.refresh_fingerprint();

        assert_ne!(first.fingerprint(), second.fingerprint());
    }
}
