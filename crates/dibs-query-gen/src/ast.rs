//! Query AST types.
//!
//! These represent the semantic structure of a query after parsing from styx.

use styx_parse::Span;

/// A file containing multiple queries and mutations.
#[derive(Debug, Clone)]
pub struct QueryFile {
    pub queries: Vec<Query>,
    pub inserts: Vec<InsertMutation>,
    pub insert_manys: Vec<InsertManyMutation>,
    pub upserts: Vec<UpsertMutation>,
    pub upsert_manys: Vec<UpsertManyMutation>,
    pub updates: Vec<UpdateMutation>,
    pub deletes: Vec<DeleteMutation>,
}

/// A single query definition.
#[derive(Debug, Clone)]
pub struct Query {
    /// Query name (e.g., "ProductListing").
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Query parameters.
    pub params: Vec<Param>,
    /// Root table to query from.
    pub from: String,
    /// WHERE filters.
    pub filters: Vec<Filter>,
    /// ORDER BY clauses.
    pub order_by: Vec<OrderBy>,
    /// LIMIT clause.
    pub limit: Option<Expr>,
    /// OFFSET clause.
    pub offset: Option<Expr>,
    /// Whether to return first row only (vs Vec).
    pub first: bool,
    /// Whether to use DISTINCT to return only unique rows.
    pub distinct: bool,
    /// DISTINCT ON columns (PostgreSQL-specific).
    pub distinct_on: Vec<String>,
    /// Fields to select.
    pub select: Vec<Field>,
    /// Raw SQL (if using sql heredoc escape hatch).
    pub raw_sql: Option<String>,
    /// Return type declaration (for raw SQL).
    pub returns: Vec<ReturnField>,
}

/// A query parameter.
#[derive(Debug, Clone)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub ty: ParamType,
    /// Source span.
    pub span: Option<Span>,
}

/// Parameter types.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    String,
    Int,
    Bool,
    Uuid,
    Decimal,
    Timestamp,
    Bytes,
    Optional(Box<ParamType>),
}

/// A field in the select clause.
#[derive(Debug, Clone)]
pub enum Field {
    /// Simple column reference.
    Column { name: String, span: Option<Span> },
    /// Relation (nested query via FK).
    Relation {
        name: String,
        span: Option<Span>,
        /// Explicit target table (if specified with `from`).
        from: Option<String>,
        /// WHERE filters for the relation.
        filters: Vec<Filter>,
        /// ORDER BY for the relation.
        order_by: Vec<OrderBy>,
        /// Whether to return first row only.
        first: bool,
        /// Nested fields to select.
        select: Vec<Field>,
    },
    /// Aggregate count.
    Count {
        name: String,
        table: String,
        span: Option<Span>,
    },
}

/// A filter condition.
#[derive(Debug, Clone)]
pub struct Filter {
    /// Column name.
    pub column: String,
    /// Operator.
    pub op: FilterOp,
    /// Value to compare against.
    pub value: Expr,
    /// Source span.
    pub span: Option<Span>,
}

/// Filter operators.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    Like,
    ILike,
    IsNull,
    IsNotNull,
    In,
    /// JSONB get object operator (->)
    JsonGet,
    /// JSONB get text operator (->>)
    JsonGetText,
    /// Contains operator (@>)
    Contains,
    /// Key exists operator (?)
    KeyExists,
}

/// An expression (value in a filter or limit).
#[derive(Debug, Clone)]
pub enum Expr {
    /// Parameter reference ($name).
    Param(String),
    /// String literal.
    String(String),
    /// Integer literal.
    Int(i64),
    /// Boolean literal.
    Bool(bool),
    /// Null.
    Null,
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Param(name) => write!(f, "${}", name),
            Expr::String(s) => write!(f, "'{}'", s.replace('\'', "''")),
            Expr::Int(n) => write!(f, "{}", n),
            Expr::Bool(b) => write!(f, "{}", b),
            Expr::Null => write!(f, "NULL"),
        }
    }
}

/// ORDER BY clause.
#[derive(Debug, Clone)]
pub struct OrderBy {
    /// Column name.
    pub column: String,
    /// Direction.
    pub direction: SortDir,
    /// Source span.
    pub span: Option<Span>,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// Return field declaration (for raw SQL queries).
#[derive(Debug, Clone)]
pub struct ReturnField {
    pub name: String,
    pub ty: ParamType,
    pub span: Option<Span>,
}

impl Query {
    /// Check if this is a raw SQL query.
    pub fn is_raw(&self) -> bool {
        self.raw_sql.is_some()
    }
}

/// An INSERT mutation.
#[derive(Debug, Clone)]
pub struct InsertMutation {
    /// Mutation name.
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Target table.
    pub table: String,
    /// Values to insert (column -> value expression).
    pub values: Vec<(String, ValueExpr)>,
    /// Columns to return.
    pub returning: Vec<String>,
}

/// An UPSERT mutation (INSERT ... ON CONFLICT ... DO UPDATE).
#[derive(Debug, Clone)]
pub struct UpsertMutation {
    /// Mutation name.
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Target table.
    pub table: String,
    /// Conflict target columns.
    pub conflict_columns: Vec<String>,
    /// Values to insert/update.
    pub values: Vec<(String, ValueExpr)>,
    /// Columns to return.
    pub returning: Vec<String>,
}

/// A bulk INSERT mutation (insert multiple rows with UNNEST).
#[derive(Debug, Clone)]
pub struct InsertManyMutation {
    /// Mutation name.
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Parameters - each becomes an array parameter for UNNEST.
    pub params: Vec<Param>,
    /// Target table.
    pub table: String,
    /// Values to insert (column -> value expression).
    /// Params reference UNNEST columns, other expressions are applied to each row.
    pub values: Vec<(String, ValueExpr)>,
    /// Columns to return.
    pub returning: Vec<String>,
}

/// A bulk UPSERT mutation (upsert multiple rows with UNNEST + ON CONFLICT).
#[derive(Debug, Clone)]
pub struct UpsertManyMutation {
    /// Mutation name.
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Parameters - each becomes an array parameter for UNNEST.
    pub params: Vec<Param>,
    /// Target table.
    pub table: String,
    /// Conflict target columns.
    pub conflict_columns: Vec<String>,
    /// Values to insert/update.
    pub values: Vec<(String, ValueExpr)>,
    /// Columns to return.
    pub returning: Vec<String>,
}

/// An UPDATE mutation.
#[derive(Debug, Clone)]
pub struct UpdateMutation {
    /// Mutation name.
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Target table.
    pub table: String,
    /// Values to set.
    pub values: Vec<(String, ValueExpr)>,
    /// WHERE filters.
    pub filters: Vec<Filter>,
    /// Columns to return.
    pub returning: Vec<String>,
}

/// A DELETE mutation.
#[derive(Debug, Clone)]
pub struct DeleteMutation {
    /// Mutation name.
    pub name: String,
    /// Doc comment from the styx file (/// comments).
    pub doc_comment: Option<String>,
    /// Source span.
    pub span: Option<Span>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Target table.
    pub table: String,
    /// WHERE filters.
    pub filters: Vec<Filter>,
    /// Columns to return.
    pub returning: Vec<String>,
}

/// A value expression for INSERT/UPDATE.
#[derive(Debug, Clone)]
pub enum ValueExpr {
    /// Parameter reference ($name).
    Param(String),
    /// String literal.
    String(String),
    /// Integer literal.
    Int(i64),
    /// Boolean literal.
    Bool(bool),
    /// Null.
    Null,
    /// SQL function call (e.g., NOW(), COALESCE($a, $b), LOWER($x)).
    FunctionCall {
        /// Function name (e.g., "now", "coalesce", "lower").
        name: String,
        /// Arguments to the function.
        args: Vec<ValueExpr>,
    },
    /// Database default.
    Default,
}
