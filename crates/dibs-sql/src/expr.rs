//! SQL expressions.

use crate::{ColumnName, ParamName, TableName};

/// A SQL expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A parameter placeholder (e.g., $handle -> $1)
    Param(ParamName),
    /// A column reference
    Column(ColumnRef),
    /// A string literal
    String(String),
    /// An integer literal
    Int(i64),
    /// A boolean literal
    Bool(bool),
    /// NULL
    Null,
    /// NOW() function
    Now,
    /// DEFAULT keyword
    Default,
    /// Binary operation (e.g., a = b, a AND b)
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// IS NULL / IS NOT NULL
    IsNull { expr: Box<Expr>, negated: bool },
    /// ILIKE pattern match
    ILike { expr: Box<Expr>, pattern: Box<Expr> },
    /// Function call
    FnCall { name: String, args: Vec<Expr> },
    /// COUNT(table.*) for counting related rows
    Count { table: TableName },
    /// Raw SQL (escape hatch)
    Raw(String),
}

/// A column reference, optionally qualified with table/alias.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnRef {
    pub table: Option<TableName>,
    pub column: ColumnName,
}

impl ColumnRef {
    pub fn new(column: ColumnName) -> Self {
        Self {
            table: None,
            column,
        }
    }

    pub fn qualified(table: TableName, column: ColumnName) -> Self {
        Self {
            table: Some(table),
            column,
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "AND",
            BinOp::Or => "OR",
        }
    }
}

// Convenience constructors
impl Expr {
    pub fn param(name: ParamName) -> Self {
        Expr::Param(name)
    }

    pub fn column(name: ColumnName) -> Self {
        Expr::Column(ColumnRef::new(name))
    }

    pub fn qualified_column(table: TableName, column: ColumnName) -> Self {
        Expr::Column(ColumnRef::qualified(table, column))
    }

    pub fn string(s: impl Into<String>) -> Self {
        Expr::String(s.into())
    }

    pub fn int(n: i64) -> Self {
        Expr::Int(n)
    }

    pub fn bool(b: bool) -> Self {
        Expr::Bool(b)
    }

    /// Create an equality expression: self = other
    pub fn eq(self, other: Expr) -> Self {
        Expr::BinOp {
            left: Box::new(self),
            op: BinOp::Eq,
            right: Box::new(other),
        }
    }

    /// Create an AND expression: self AND other
    pub fn and(self, other: Expr) -> Self {
        Expr::BinOp {
            left: Box::new(self),
            op: BinOp::And,
            right: Box::new(other),
        }
    }

    /// Create an OR expression: self OR other
    pub fn or(self, other: Expr) -> Self {
        Expr::BinOp {
            left: Box::new(self),
            op: BinOp::Or,
            right: Box::new(other),
        }
    }

    /// Create IS NULL expression
    pub fn is_null(self) -> Self {
        Expr::IsNull {
            expr: Box::new(self),
            negated: false,
        }
    }

    /// Create IS NOT NULL expression
    pub fn is_not_null(self) -> Self {
        Expr::IsNull {
            expr: Box::new(self),
            negated: true,
        }
    }

    /// Create ILIKE expression
    pub fn ilike(self, pattern: Expr) -> Self {
        Expr::ILike {
            expr: Box::new(self),
            pattern: Box::new(pattern),
        }
    }
}
