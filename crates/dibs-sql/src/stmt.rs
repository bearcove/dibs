//! SQL statements.

use crate::expr::Expr;
use crate::{ColumnName, TableName};

/// A SQL statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// A SELECT query.
    Select(SelectStmt),
    /// An INSERT statement.
    Insert(InsertStmt),
    /// An UPDATE statement.
    Update(UpdateStmt),
    /// A DELETE statement.
    Delete(DeleteStmt),
}

/// A SELECT statement.
#[derive(Debug, Clone, Default)]
pub struct SelectStmt {
    /// Columns to select (empty means `SELECT *`).
    pub columns: Vec<SelectColumn>,
    /// The FROM clause specifying the primary table.
    pub from: Option<FromClause>,
    /// JOIN clauses for related tables.
    pub joins: Vec<Join>,
    /// The WHERE clause filter condition.
    pub where_: Option<Expr>,
    /// ORDER BY clauses for sorting results.
    pub order_by: Vec<OrderBy>,
    /// LIMIT clause to restrict number of rows.
    pub limit: Option<Expr>,
    /// OFFSET clause for pagination.
    pub offset: Option<Expr>,
}

/// A column in a SELECT clause.
#[derive(Debug, Clone)]
pub enum SelectColumn {
    /// An expression with optional alias: `expr AS alias`.
    Expr {
        /// The expression to select.
        expr: Expr,
        /// Optional alias for the column.
        alias: Option<ColumnName>,
    },

    /// All columns from a table: `table.*`.
    AllFrom(TableName),
}

impl SelectColumn {
    pub fn expr(expr: Expr) -> Self {
        SelectColumn::Expr { expr, alias: None }
    }

    pub fn aliased(expr: Expr, alias: ColumnName) -> Self {
        SelectColumn::Expr {
            expr,
            alias: Some(alias),
        }
    }

    pub fn all_from(table: TableName) -> Self {
        SelectColumn::AllFrom(table)
    }
}

/// A FROM clause specifying the primary table.
#[derive(Debug, Clone)]
pub struct FromClause {
    /// The table name.
    pub table: TableName,
    /// Optional alias for the table (e.g., `FROM users t0`).
    pub alias: Option<TableName>,
}

impl FromClause {
    pub fn table(name: TableName) -> Self {
        Self {
            table: name,
            alias: None,
        }
    }

    pub fn aliased(name: TableName, alias: TableName) -> Self {
        Self {
            table: name,
            alias: Some(alias),
        }
    }
}

/// A JOIN clause.
#[derive(Debug, Clone)]
pub struct Join {
    /// The type of join (INNER, LEFT, RIGHT, FULL).
    pub kind: JoinKind,
    /// The table to join.
    pub table: TableName,
    /// Optional alias for the joined table.
    pub alias: Option<TableName>,
    /// The ON condition for the join.
    pub on: Expr,
}

/// Type of JOIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    /// INNER JOIN - only matching rows from both tables.
    Inner,
    /// LEFT JOIN - all rows from left table, matching from right.
    Left,
    /// RIGHT JOIN - all rows from right table, matching from left.
    Right,
    /// FULL JOIN - all rows from both tables.
    Full,
}

impl JoinKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JoinKind::Inner => "INNER JOIN",
            JoinKind::Left => "LEFT JOIN",
            JoinKind::Right => "RIGHT JOIN",
            JoinKind::Full => "FULL JOIN",
        }
    }
}

/// ORDER BY clause for sorting query results.
#[derive(Debug, Clone)]
pub struct OrderBy {
    /// The expression to sort by.
    pub expr: Expr,
    /// Whether to sort descending (true) or ascending (false).
    pub desc: bool,
    /// Optional NULLS FIRST / NULLS LAST specification.
    pub nulls: Option<NullsOrder>,
}

impl OrderBy {
    pub fn asc(expr: Expr) -> Self {
        Self {
            expr,
            desc: false,
            nulls: None,
        }
    }

    pub fn desc(expr: Expr) -> Self {
        Self {
            expr,
            desc: true,
            nulls: None,
        }
    }
}

/// NULLS FIRST / NULLS LAST ordering for ORDER BY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullsOrder {
    /// NULL values sort before non-NULL values.
    First,
    /// NULL values sort after non-NULL values.
    Last,
}

// ============================================================================
// INSERT statement
// ============================================================================

/// An INSERT statement.
#[derive(Debug, Clone)]
pub struct InsertStmt {
    /// The table to insert into.
    pub table: TableName,
    /// Column names for the insert.
    pub columns: Vec<ColumnName>,
    /// Values to insert (parallel to columns).
    pub values: Vec<Expr>,
    /// Optional ON CONFLICT clause for upsert behavior.
    pub on_conflict: Option<OnConflict>,
    /// Columns to return after insert (RETURNING clause).
    pub returning: Vec<ColumnName>,
}

/// ON CONFLICT clause for upsert behavior.
#[derive(Debug, Clone)]
pub struct OnConflict {
    /// Conflict target columns (the unique constraint columns).
    pub columns: Vec<ColumnName>,
    /// What to do when a conflict occurs.
    pub action: ConflictAction,
}

/// Action to take when a conflict occurs.
#[derive(Debug, Clone)]
pub enum ConflictAction {
    /// DO NOTHING - skip the conflicting row.
    DoNothing,
    /// DO UPDATE SET - update the existing row.
    DoUpdate(Vec<UpdateAssignment>),
}

/// A column assignment for UPDATE SET or ON CONFLICT DO UPDATE SET.
#[derive(Debug, Clone)]
pub struct UpdateAssignment {
    /// The column to update.
    pub column: ColumnName,
    /// The value to assign.
    pub value: Expr,
}

impl UpdateAssignment {
    pub fn new(column: ColumnName, value: Expr) -> Self {
        Self { column, value }
    }
}

// ============================================================================
// UPDATE statement
// ============================================================================

/// An UPDATE statement.
#[derive(Debug, Clone)]
pub struct UpdateStmt {
    /// The table to update.
    pub table: TableName,
    /// Column assignments (SET clause).
    pub assignments: Vec<UpdateAssignment>,
    /// Optional WHERE clause filter.
    pub where_: Option<Expr>,
    /// Columns to return after update (RETURNING clause).
    pub returning: Vec<ColumnName>,
}

// ============================================================================
// DELETE statement
// ============================================================================

/// A DELETE statement.
#[derive(Debug, Clone)]
pub struct DeleteStmt {
    /// The table to delete from.
    pub table: TableName,
    /// Optional WHERE clause filter.
    pub where_: Option<Expr>,
    /// Columns to return after delete (RETURNING clause).
    pub returning: Vec<ColumnName>,
}

// ============================================================================
// Builder-style constructors
// ============================================================================

impl SelectStmt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn column(mut self, col: SelectColumn) -> Self {
        self.columns.push(col);
        self
    }

    pub fn columns(mut self, cols: impl IntoIterator<Item = SelectColumn>) -> Self {
        self.columns.extend(cols);
        self
    }

    pub fn from(mut self, from: FromClause) -> Self {
        self.from = Some(from);
        self
    }

    pub fn join(mut self, join: Join) -> Self {
        self.joins.push(join);
        self
    }

    pub fn where_(mut self, expr: Expr) -> Self {
        self.where_ = Some(expr);
        self
    }

    pub fn and_where(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(existing) => existing.and(expr),
            None => expr,
        });
        self
    }

    pub fn order_by(mut self, order: OrderBy) -> Self {
        self.order_by.push(order);
        self
    }

    pub fn limit(mut self, expr: Expr) -> Self {
        self.limit = Some(expr);
        self
    }

    pub fn offset(mut self, expr: Expr) -> Self {
        self.offset = Some(expr);
        self
    }
}

impl InsertStmt {
    pub fn new(table: TableName) -> Self {
        Self {
            table,
            columns: Vec::new(),
            values: Vec::new(),
            on_conflict: None,
            returning: Vec::new(),
        }
    }

    pub fn column(mut self, name: ColumnName, value: Expr) -> Self {
        self.columns.push(name);
        self.values.push(value);
        self
    }

    pub fn on_conflict(mut self, conflict: OnConflict) -> Self {
        self.on_conflict = Some(conflict);
        self
    }

    pub fn returning(mut self, cols: impl IntoIterator<Item = ColumnName>) -> Self {
        self.returning.extend(cols);
        self
    }
}

impl UpdateStmt {
    pub fn new(table: TableName) -> Self {
        Self {
            table,
            assignments: Vec::new(),
            where_: None,
            returning: Vec::new(),
        }
    }

    pub fn set(mut self, column: ColumnName, value: Expr) -> Self {
        self.assignments.push(UpdateAssignment::new(column, value));
        self
    }

    pub fn where_(mut self, expr: Expr) -> Self {
        self.where_ = Some(expr);
        self
    }

    pub fn and_where(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(existing) => existing.and(expr),
            None => expr,
        });
        self
    }

    pub fn returning(mut self, cols: impl IntoIterator<Item = ColumnName>) -> Self {
        self.returning.extend(cols);
        self
    }
}

impl DeleteStmt {
    pub fn new(table: TableName) -> Self {
        Self {
            table,
            where_: None,
            returning: Vec::new(),
        }
    }

    pub fn where_(mut self, expr: Expr) -> Self {
        self.where_ = Some(expr);
        self
    }

    pub fn and_where(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(existing) => existing.and(expr),
            None => expr,
        });
        self
    }

    pub fn returning(mut self, cols: impl IntoIterator<Item = ColumnName>) -> Self {
        self.returning.extend(cols);
        self
    }
}
