//! Query AST types.
//!
//! These represent the semantic structure of a query after parsing from styx.

use styx_parse::Span;

/// A file containing multiple queries.
#[derive(Debug, Clone)]
pub struct QueryFile {
    pub queries: Vec<Query>,
}

/// A single query definition.
#[derive(Debug, Clone)]
pub struct Query {
    /// Query name (e.g., "ProductListing").
    pub name: String,
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
