//! Database schema types for dibs.
//!
//! This crate contains the core schema types that are shared between
//! `dibs` (schema introspection) and `dibs-query-gen` (query planning).

use std::fmt;

use indexmap::IndexMap;

/// Postgres column types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgType {
    /// SMALLINT (2 bytes)
    SmallInt,
    /// INTEGER (4 bytes)
    Integer,
    /// BIGINT (8 bytes)
    BigInt,
    /// REAL (4 bytes floating point)
    Real,
    /// DOUBLE PRECISION (8 bytes floating point)
    DoublePrecision,
    /// NUMERIC (arbitrary precision)
    Numeric,
    /// BOOLEAN
    Boolean,
    /// TEXT
    Text,
    /// BYTEA (binary)
    Bytea,
    /// TIMESTAMPTZ
    Timestamptz,
    /// DATE
    Date,
    /// TIME
    Time,
    /// UUID
    Uuid,
    /// JSONB
    Jsonb,
    /// TEXT[] (array of text)
    TextArray,
    /// BIGINT[] (array of bigint)
    BigIntArray,
    /// INTEGER[] (array of integer)
    IntegerArray,
}

impl PgType {
    /// Map this Postgres type to a Rust type string.
    ///
    /// These names match what's exported in `dibs_runtime::prelude`.
    pub fn to_rust_type(&self) -> &'static str {
        match self {
            PgType::SmallInt => "i16",
            PgType::Integer => "i32",
            PgType::BigInt => "i64",
            PgType::Real => "f32",
            PgType::DoublePrecision => "f64",
            PgType::Numeric => "Decimal",
            PgType::Boolean => "bool",
            PgType::Text => "String",
            PgType::Bytea => "Vec<u8>",
            PgType::Timestamptz => "Timestamp",
            PgType::Date => "Date",
            PgType::Time => "Time",
            PgType::Uuid => "Uuid",
            PgType::Jsonb => "JsonValue",
            PgType::TextArray => "Vec<String>",
            PgType::BigIntArray => "Vec<i64>",
            PgType::IntegerArray => "Vec<i32>",
        }
    }
}

impl fmt::Display for PgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgType::SmallInt => write!(f, "SMALLINT"),
            PgType::Integer => write!(f, "INTEGER"),
            PgType::BigInt => write!(f, "BIGINT"),
            PgType::Real => write!(f, "REAL"),
            PgType::DoublePrecision => write!(f, "DOUBLE PRECISION"),
            PgType::Numeric => write!(f, "NUMERIC"),
            PgType::Boolean => write!(f, "BOOLEAN"),
            PgType::Text => write!(f, "TEXT"),
            PgType::Bytea => write!(f, "BYTEA"),
            PgType::Timestamptz => write!(f, "TIMESTAMPTZ"),
            PgType::Date => write!(f, "DATE"),
            PgType::Time => write!(f, "TIME"),
            PgType::Uuid => write!(f, "UUID"),
            PgType::Jsonb => write!(f, "JSONB"),
            PgType::TextArray => write!(f, "TEXT[]"),
            PgType::BigIntArray => write!(f, "BIGINT[]"),
            PgType::IntegerArray => write!(f, "INTEGER[]"),
        }
    }
}

/// A database column definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    /// Column name
    pub name: String,
    /// Postgres type
    pub pg_type: PgType,
    /// Rust type name (if known, e.g., from reflection)
    pub rust_type: Option<String>,
    /// Whether the column allows NULL
    pub nullable: bool,
    /// Default value expression (if any)
    pub default: Option<String>,
    /// Whether this is a primary key
    pub primary_key: bool,
    /// Whether this has a unique constraint
    pub unique: bool,
    /// Whether this column is auto-generated (serial, identity, uuid default, etc.)
    pub auto_generated: bool,
    /// Whether this is a long text field (use textarea)
    pub long: bool,
    /// Whether this column should be used as the display label
    pub label: bool,
    /// Enum variants (if this is an enum type)
    pub enum_variants: Vec<String>,
    /// Doc comment (if any)
    pub doc: Option<String>,
    /// Language/format for code editor (e.g., "markdown", "json")
    pub lang: Option<String>,
    /// Lucide icon name for display in admin UI (explicit or derived from subtype)
    pub icon: Option<String>,
    /// Semantic subtype of the column (e.g., "email", "url", "password")
    pub subtype: Option<String>,
}

/// A foreign key constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForeignKey {
    /// Column(s) in this table
    pub columns: Vec<String>,
    /// Referenced table
    pub references_table: String,
    /// Referenced column(s)
    pub references_columns: Vec<String>,
}

/// Sort order for index columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    /// Ascending order (default)
    #[default]
    Asc,
    /// Descending order
    Desc,
}

impl SortOrder {
    /// Returns the SQL keyword for this sort order, or empty string for ASC (default).
    pub fn to_sql(&self) -> &'static str {
        match self {
            SortOrder::Asc => "",
            SortOrder::Desc => " DESC",
        }
    }
}

/// Nulls ordering for index columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NullsOrder {
    /// Use database default (NULLS LAST for ASC, NULLS FIRST for DESC)
    #[default]
    Default,
    /// Sort nulls before non-null values
    First,
    /// Sort nulls after non-null values
    Last,
}

impl NullsOrder {
    /// Returns the SQL clause for this nulls ordering, or empty string for default.
    pub fn to_sql(&self) -> &'static str {
        match self {
            NullsOrder::Default => "",
            NullsOrder::First => " NULLS FIRST",
            NullsOrder::Last => " NULLS LAST",
        }
    }
}

/// A column in an index with optional sort order and nulls ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexColumn {
    /// Column name
    pub name: String,
    /// Sort order (ASC or DESC)
    pub order: SortOrder,
    /// Nulls ordering (NULLS FIRST, NULLS LAST, or default)
    pub nulls: NullsOrder,
}

impl IndexColumn {
    /// Create a new index column with default (ASC) ordering and default nulls.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: SortOrder::Asc,
            nulls: NullsOrder::Default,
        }
    }

    /// Create a new index column with DESC ordering and default nulls.
    pub fn desc(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: SortOrder::Desc,
            nulls: NullsOrder::Default,
        }
    }

    /// Create a new index column with NULLS FIRST ordering.
    pub fn nulls_first(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: SortOrder::Asc,
            nulls: NullsOrder::First,
        }
    }

    /// Returns the SQL fragment for this column (name + order + nulls).
    pub fn to_sql(&self, quote_ident: impl Fn(&str) -> String) -> String {
        format!(
            "{}{}{}",
            quote_ident(&self.name),
            self.order.to_sql(),
            self.nulls.to_sql()
        )
    }

    /// Parse a column specification like "col_name", "col_name DESC", or "col_name DESC NULLS FIRST".
    pub fn parse(spec: &str) -> Self {
        let spec = spec.trim();
        let upper = spec.to_uppercase();

        // Parse nulls ordering first (it comes at the end)
        let (spec_without_nulls, nulls) = if upper.ends_with(" NULLS FIRST") {
            (&spec[..spec.len() - 12], NullsOrder::First)
        } else if upper.ends_with(" NULLS LAST") {
            (&spec[..spec.len() - 11], NullsOrder::Last)
        } else {
            (spec, NullsOrder::Default)
        };

        let trimmed = spec_without_nulls.trim();
        let upper_trimmed = trimmed.to_uppercase();

        // Parse sort order
        let (name, order) = if upper_trimmed.ends_with(" DESC") {
            (
                trimmed[..trimmed.len() - 5].trim().to_string(),
                SortOrder::Desc,
            )
        } else if upper_trimmed.ends_with(" ASC") {
            (
                trimmed[..trimmed.len() - 4].trim().to_string(),
                SortOrder::Asc,
            )
        } else {
            (trimmed.to_string(), SortOrder::Asc)
        };

        fn unquote_pg_ident_if_quoted(s: &str) -> String {
            let s = s.trim();
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                let inner = &s[1..s.len() - 1];
                return inner.replace("\"\"", "\"");
            }
            s.to_string()
        }

        Self {
            name: unquote_pg_ident_if_quoted(&name),
            order,
            nulls,
        }
    }
}

/// A database index.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    /// Index name
    pub name: String,
    /// Column(s) in the index with sort order
    pub columns: Vec<IndexColumn>,
    /// Whether this is a unique index
    pub unique: bool,
    /// Optional WHERE clause for partial indexes (PostgreSQL-specific)
    pub where_clause: Option<String>,
}

/// Source location of a schema element.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceLocation {
    /// Source file path
    pub file: Option<String>,
    /// Line number (1-indexed)
    pub line: Option<u32>,
    /// Column number (1-indexed)
    pub column: Option<u32>,
}

impl SourceLocation {
    /// Check if we have any source location info.
    pub fn is_known(&self) -> bool {
        self.file.is_some()
    }

    /// Format as "file:line" or "file:line:column"
    pub fn to_string_short(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        match (self.line, self.column) {
            (Some(line), Some(col)) => Some(format!("{}:{}:{}", file, line, col)),
            (Some(line), None) => Some(format!("{}:{}", file, line)),
            _ => Some(file.clone()),
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_string_short() {
            Some(s) => write!(f, "{}", s),
            None => write!(f, "<unknown>"),
        }
    }
}

/// A table CHECK constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckConstraint {
    pub name: String,
    pub expr: String,
}

/// A trigger-enforced invariant check (BEFORE INSERT OR UPDATE).
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerCheckConstraint {
    pub name: String,
    pub expr: String,
    pub message: Option<String>,
}

/// A database table definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Table name
    pub name: String,
    /// Columns
    pub columns: Vec<Column>,
    /// CHECK constraints
    pub check_constraints: Vec<CheckConstraint>,
    /// Trigger-enforced checks
    pub trigger_checks: Vec<TriggerCheckConstraint>,
    /// Foreign keys
    pub foreign_keys: Vec<ForeignKey>,
    /// Indices
    pub indices: Vec<Index>,
    /// Source location of the Rust struct
    pub source: SourceLocation,
    /// Doc comment from the Rust struct
    pub doc: Option<String>,
    /// Lucide icon name for display in admin UI
    pub icon: Option<String>,
}

/// A complete database schema.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// Tables in the schema, indexed by name
    pub tables: IndexMap<String, Table>,
}

impl Schema {
    /// Create a new empty schema.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a table by name.
    pub fn get_table(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Iterate over all tables.
    pub fn iter_tables(&self) -> impl Iterator<Item = &Table> {
        self.tables.values()
    }
}
