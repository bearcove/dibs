//! Render SQL AST to string.

use std::cell::RefCell;
use std::fmt;

use indexmap::IndexMap;

use crate::expr::{ColumnRef, Expr};
use crate::stmt::*;
use crate::{Ident, ParamName, RenderedSql, escape_string};

/// Mutable parameter tracking state.
struct ParamState {
    /// Named parameters -> their assigned index
    params: IndexMap<ParamName, usize>,
    /// Next parameter index to assign
    next_param_idx: usize,
}

impl ParamState {
    fn new() -> Self {
        Self {
            params: IndexMap::new(),
            next_param_idx: 1,
        }
    }

    /// Get or create a parameter index.
    fn get_or_insert(&mut self, name: &ParamName) -> usize {
        *self.params.entry(name.clone()).or_insert_with(|| {
            let idx = self.next_param_idx;
            self.next_param_idx += 1;
            idx
        })
    }
}

/// Rendering context that tracks parameters.
pub struct RenderContext {
    params: RefCell<ParamState>,
}

impl RenderContext {
    pub fn new() -> Self {
        Self {
            params: RefCell::new(ParamState::new()),
        }
    }

    /// Get or create a parameter placeholder index.
    fn param_idx(&self, name: &ParamName) -> usize {
        self.params.borrow_mut().get_or_insert(name)
    }

    /// Finish rendering and return the collected params.
    fn into_params(self) -> Vec<ParamName> {
        self.params.into_inner().params.into_keys().collect()
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for rendering a value via Display.
pub struct Fmt<'a, T: Render>(&'a RenderContext, &'a T);

impl<T: Render> fmt::Display for Fmt<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.1.render(self.0, f)
    }
}

// ============================================================================
// Render implementations
// ============================================================================

/// Trait for types that can be rendered to SQL.
pub trait Render {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl Render for Expr {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Param(name) => {
                let idx = ctx.param_idx(name);
                write!(f, "${idx}")
            }
            Expr::Column(col) => col.render(ctx, f),
            Expr::String(s) => {
                let escaped = escape_string(s);
                write!(f, "{escaped}")
            }
            Expr::Int(n) => write!(f, "{n}"),
            Expr::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Expr::Null => write!(f, "NULL"),
            Expr::Now => write!(f, "NOW()"),
            Expr::Default => write!(f, "DEFAULT"),
            Expr::BinOp { left, op, right } => {
                let left = Fmt(ctx, left.as_ref());
                let right = Fmt(ctx, right.as_ref());
                let op = op.as_str();
                write!(f, "{left} {op} {right}")
            }
            Expr::IsNull { expr, negated } => {
                let expr = Fmt(ctx, expr.as_ref());
                let suffix = if *negated { " IS NOT NULL" } else { " IS NULL" };
                write!(f, "{expr}{suffix}")
            }
            Expr::ILike { expr, pattern } => {
                let expr = Fmt(ctx, expr.as_ref());
                let pattern = Fmt(ctx, pattern.as_ref());
                write!(f, "{expr} ILIKE {pattern}")
            }
            Expr::FnCall { name, args } => {
                write!(f, "{name}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", Fmt(ctx, arg))?;
                }
                write!(f, ")")
            }
            Expr::Count { table } => {
                let table = Ident(table.as_str());
                write!(f, "COUNT({table}.*)")
            }
            Expr::Raw(s) => write!(f, "{s}"),
        }
    }
}

impl Render for ColumnRef {
    fn render(&self, _ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(table) = &self.table {
            let table = Ident(table.as_str());
            write!(f, "{table}.")?;
        }
        let column = Ident(self.column.as_str());
        write!(f, "{column}")
    }
}

impl Render for SelectStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SELECT")?;

        // Columns
        if self.columns.is_empty() {
            write!(f, " *")?;
        } else {
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, " {}", Fmt(ctx, col))?;
            }
        }

        // FROM
        if let Some(from) = &self.from {
            let table = Ident(from.table.as_str());
            write!(f, "\nFROM {table}")?;
            if let Some(alias) = &from.alias {
                let alias = Ident(alias.as_str());
                write!(f, " {alias}")?;
            }
        }

        // JOINs
        for join in &self.joins {
            let kind = join.kind.as_str();
            let table = Ident(join.table.as_str());
            write!(f, "\n{kind} {table}")?;
            if let Some(alias) = &join.alias {
                let alias = Ident(alias.as_str());
                write!(f, " {alias}")?;
            }
            let on = Fmt(ctx, &join.on);
            write!(f, " ON {on}")?;
        }

        // WHERE
        if let Some(where_) = &self.where_ {
            let where_ = Fmt(ctx, where_);
            write!(f, "\nWHERE {where_}")?;
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            write!(f, "\nORDER BY ")?;
            for (i, order) in self.order_by.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let expr = Fmt(ctx, &order.expr);
                let dir = if order.desc { " DESC" } else { " ASC" };
                write!(f, "{expr}{dir}")?;
                if let Some(nulls) = &order.nulls {
                    write!(
                        f,
                        "{}",
                        match nulls {
                            NullsOrder::First => " NULLS FIRST",
                            NullsOrder::Last => " NULLS LAST",
                        }
                    )?;
                }
            }
        }

        // LIMIT
        if let Some(limit) = &self.limit {
            let limit = Fmt(ctx, limit);
            write!(f, "\nLIMIT {limit}")?;
        }

        // OFFSET
        if let Some(offset) = &self.offset {
            let offset = Fmt(ctx, offset);
            write!(f, "\nOFFSET {offset}")?;
        }

        Ok(())
    }
}

impl Render for SelectColumn {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectColumn::Expr { expr, alias } => {
                let expr = Fmt(ctx, expr);
                write!(f, "{expr}")?;
                if let Some(alias) = alias {
                    let alias = Ident(alias.as_str());
                    write!(f, " AS {alias}")?;
                }
                Ok(())
            }
            SelectColumn::AllFrom(table) => {
                let table = Ident(table.as_str());
                write!(f, "{table}.*")
            }
        }
    }
}

impl Render for InsertStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "INSERT INTO {table} (")?;

        // Columns
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let col = Ident(col.as_str());
            write!(f, "{col}")?;
        }
        write!(f, ")")?;

        // VALUES
        write!(f, "\nVALUES (")?;
        for (i, val) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", Fmt(ctx, val))?;
        }
        write!(f, ")")?;

        // ON CONFLICT
        if let Some(conflict) = &self.on_conflict {
            write!(f, "\nON CONFLICT (")?;
            for (i, col) in conflict.columns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
            write!(f, ")")?;

            match &conflict.action {
                ConflictAction::DoNothing => {
                    write!(f, " DO NOTHING")?;
                }
                ConflictAction::DoUpdate(assignments) => {
                    write!(f, " DO UPDATE SET ")?;
                    for (i, assign) in assignments.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        let col = Ident(assign.column.as_str());
                        let val = Fmt(ctx, &assign.value);
                        write!(f, "{col} = {val}")?;
                    }
                }
            }
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for UpdateStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "UPDATE {table}")?;

        // SET
        write!(f, "\nSET ")?;
        for (i, assign) in self.assignments.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let col = Ident(assign.column.as_str());
            let val = Fmt(ctx, &assign.value);
            write!(f, "{col} = {val}")?;
        }

        // WHERE
        if let Some(where_) = &self.where_ {
            let where_ = Fmt(ctx, where_);
            write!(f, "\nWHERE {where_}")?;
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for DeleteStmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = Ident(self.table.as_str());
        write!(f, "DELETE FROM {table}")?;

        // WHERE
        if let Some(where_) = &self.where_ {
            let where_ = Fmt(ctx, where_);
            write!(f, "\nWHERE {where_}")?;
        }

        // RETURNING
        if !self.returning.is_empty() {
            write!(f, "\nRETURNING ")?;
            for (i, col) in self.returning.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let col = Ident(col.as_str());
                write!(f, "{col}")?;
            }
        }

        Ok(())
    }
}

impl Render for Stmt {
    fn render(&self, ctx: &RenderContext, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Select(s) => s.render(ctx, f),
            Stmt::Insert(s) => s.render(ctx, f),
            Stmt::Update(s) => s.render(ctx, f),
            Stmt::Delete(s) => s.render(ctx, f),
        }
    }
}

// ============================================================================
// Convenience methods
// ============================================================================

/// Render a statement to SQL.
pub fn render(stmt: &impl Render) -> RenderedSql {
    let ctx = RenderContext::new();
    let sql = format!("{}", Fmt(&ctx, stmt));
    RenderedSql {
        sql,
        params: ctx.into_params(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Expr;

    #[test]
    fn test_param_deduplication() {
        // Build: INSERT INTO t (a, b) VALUES ($a, $b) ON CONFLICT (a) DO UPDATE SET b = $b
        let stmt = InsertStmt::new("products")
            .column("handle", Expr::param("handle"))
            .column("status", Expr::param("status"))
            .on_conflict(OnConflict {
                columns: vec!["handle".into()],
                action: ConflictAction::DoUpdate(vec![UpdateAssignment::new(
                    "status",
                    Expr::param("status"), // same param, should be $2 not $3
                )]),
            })
            .returning(["id", "handle", "status"]);

        let result = render(&stmt);

        // Key assertion: params should only have 2 entries
        assert_eq!(result.params, vec!["handle", "status"]);

        // SQL should reuse $2 for both VALUES and UPDATE SET
        assert!(result.sql.contains("VALUES ($1, $2)"));
        assert!(result.sql.contains("\"status\" = $2"));
    }

    #[test]
    fn test_simple_select() {
        let stmt = SelectStmt::new()
            .columns([
                SelectColumn::expr(Expr::column("id")),
                SelectColumn::expr(Expr::column("name")),
            ])
            .from(FromClause::table("users"));

        let result = render(&stmt);
        assert_eq!(result.sql, "SELECT \"id\", \"name\" FROM \"users\"");
    }

    #[test]
    fn test_select_with_where() {
        let stmt = SelectStmt::new()
            .columns([SelectColumn::expr(Expr::column("id"))])
            .from(FromClause::table("users"))
            .where_(Expr::column("id").eq(Expr::param("id")));

        let result = render(&stmt);
        assert_eq!(result.sql, "SELECT \"id\" FROM \"users\" WHERE \"id\" = $1");
        assert_eq!(result.params, vec!["id"]);
    }

    #[test]
    fn test_insert() {
        let stmt = InsertStmt::new("products")
            .column("handle", Expr::param("handle"))
            .column("status", Expr::param("status"))
            .returning(["id", "handle", "status"]);

        let result = render(&stmt);
        assert_eq!(
            result.sql,
            "INSERT INTO \"products\" (\"handle\", \"status\") VALUES ($1, $2) RETURNING \"id\", \"handle\", \"status\""
        );
        assert_eq!(result.params, vec!["handle", "status"]);
    }

    #[test]
    fn test_insert_with_literals() {
        let stmt = InsertStmt::new("products")
            .column("handle", Expr::param("handle"))
            .column("status", Expr::Default)
            .column("created_at", Expr::Now);

        let result = render(&stmt);
        assert!(result.sql.contains("VALUES ($1, DEFAULT, NOW())"));
        assert_eq!(result.params, vec!["handle"]);
    }

    #[test]
    fn test_update() {
        let stmt = UpdateStmt::new("products")
            .set("status", Expr::param("status"))
            .where_(Expr::column("handle").eq(Expr::param("handle")))
            .returning(["id", "handle", "status"]);

        let result = render(&stmt);
        assert_eq!(
            result.sql,
            "UPDATE \"products\" SET \"status\" = $1 WHERE \"handle\" = $2 RETURNING \"id\", \"handle\", \"status\""
        );
        assert_eq!(result.params, vec!["status", "handle"]);
    }

    #[test]
    fn test_delete() {
        let stmt = DeleteStmt::new("products")
            .where_(Expr::column("id").eq(Expr::param("id")))
            .returning(["id", "handle"]);

        let result = render(&stmt);
        assert_eq!(
            result.sql,
            "DELETE FROM \"products\" WHERE \"id\" = $1 RETURNING \"id\", \"handle\""
        );
        assert_eq!(result.params, vec!["id"]);
    }

    #[test]
    fn test_qualified_columns() {
        let stmt = SelectStmt::new()
            .columns([
                SelectColumn::expr(Expr::qualified_column("t0", "id")),
                SelectColumn::expr(Expr::qualified_column("t1", "name")),
            ])
            .from(FromClause::aliased("users", "t0"))
            .join(Join {
                kind: JoinKind::Left,
                table: "profiles".into(),
                alias: Some("t1".into()),
                on: Expr::qualified_column("t1", "user_id").eq(Expr::qualified_column("t0", "id")),
            });

        let result = render(&stmt);
        assert!(result.sql.contains("\"t0\".\"id\""));
        assert!(result.sql.contains("\"t1\".\"name\""));
        assert!(result.sql.contains("LEFT JOIN \"profiles\" \"t1\" ON"));
    }

    #[test]
    fn test_pretty_formatting() {
        let stmt = SelectStmt::new()
            .columns([
                SelectColumn::expr(Expr::column("id")),
                SelectColumn::expr(Expr::column("name")),
            ])
            .from(FromClause::table("users"))
            .where_(Expr::column("active").eq(Expr::Bool(true)))
            .order_by(OrderBy::desc(Expr::column("created_at")))
            .limit(Expr::Int(10));

        let result = render_pretty(&stmt);
        assert!(result.sql.contains("\n"), "Should have newlines");
        assert!(result.sql.contains("FROM"));
        assert!(result.sql.contains("WHERE"));
        assert!(result.sql.contains("ORDER BY"));
        assert!(result.sql.contains("LIMIT"));
    }

    #[test]
    fn test_is_null() {
        let stmt = SelectStmt::new()
            .columns([SelectColumn::expr(Expr::column("id"))])
            .from(FromClause::table("users"))
            .where_(Expr::column("deleted_at").is_null());

        let result = render(&stmt);
        assert!(result.sql.contains("\"deleted_at\" IS NULL"));
    }

    #[test]
    fn test_ilike() {
        let stmt = SelectStmt::new()
            .columns([SelectColumn::expr(Expr::column("id"))])
            .from(FromClause::table("users"))
            .where_(Expr::column("name").ilike(Expr::param("pattern")));

        let result = render(&stmt);
        assert!(result.sql.contains("\"name\" ILIKE $1"));
        assert_eq!(result.params, vec!["pattern"]);
    }
}
