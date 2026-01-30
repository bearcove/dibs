//! SQL generation from query AST.

use crate::ast::*;
use crate::planner::{PlannerSchema, QueryPlan, QueryPlanner};
use dibs_sql::{
    BinOp as SqlBinOp, ConflictAction, DeleteStmt, Expr as SqlExpr, InsertStmt, OnConflict,
    UpdateAssignment, UpdateStmt, render,
};

/// Generated SQL with parameter placeholders.
#[derive(Debug, Clone)]
pub struct GeneratedSql {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,
    /// Parameter names in order (maps to $1, $2, etc.).
    pub param_order: Vec<String>,
    /// Query plan (if JOINs are involved).
    pub plan: Option<QueryPlan>,
    /// Column names in SELECT order (for index-based access).
    /// Maps column names to their index in the result set.
    pub column_order: std::collections::HashMap<String, usize>,
}

/// Generate SQL for a simple single-table query (no relations).
pub fn generate_simple_sql(query: &Query) -> GeneratedSql {
    let mut sql = String::new();
    let mut param_order = Vec::new();
    let mut param_idx = 1;
    let mut column_order = std::collections::HashMap::new();

    // SELECT
    sql.push_str("SELECT ");

    // DISTINCT or DISTINCT ON
    if !query.distinct_on.is_empty() {
        sql.push_str("DISTINCT ON (");
        let distinct_cols: Vec<_> = query
            .distinct_on
            .iter()
            .map(|col| format!("\"{}\"", col))
            .collect();
        sql.push_str(&distinct_cols.join(", "));
        sql.push_str(") ");
    } else if query.distinct {
        sql.push_str("DISTINCT ");
    }

    let columns: Vec<_> = query
        .select
        .iter()
        .filter_map(|f| match f {
            Field::Column { name, .. } => Some(name.clone()),
            _ => None, // Skip relations/aggregates for simple query
        })
        .collect();

    // Build column_order map
    for (idx, col_name) in columns.iter().enumerate() {
        column_order.insert(col_name.clone(), idx);
    }

    let formatted_columns: Vec<_> = columns.iter().map(|name| format!("\"{}\"", name)).collect();

    if formatted_columns.is_empty() {
        sql.push('*');
    } else {
        sql.push_str(&formatted_columns.join(", "));
    }

    // FROM
    sql.push_str(" FROM \"");
    sql.push_str(&query.from);
    sql.push('"');

    // WHERE
    if !query.filters.is_empty() {
        sql.push_str(" WHERE ");
        let conditions: Vec<_> = query
            .filters
            .iter()
            .map(|f| {
                let (cond, new_idx) = format_filter(f, param_idx, &mut param_order);
                param_idx = new_idx;
                cond
            })
            .collect();
        sql.push_str(&conditions.join(" AND "));
    }

    // ORDER BY
    if !query.order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        let orders: Vec<_> = query
            .order_by
            .iter()
            .map(|o| {
                format!(
                    "\"{}\" {}",
                    o.column,
                    match o.direction {
                        SortDir::Asc => "ASC",
                        SortDir::Desc => "DESC",
                    }
                )
            })
            .collect();
        sql.push_str(&orders.join(", "));
    }

    // LIMIT
    if let Some(limit) = &query.limit {
        sql.push_str(" LIMIT ");
        match limit {
            Expr::Int(n) => sql.push_str(&n.to_string()),
            Expr::Param(name) => {
                param_order.push(name.clone());
                sql.push_str(&format!("${}", param_idx));
                param_idx += 1;
            }
            _ => sql.push_str("20"), // fallback
        }
    }

    // OFFSET
    if let Some(offset) = &query.offset {
        sql.push_str(" OFFSET ");
        match offset {
            Expr::Int(n) => sql.push_str(&n.to_string()),
            Expr::Param(name) => {
                param_order.push(name.clone());
                sql.push_str(&format!("${}", param_idx));
                param_idx += 1;
            }
            _ => sql.push('0'), // fallback
        }
    }

    // Suppress unused warning - param_idx is used during iteration
    let _ = param_idx;

    GeneratedSql {
        sql,
        param_order,
        plan: None,
        column_order,
    }
}

/// Generate SQL for a query with optional JOINs using the planner.
///
/// If schema is None or the query has no relations/COUNT fields, falls back to simple SQL generation.
pub fn generate_sql_with_joins(
    query: &Query,
    schema: Option<&PlannerSchema>,
) -> Result<GeneratedSql, crate::planner::PlanError> {
    // Check if query needs the planner (has relations or COUNT fields)
    let needs_planner = query
        .select
        .iter()
        .any(|f| matches!(f, Field::Relation { .. }) || matches!(f, Field::Count { .. }));

    if !needs_planner || schema.is_none() {
        // Fall back to simple SQL generation
        return Ok(generate_simple_sql(query));
    }

    // Plan the query
    let planner = QueryPlanner::new(schema.unwrap());
    let plan = planner.plan(query)?;

    let mut sql = String::new();
    let mut param_order = Vec::new();
    let mut param_idx = 1;
    let mut column_order = std::collections::HashMap::new();

    // Build column_order from plan's select_columns and count_subqueries
    let mut col_idx = 0;
    for col in &plan.select_columns {
        column_order.insert(col.result_alias.clone(), col_idx);
        col_idx += 1;
    }
    for count in &plan.count_subqueries {
        column_order.insert(count.result_alias.clone(), col_idx);
        col_idx += 1;
    }

    // SELECT with aliased columns
    sql.push_str("SELECT ");

    // DISTINCT or DISTINCT ON
    if !query.distinct_on.is_empty() {
        sql.push_str("DISTINCT ON (");
        let distinct_cols: Vec<_> = query
            .distinct_on
            .iter()
            .map(|col| format!("\"t0\".\"{}\"", col))
            .collect();
        sql.push_str(&distinct_cols.join(", "));
        sql.push_str(") ");
    } else if query.distinct {
        sql.push_str("DISTINCT ");
    }

    sql.push_str(&plan.select_sql());

    // FROM with JOINs (including relation filters in ON clauses)
    sql.push_str(" FROM ");
    sql.push_str(&plan.from_sql_with_params(&mut param_order, &mut param_idx));

    // WHERE
    if !query.filters.is_empty() {
        sql.push_str(" WHERE ");
        let conditions: Vec<_> = query
            .filters
            .iter()
            .map(|f| {
                // Prefix column with base table alias
                let mut filter = f.clone();
                filter.column = format!("t0.{}", f.column);
                let (cond, new_idx) = format_filter(&filter, param_idx, &mut param_order);
                param_idx = new_idx;
                cond
            })
            .collect();
        sql.push_str(&conditions.join(" AND "));
    }

    // ORDER BY
    if !query.order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        let orders: Vec<_> = query
            .order_by
            .iter()
            .map(|o| {
                format!(
                    "\"t0\".\"{}\" {}",
                    o.column,
                    match o.direction {
                        SortDir::Asc => "ASC",
                        SortDir::Desc => "DESC",
                    }
                )
            })
            .collect();
        sql.push_str(&orders.join(", "));
    }

    // LIMIT
    if let Some(limit) = &query.limit {
        sql.push_str(" LIMIT ");
        match limit {
            Expr::Int(n) => sql.push_str(&n.to_string()),
            Expr::Param(name) => {
                param_order.push(name.clone());
                sql.push_str(&format!("${}", param_idx));
                param_idx += 1;
            }
            _ => sql.push_str("20"),
        }
    }

    // OFFSET
    if let Some(offset) = &query.offset {
        sql.push_str(" OFFSET ");
        match offset {
            Expr::Int(n) => sql.push_str(&n.to_string()),
            Expr::Param(name) => {
                param_order.push(name.clone());
                sql.push_str(&format!("${}", param_idx));
                param_idx += 1;
            }
            _ => sql.push('0'),
        }
    }

    let _ = param_idx;

    Ok(GeneratedSql {
        sql,
        param_order,
        plan: Some(plan),
        column_order,
    })
}

fn format_filter(
    filter: &Filter,
    mut param_idx: usize,
    param_order: &mut Vec<String>,
) -> (String, usize) {
    // Handle dotted column names (e.g., "t0.column") by quoting each part
    let col = if filter.column.contains('.') {
        filter
            .column
            .split('.')
            .map(|part| format!("\"{}\"", part))
            .collect::<Vec<_>>()
            .join(".")
    } else {
        format!("\"{}\"", filter.column)
    };

    let result = match (&filter.op, &filter.value) {
        (FilterOp::IsNull, _) => format!("{} IS NULL", col),
        (FilterOp::IsNotNull, _) => format!("{} IS NOT NULL", col),
        (FilterOp::Eq, Expr::Null) => format!("{} IS NULL", col),
        (FilterOp::Ne, Expr::Null) => format!("{} IS NOT NULL", col),
        (FilterOp::Eq, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} = ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Eq, Expr::String(s)) => {
            // Inline string literals directly - escape single quotes
            let escaped = s.replace('\'', "''");
            format!("{} = '{}'", col, escaped)
        }
        (FilterOp::Eq, Expr::Int(n)) => format!("{} = {}", col, n),
        (FilterOp::Eq, Expr::Bool(b)) => format!("{} = {}", col, b),
        (FilterOp::Ne, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} != ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Lt, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} < ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Lte, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} <= ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Gt, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} > ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Gte, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} >= ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Like, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} LIKE ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::ILike, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} ILIKE ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::In, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} = ANY(${})", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::JsonGet, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} -> ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::JsonGet, Expr::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("{} -> '{}'", col, escaped)
        }
        (FilterOp::JsonGetText, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} ->> ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::JsonGetText, Expr::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("{} ->> '{}'", col, escaped)
        }
        (FilterOp::Contains, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} @> ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::Contains, Expr::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("{} @> '{}'", col, escaped)
        }
        (FilterOp::KeyExists, Expr::Param(name)) => {
            param_order.push(name.clone());
            let s = format!("{} ? ${}", col, param_idx);
            param_idx += 1;
            s
        }
        (FilterOp::KeyExists, Expr::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("{} ? '{}'", col, escaped)
        }
        _ => format!("{} = TRUE", col), // fallback
    };

    (result, param_idx)
}

/// Convert a ValueExpr to a dibs_sql::Expr.
fn value_expr_to_sql(expr: &ValueExpr) -> SqlExpr {
    match expr {
        ValueExpr::Param(name) => SqlExpr::param(name),
        ValueExpr::String(s) => SqlExpr::string(s),
        ValueExpr::Int(n) => SqlExpr::int(*n),
        ValueExpr::Bool(b) => SqlExpr::bool(*b),
        ValueExpr::Null => SqlExpr::Null,
        ValueExpr::FunctionCall { name, args } => {
            let sql_args: Vec<SqlExpr> = args.iter().map(value_expr_to_sql).collect();
            SqlExpr::FnCall {
                name: name.to_uppercase(),
                args: sql_args,
            }
        }
        ValueExpr::Default => SqlExpr::Default,
    }
}

/// Convert an AST Expr (from filters) to a dibs_sql::Expr.
fn ast_expr_to_sql(expr: &Expr) -> SqlExpr {
    match expr {
        Expr::Param(name) => SqlExpr::param(name),
        Expr::String(s) => SqlExpr::string(s),
        Expr::Int(n) => SqlExpr::int(*n),
        Expr::Bool(b) => SqlExpr::bool(*b),
        Expr::Null => SqlExpr::Null,
    }
}

/// Convert a Filter to a dibs_sql::Expr condition.
fn filter_to_sql(filter: &Filter) -> SqlExpr {
    let col = SqlExpr::column(&filter.column);

    match (&filter.op, &filter.value) {
        (FilterOp::IsNull, _) => col.is_null(),
        (FilterOp::IsNotNull, _) => col.is_not_null(),
        (FilterOp::Eq, Expr::Null) => col.is_null(),
        (FilterOp::Ne, Expr::Null) => col.is_not_null(),
        (FilterOp::Eq, value) => col.eq(ast_expr_to_sql(value)),
        (FilterOp::Ne, value) => SqlExpr::BinOp {
            left: Box::new(col),
            op: SqlBinOp::Ne,
            right: Box::new(ast_expr_to_sql(value)),
        },
        (FilterOp::Lt, value) => SqlExpr::BinOp {
            left: Box::new(col),
            op: SqlBinOp::Lt,
            right: Box::new(ast_expr_to_sql(value)),
        },
        (FilterOp::Lte, value) => SqlExpr::BinOp {
            left: Box::new(col),
            op: SqlBinOp::Le,
            right: Box::new(ast_expr_to_sql(value)),
        },
        (FilterOp::Gt, value) => SqlExpr::BinOp {
            left: Box::new(col),
            op: SqlBinOp::Gt,
            right: Box::new(ast_expr_to_sql(value)),
        },
        (FilterOp::Gte, value) => SqlExpr::BinOp {
            left: Box::new(col),
            op: SqlBinOp::Ge,
            right: Box::new(ast_expr_to_sql(value)),
        },
        (FilterOp::Like, value) => {
            // Use Raw for LIKE since we don't have a dedicated type
            SqlExpr::Raw(format!("\"{}\" LIKE {}", filter.column, value))
        }
        (FilterOp::ILike, value) => col.ilike(ast_expr_to_sql(value)),
        (FilterOp::In, value) => {
            // Use Raw for IN/ANY since we don't have a dedicated type
            SqlExpr::Raw(format!("\"{}\" = ANY({})", filter.column, value))
        }
        (FilterOp::JsonGet, value) => SqlExpr::Raw(format!("\"{}\" -> {}", filter.column, value)),
        (FilterOp::JsonGetText, value) => {
            SqlExpr::Raw(format!("\"{}\" ->> {}", filter.column, value))
        }
        (FilterOp::Contains, value) => SqlExpr::Raw(format!("\"{}\" @> {}", filter.column, value)),
        (FilterOp::KeyExists, value) => SqlExpr::Raw(format!("\"{}\" ? {}", filter.column, value)),
    }
}

/// Combine multiple filters with AND.
fn filters_to_where(filters: &[Filter]) -> Option<SqlExpr> {
    let mut iter = filters.iter();
    let first = iter.next()?;
    let mut result = filter_to_sql(first);
    for filter in iter {
        result = result.and(filter_to_sql(filter));
    }
    Some(result)
}

/// Generate SQL for an INSERT mutation.
pub fn generate_insert_sql(insert: &InsertMutation) -> GeneratedSql {
    let mut stmt = InsertStmt::new(&insert.table);

    for (col, expr) in &insert.values {
        stmt = stmt.column(col, value_expr_to_sql(expr));
    }

    let mut column_order = std::collections::HashMap::new();
    if !insert.returning.is_empty() {
        stmt = stmt.returning(insert.returning.iter().map(|s| s.as_str()));
        // Build column_order for RETURNING clause
        for (idx, col) in insert.returning.iter().enumerate() {
            column_order.insert(col.clone(), idx);
        }
    }

    let rendered = render(&stmt);
    GeneratedSql {
        sql: rendered.sql,
        param_order: rendered.params,
        plan: None,
        column_order,
    }
}

/// Generate SQL for an UPSERT mutation (INSERT ... ON CONFLICT ... DO UPDATE).
pub fn generate_upsert_sql(upsert: &UpsertMutation) -> GeneratedSql {
    let mut stmt = InsertStmt::new(&upsert.table);

    // Add all columns and values
    for (col, expr) in &upsert.values {
        stmt = stmt.column(col, value_expr_to_sql(expr));
    }

    // Build ON CONFLICT clause - update columns that are NOT in conflict_columns
    let update_assignments: Vec<_> = upsert
        .values
        .iter()
        .filter(|(col, _)| !upsert.conflict_columns.contains(col))
        .map(|(col, expr)| UpdateAssignment::new(col, value_expr_to_sql(expr)))
        .collect();

    stmt = stmt.on_conflict(OnConflict {
        columns: upsert.conflict_columns.clone(),
        action: ConflictAction::DoUpdate(update_assignments),
    });

    let mut column_order = std::collections::HashMap::new();
    if !upsert.returning.is_empty() {
        stmt = stmt.returning(upsert.returning.iter().map(|s| s.as_str()));
        // Build column_order for RETURNING clause
        for (idx, col) in upsert.returning.iter().enumerate() {
            column_order.insert(col.clone(), idx);
        }
    }

    let rendered = render(&stmt);
    GeneratedSql {
        sql: rendered.sql,
        param_order: rendered.params,
        plan: None,
        column_order,
    }
}

/// Generate SQL for an UPDATE mutation.
pub fn generate_update_sql(update: &UpdateMutation) -> GeneratedSql {
    let mut stmt = UpdateStmt::new(&update.table);

    // SET clause
    for (col, expr) in &update.values {
        stmt = stmt.set(col, value_expr_to_sql(expr));
    }

    // WHERE clause
    if let Some(where_expr) = filters_to_where(&update.filters) {
        stmt = stmt.where_(where_expr);
    }

    // RETURNING
    let mut column_order = std::collections::HashMap::new();
    if !update.returning.is_empty() {
        stmt = stmt.returning(update.returning.iter().map(|s| s.as_str()));
        // Build column_order for RETURNING clause
        for (idx, col) in update.returning.iter().enumerate() {
            column_order.insert(col.clone(), idx);
        }
    }

    let rendered = render(&stmt);
    GeneratedSql {
        sql: rendered.sql,
        param_order: rendered.params,
        plan: None,
        column_order,
    }
}

/// Generate SQL for a DELETE mutation.
pub fn generate_delete_sql(delete: &DeleteMutation) -> GeneratedSql {
    let mut stmt = DeleteStmt::new(&delete.table);

    // WHERE clause
    if let Some(where_expr) = filters_to_where(&delete.filters) {
        stmt = stmt.where_(where_expr);
    }

    // RETURNING
    let mut column_order = std::collections::HashMap::new();
    if !delete.returning.is_empty() {
        stmt = stmt.returning(delete.returning.iter().map(|s| s.as_str()));
        // Build column_order for RETURNING clause
        for (idx, col) in delete.returning.iter().enumerate() {
            column_order.insert(col.clone(), idx);
        }
    }

    let rendered = render(&stmt);
    GeneratedSql {
        sql: rendered.sql,
        param_order: rendered.params,
        plan: None,
        column_order,
    }
}

/// Generate SQL for a bulk INSERT mutation using UNNEST.
///
/// Generates SQL like:
/// ```sql
/// INSERT INTO products (handle, status, created_at)
/// SELECT handle, status, NOW()
/// FROM UNNEST($1::text[], $2::text[]) AS t(handle, status)
/// RETURNING id, handle, status
/// ```
pub fn generate_insert_many_sql(insert: &InsertManyMutation) -> GeneratedSql {
    let mut sql = String::new();
    let mut param_order = Vec::new();
    let mut column_order = std::collections::HashMap::new();

    // Collect param names for UNNEST
    let param_names: Vec<&str> = insert.params.iter().map(|p| p.name.as_str()).collect();

    // INSERT INTO table (columns)
    sql.push_str("INSERT INTO \"");
    sql.push_str(&insert.table);
    sql.push_str("\" (");

    let columns: Vec<&str> = insert.values.iter().map(|(col, _)| col.as_str()).collect();
    sql.push_str(
        &columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push(')');

    // SELECT expressions FROM UNNEST
    sql.push_str(" SELECT ");

    let select_exprs: Vec<String> = insert
        .values
        .iter()
        .map(|(col, expr)| value_expr_to_unnest_select(col, expr, &param_names))
        .collect();
    sql.push_str(&select_exprs.join(", "));

    // FROM UNNEST($1::type[], $2::type[], ...) AS t(col1, col2, ...)
    sql.push_str(" FROM UNNEST(");

    let unnest_params: Vec<String> = insert
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            param_order.push(p.name.clone());
            let pg_type = param_type_to_pg_array(&p.ty);
            format!("${}::{}", i + 1, pg_type)
        })
        .collect();
    sql.push_str(&unnest_params.join(", "));

    sql.push_str(") AS t(");
    sql.push_str(&param_names.join(", "));
    sql.push(')');

    // RETURNING
    if !insert.returning.is_empty() {
        sql.push_str(" RETURNING ");
        sql.push_str(
            &insert
                .returning
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect::<Vec<_>>()
                .join(", "),
        );
        for (idx, col) in insert.returning.iter().enumerate() {
            column_order.insert(col.clone(), idx);
        }
    }

    GeneratedSql {
        sql,
        param_order,
        plan: None,
        column_order,
    }
}

/// Generate SQL for a bulk UPSERT mutation using UNNEST with ON CONFLICT.
///
/// Generates SQL like:
/// ```sql
/// INSERT INTO products (handle, status, created_at)
/// SELECT handle, status, NOW()
/// FROM UNNEST($1::text[], $2::text[]) AS t(handle, status)
/// ON CONFLICT (handle) DO UPDATE SET status = EXCLUDED.status, updated_at = NOW()
/// RETURNING id, handle, status
/// ```
pub fn generate_upsert_many_sql(upsert: &UpsertManyMutation) -> GeneratedSql {
    let mut sql = String::new();
    let mut param_order = Vec::new();
    let mut column_order = std::collections::HashMap::new();

    // Collect param names for UNNEST
    let param_names: Vec<&str> = upsert.params.iter().map(|p| p.name.as_str()).collect();

    // INSERT INTO table (columns)
    sql.push_str("INSERT INTO \"");
    sql.push_str(&upsert.table);
    sql.push_str("\" (");

    let columns: Vec<&str> = upsert.values.iter().map(|(col, _)| col.as_str()).collect();
    sql.push_str(
        &columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push(')');

    // SELECT expressions FROM UNNEST
    sql.push_str(" SELECT ");

    let select_exprs: Vec<String> = upsert
        .values
        .iter()
        .map(|(col, expr)| value_expr_to_unnest_select(col, expr, &param_names))
        .collect();
    sql.push_str(&select_exprs.join(", "));

    // FROM UNNEST($1::type[], $2::type[], ...) AS t(col1, col2, ...)
    sql.push_str(" FROM UNNEST(");

    let unnest_params: Vec<String> = upsert
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            param_order.push(p.name.clone());
            let pg_type = param_type_to_pg_array(&p.ty);
            format!("${}::{}", i + 1, pg_type)
        })
        .collect();
    sql.push_str(&unnest_params.join(", "));

    sql.push_str(") AS t(");
    sql.push_str(&param_names.join(", "));
    sql.push(')');

    // ON CONFLICT (columns) DO UPDATE SET ...
    sql.push_str(" ON CONFLICT (");
    sql.push_str(
        &upsert
            .conflict_columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push_str(") DO UPDATE SET ");

    // Build update assignments - update columns that are NOT in conflict_columns
    let update_assignments: Vec<String> = upsert
        .values
        .iter()
        .filter(|(col, _)| !upsert.conflict_columns.contains(col))
        .map(|(col, expr)| {
            let value = value_expr_to_excluded(col, expr, &param_names);
            format!("\"{}\" = {}", col, value)
        })
        .collect();
    sql.push_str(&update_assignments.join(", "));

    // RETURNING
    if !upsert.returning.is_empty() {
        sql.push_str(" RETURNING ");
        sql.push_str(
            &upsert
                .returning
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect::<Vec<_>>()
                .join(", "),
        );
        for (idx, col) in upsert.returning.iter().enumerate() {
            column_order.insert(col.clone(), idx);
        }
    }

    GeneratedSql {
        sql,
        param_order,
        plan: None,
        column_order,
    }
}

/// Convert a ValueExpr to a SELECT expression for UNNEST queries.
///
/// For params that are in the UNNEST, reference them as column names.
/// For other expressions (like @now), render as SQL.
fn value_expr_to_unnest_select(_col: &str, expr: &ValueExpr, param_names: &[&str]) -> String {
    match expr {
        ValueExpr::Param(name) if param_names.contains(&name.as_str()) => {
            // Reference the UNNEST column directly
            name.clone()
        }
        ValueExpr::Param(name) => {
            // This shouldn't happen for well-formed bulk inserts, but handle it
            format!("${}", name)
        }
        ValueExpr::String(s) => {
            let escaped = s.replace('\'', "''");
            format!("'{}'", escaped)
        }
        ValueExpr::Int(n) => n.to_string(),
        ValueExpr::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        ValueExpr::Null => "NULL".to_string(),
        ValueExpr::FunctionCall { name, args } => {
            if args.is_empty() {
                format!("{}()", name.to_uppercase())
            } else {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| value_expr_to_unnest_select(_col, a, param_names))
                    .collect();
                format!("{}({})", name.to_uppercase(), arg_strs.join(", "))
            }
        }
        ValueExpr::Default => "DEFAULT".to_string(),
    }
}

/// Convert a ValueExpr to an EXCLUDED reference for ON CONFLICT DO UPDATE.
///
/// For params, use EXCLUDED.column. For other expressions, render as SQL.
fn value_expr_to_excluded(col: &str, expr: &ValueExpr, param_names: &[&str]) -> String {
    match expr {
        ValueExpr::Param(name) if param_names.contains(&name.as_str()) => {
            // Use EXCLUDED.column to reference the value that would have been inserted
            format!("EXCLUDED.\"{}\"", col)
        }
        ValueExpr::Param(name) => {
            format!("${}", name)
        }
        ValueExpr::String(s) => {
            let escaped = s.replace('\'', "''");
            format!("'{}'", escaped)
        }
        ValueExpr::Int(n) => n.to_string(),
        ValueExpr::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        ValueExpr::Null => "NULL".to_string(),
        ValueExpr::FunctionCall { name, args } => {
            if args.is_empty() {
                format!("{}()", name.to_uppercase())
            } else {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| value_expr_to_excluded(col, a, param_names))
                    .collect();
                format!("{}({})", name.to_uppercase(), arg_strs.join(", "))
            }
        }
        ValueExpr::Default => "DEFAULT".to_string(),
    }
}

/// Convert a ParamType to PostgreSQL array type.
fn param_type_to_pg_array(ty: &ParamType) -> &'static str {
    match ty {
        ParamType::String => "text[]",
        ParamType::Int => "bigint[]",
        ParamType::Bool => "boolean[]",
        ParamType::Uuid => "uuid[]",
        ParamType::Decimal => "numeric[]",
        ParamType::Timestamp => "timestamptz[]",
        ParamType::Bytes => "bytea[]",
        ParamType::Optional(inner) => {
            // For optional, use the inner type's array
            param_type_to_pg_array(inner)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_query_file;

    #[test]
    fn test_simple_select() {
        let source = r#"
AllProducts @query{
  from product
  select { id, handle, status }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        // Column order is non-deterministic due to HashMap iteration
        assert!(sql.sql.starts_with("SELECT "));
        assert!(sql.sql.contains(r#""id""#));
        assert!(sql.sql.contains(r#""handle""#));
        assert!(sql.sql.contains(r#""status""#));
        assert!(sql.sql.ends_with(r#" FROM "product""#));
        assert!(sql.param_order.is_empty());
    }

    #[test]
    fn test_select_with_where() {
        let source = r#"
ActiveProducts @query{
  from product
  where { status "published", active true }
  select { id, handle }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        // Check structure without depending on order
        assert!(sql.sql.contains("SELECT "));
        assert!(sql.sql.contains(r#""id""#));
        assert!(sql.sql.contains(r#""handle""#));
        assert!(sql.sql.contains("WHERE"));
        assert!(sql.sql.contains(r#""status" = 'published'"#));
        assert!(sql.sql.contains(r#""active" = true"#));
        assert!(sql.param_order.is_empty()); // No params for literals
    }

    #[test]
    fn test_select_with_params() {
        let source = r#"
ProductByHandle @query{
  params { handle @string }
  from product
  where { handle $handle }
  select { id, handle }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""handle" = $1"#));
        assert_eq!(sql.param_order, vec!["handle"]);
    }

    #[test]
    fn test_select_with_order_and_limit() {
        let source = r#"
RecentProducts @query{
  from product
  order-by {created_at desc}
  limit 20
  select {id, handle}
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#"ORDER BY "created_at" DESC"#));
        assert!(sql.sql.contains("LIMIT 20"));
    }

    #[test]
    fn test_null_filter() {
        let source = r#"
ActiveProducts @query{
  from product
  where { deleted_at @null }
  select { id }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""deleted_at" IS NULL"#));
    }

    #[test]
    fn test_ilike_filter() {
        let source = r#"
SearchProducts @query{
  params { q @string }
  from product
  where { handle @ilike($q) }
  select { id, handle }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""handle" ILIKE $1"#));
        assert_eq!(sql.param_order, vec!["q"]);
    }

    #[test]
    fn test_not_null_filter() {
        let source = r#"
PublishedProducts @query{
  from product
  where { published_at @not-null }
  select { id }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(
            sql.sql.contains(r#""published_at" IS NOT NULL"#),
            "SQL: {}",
            sql.sql
        );
    }

    #[test]
    fn test_gte_filter() {
        let source = r#"
FilteredProducts @query{
  params { min_price @int }
  from product
  where { price @gte($min_price) }
  select { id, price }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""price" >= $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["min_price"]);
    }

    #[test]
    fn test_lte_filter() {
        let source = r#"
FilteredProducts @query{
  params { max_price @int }
  from product
  where { price @lte($max_price) }
  select { id, price }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""price" <= $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["max_price"]);
    }

    #[test]
    fn test_ne_filter() {
        let source = r#"
NonDraftProducts @query{
  params { excluded_status @string }
  from product
  where { status @ne($excluded_status) }
  select { id, status }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""status" != $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["excluded_status"]);
    }

    #[test]
    fn test_in_filter() {
        let source = r#"
ProductsByStatus @query{
  params { statuses @string }
  from product
  where { status @in($statuses) }
  select { id, status }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(
            sql.sql.contains(r#""status" = ANY($1)"#),
            "SQL: {}",
            sql.sql
        );
        assert_eq!(sql.param_order, vec!["statuses"]);
    }

    #[test]
    fn test_json_get_operator() {
        let source = r#"
ProductWithMetadata @query{
  params { key @string }
  from product
  where { metadata @json-get($key) }
  select { id, metadata }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""metadata" -> $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["key"]);
    }

    #[test]
    fn test_json_get_operator_literal() {
        let source = r#"
ProductWithSettings @query{
  from product
  where { metadata @json-get("settings") }
  select { id, metadata }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(
            sql.sql.contains(r#""metadata" -> 'settings'"#),
            "SQL: {}",
            sql.sql
        );
        assert!(sql.param_order.is_empty());
    }

    #[test]
    fn test_json_get_text_operator() {
        let source = r#"
ProductWithJsonValue @query{
  params { key @string }
  from product
  where { metadata @json-get-text($key) }
  select { id, metadata }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""metadata" ->> $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["key"]);
    }

    #[test]
    fn test_json_contains_operator() {
        let source = r#"
ProductWithMetadataContains @query{
  params { json_value @string }
  from product
  where { metadata @contains($json_value) }
  select { id, metadata }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""metadata" @> $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["json_value"]);
    }

    #[test]
    fn test_json_key_exists_operator() {
        let source = r#"
ProductWithMetadataKey @query{
  params { key @string }
  from product
  where { metadata @key-exists($key) }
  select { id, metadata }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""metadata" ? $1"#), "SQL: {}", sql.sql);
        assert_eq!(sql.param_order, vec!["key"]);
    }

    #[test]
    fn test_json_key_exists_operator_literal() {
        let source = r#"
ProductWithLocale @query{
  from product
  where { metadata @key-exists("locale") }
  select { id, metadata }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(
            sql.sql.contains(r#""metadata" ? 'locale'"#),
            "SQL: {}",
            sql.sql
        );
        assert!(sql.param_order.is_empty());
    }

    #[test]
    fn test_pagination_literals() {
        let source = r#"
PaginatedProducts @query{
  from product
  order_by { created_at desc }
  limit 20
  offset 40
  select { id, handle }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains("LIMIT 20"), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("OFFSET 40"), "SQL: {}", sql.sql);
        assert!(sql.param_order.is_empty());
    }

    #[test]
    fn test_distinct() {
        let source = r#"
UniqueStatuses @query{
  from product
  distinct true
  select { status }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains("SELECT DISTINCT"), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("\"status\""), "SQL: {}", sql.sql);
    }

    #[test]
    fn test_distinct_on() {
        let source = r#"
LatestPerCategory @query{
  from product
  distinct-on (category_id)
  order-by {category_id asc, created_at desc}
  select {id, category_id, handle}
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let query = &file.queries[0];
        eprintln!("distinct_on: {:?}", query.distinct_on);
        eprintln!("order_by: {:?}", query.order_by);
        let sql = generate_simple_sql(query);
        eprintln!("Generated SQL: {}", sql.sql);

        assert!(
            sql.sql.contains("SELECT DISTINCT ON (\"category_id\")"),
            "SQL: {}",
            sql.sql
        );
        assert!(
            sql.sql
                .contains("ORDER BY \"category_id\" ASC, \"created_at\" DESC"),
            "SQL: {}",
            sql.sql
        );
    }

    #[test]
    fn test_distinct_on_multiple_columns() {
        let source = r#"
DistinctProducts @query{
  from product
  distinct-on (brand category)
  order-by {brand asc, category asc, created_at desc}
  select {id, brand, category, handle}
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let query = &file.queries[0];
        eprintln!("distinct_on: {:?}", query.distinct_on);
        eprintln!("order_by: {:?}", query.order_by);
        let sql = generate_simple_sql(query);
        eprintln!("Generated SQL: {}", sql.sql);

        assert!(
            sql.sql
                .contains("SELECT DISTINCT ON (\"brand\", \"category\")"),
            "SQL: {}",
            sql.sql
        );
    }

    #[test]
    fn test_pagination_params() {
        let source = r#"
PaginatedProducts @query{
  params { page_size @int, page_offset @int }
  from product
  order_by { created_at desc }
  limit $page_size
  offset $page_offset
  select { id, handle }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains("LIMIT $1"));
        assert!(sql.sql.contains("OFFSET $2"));
        assert_eq!(sql.param_order, vec!["page_size", "page_offset"]);
    }

    #[test]
    fn test_sql_with_joins() {
        use crate::planner::{PlannerForeignKey, PlannerSchema, PlannerTable};

        let source = r#"
ProductWithTranslation @query{
  params { handle @string }
  from product
  where { handle $handle }
  select {
    id
    handle
    translation @rel{
      from product_translation
      first true
      select { title, description }
    }
  }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();

        // Build test schema
        let mut schema = PlannerSchema::default();
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string(), "handle".to_string()],
                foreign_keys: vec![],
            },
        );
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "title".to_string(),
                    "description".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let sql = generate_sql_with_joins(&file.queries[0], Some(&schema)).unwrap();

        // Check SELECT
        assert!(sql.sql.contains("\"t0\".\"id\""));
        assert!(sql.sql.contains("\"t0\".\"handle\""));
        assert!(sql.sql.contains("\"t1\".\"title\""));
        assert!(sql.sql.contains("\"t1\".\"description\""));

        // Check FROM with JOIN
        assert!(sql.sql.contains("FROM \"product\" AS \"t0\""));
        assert!(
            sql.sql
                .contains("LEFT JOIN \"product_translation\" AS \"t1\"")
        );
        assert!(sql.sql.contains("ON t0.id = t1.product_id"));

        // Check WHERE
        assert!(sql.sql.contains("\"t0\".\"handle\" = $1"));

        // Check param order
        assert_eq!(sql.param_order, vec!["handle"]);

        // Check plan exists
        assert!(sql.plan.is_some());
    }

    #[test]
    fn test_sql_with_relation_where_literal() {
        use crate::planner::{PlannerForeignKey, PlannerSchema, PlannerTable};

        let source = r#"
ProductWithEnglishTranslation @query{
  from product
  select {
    id
    translation @rel{
      from product_translation
      where { locale en }
      first true
      select { title }
    }
  }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();

        let mut schema = PlannerSchema::default();
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string()],
                foreign_keys: vec![],
            },
        );
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "locale".to_string(),
                    "title".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let sql = generate_sql_with_joins(&file.queries[0], Some(&schema)).unwrap();

        // Check that relation filter is in the ON clause
        assert!(
            sql.sql
                .contains("ON t0.id = t1.product_id AND \"t1\".\"locale\" = 'en'"),
            "Expected relation filter in ON clause, got: {}",
            sql.sql
        );
    }

    #[test]
    fn test_sql_with_relation_where_param() {
        use crate::planner::{PlannerForeignKey, PlannerSchema, PlannerTable};

        let source = r#"
ProductWithTranslation @query{
  params { locale @string }
  from product
  select {
    id
    translation @rel{
      from product_translation
      where { locale $locale }
      first true
      select { title }
    }
  }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();

        let mut schema = PlannerSchema::default();
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string()],
                foreign_keys: vec![],
            },
        );
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "locale".to_string(),
                    "title".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let sql = generate_sql_with_joins(&file.queries[0], Some(&schema)).unwrap();

        // Check that relation filter is in the ON clause with param placeholder
        assert!(
            sql.sql
                .contains("ON t0.id = t1.product_id AND \"t1\".\"locale\" = $1"),
            "Expected relation filter with param in ON clause, got: {}",
            sql.sql
        );

        // Check param order includes the relation param
        assert_eq!(sql.param_order, vec!["locale"]);
    }

    #[test]
    fn test_sql_with_relation_where_and_base_where() {
        use crate::planner::{PlannerForeignKey, PlannerSchema, PlannerTable};

        let source = r#"
ProductWithTranslation @query{
  params { handle @string, locale @string }
  from product
  where { handle $handle }
  select {
    id
    translation @rel{
      from product_translation
      where { locale $locale }
      first true
      select { title }
    }
  }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();

        let mut schema = PlannerSchema::default();
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string(), "handle".to_string()],
                foreign_keys: vec![],
            },
        );
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "locale".to_string(),
                    "title".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let sql = generate_sql_with_joins(&file.queries[0], Some(&schema)).unwrap();

        // Relation filter should be $1 (comes first in FROM clause)
        assert!(
            sql.sql.contains("\"t1\".\"locale\" = $1"),
            "Expected relation filter as $1, got: {}",
            sql.sql
        );

        // Base WHERE filter should be $2 (comes after FROM clause)
        assert!(
            sql.sql.contains("\"t0\".\"handle\" = $2"),
            "Expected base filter as $2, got: {}",
            sql.sql
        );

        // Check param order: relation params first, then base WHERE params
        assert_eq!(sql.param_order, vec!["locale", "handle"]);
    }

    #[test]
    fn test_insert_sql() {
        let source = r#"
CreateUser @insert{
  params {
    name @string
    email @string
  }
  into users
  values {
    name $name
    email $email
    created_at @now
  }
  returning { id, name, email, created_at }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_insert_sql(&file.inserts[0]);

        assert!(sql.sql.contains("INSERT INTO \"users\""));
        assert!(sql.sql.contains("\"name\""));
        assert!(sql.sql.contains("\"email\""));
        assert!(sql.sql.contains("\"created_at\""));
        assert!(sql.sql.contains("NOW()"));
        assert!(sql.sql.contains("RETURNING"));
        assert_eq!(sql.param_order.len(), 2);
    }

    #[test]
    fn test_upsert_sql() {
        let source = r#"
UpsertProduct @upsert{
  params {
    id @uuid
    name @string
    price @decimal
  }
  into products
  on-conflict {
    target { id }
    update { name, price, updated_at @now }
  }
  values {
    id $id
    name $name
    price $price
  }
  returning { id, name, price, updated_at }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_upsert_sql(&file.upserts[0]);

        assert!(sql.sql.contains("INSERT INTO \"products\""));
        assert!(sql.sql.contains("ON CONFLICT (\"id\")"));
        assert!(sql.sql.contains("DO UPDATE SET"));
        // id should NOT be in the update set
        assert!(!sql.sql.contains("\"id\" = $"));
        assert!(sql.sql.contains("\"name\" ="));
        assert!(sql.sql.contains("\"price\" ="));
        assert!(sql.sql.contains("RETURNING"));
    }

    #[test]
    fn test_update_sql() {
        let source = r#"
UpdateUserEmail @update{
  params {
    id @uuid
    email @string
  }
  table users
  set {
    email $email
    updated_at @now
  }
  where { id $id }
  returning { id, email, updated_at }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_update_sql(&file.updates[0]);

        assert!(sql.sql.contains("UPDATE \"users\" SET"));
        assert!(sql.sql.contains("\"email\" = $1"));
        assert!(sql.sql.contains("\"updated_at\" = NOW()"));
        assert!(sql.sql.contains("WHERE \"id\" = $2"));
        assert!(sql.sql.contains("RETURNING"));
        assert_eq!(sql.param_order, vec!["email", "id"]);
    }

    #[test]
    fn test_delete_sql() {
        let source = r#"
DeleteUser @delete{
  params {
    id @uuid
  }
  from users
  where { id $id }
  returning { id }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        let sql = generate_delete_sql(&file.deletes[0]);

        assert!(sql.sql.contains("DELETE FROM \"users\""));
        assert!(sql.sql.contains("WHERE \"id\" = $1"));
        assert!(sql.sql.contains("RETURNING \"id\""));
        assert_eq!(sql.param_order, vec!["id"]);
    }

    #[test]
    fn test_relation_order_by_lateral() {
        use crate::planner::{PlannerForeignKey, PlannerSchema, PlannerTable};

        let source = r#"
ProductWithLatestTranslation @query{
  from product
  select {
    id
    translation @rel{
      from product_translation
      order-by {updated_at desc}
      first true
      select {title, description}
    }
  }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();

        let mut schema = PlannerSchema::default();
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string()],
                foreign_keys: vec![],
            },
        );
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "title".to_string(),
                    "description".to_string(),
                    "updated_at".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let sql = generate_sql_with_joins(&file.queries[0], Some(&schema)).unwrap();

        // Should use LATERAL join for first:true with order_by
        assert!(
            sql.sql.contains("LEFT JOIN LATERAL"),
            "Expected LATERAL join, got: {}",
            sql.sql
        );

        // Should have ORDER BY in the subquery
        assert!(
            sql.sql.contains("ORDER BY \"updated_at\" DESC"),
            "Expected ORDER BY in LATERAL subquery, got: {}",
            sql.sql
        );

        // Should have LIMIT 1
        assert!(
            sql.sql.contains("LIMIT 1"),
            "Expected LIMIT 1 in LATERAL subquery, got: {}",
            sql.sql
        );

        // Should join ON true (LATERAL handles the correlation)
        assert!(
            sql.sql.contains("ON true"),
            "Expected ON true for LATERAL join, got: {}",
            sql.sql
        );
    }

    #[test]
    fn test_relation_order_by_with_filter() {
        use crate::planner::{PlannerForeignKey, PlannerSchema, PlannerTable};

        let source = r#"
ProductWithLatestEnglishTranslation @query{
  params {locale @string}
  from product
  select {
    id
    translation @rel{
      from product_translation
      where {locale $locale}
      order-by {updated_at desc}
      first true
      select {title}
    }
  }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();

        let mut schema = PlannerSchema::default();
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string()],
                foreign_keys: vec![],
            },
        );
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "locale".to_string(),
                    "title".to_string(),
                    "updated_at".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let sql = generate_sql_with_joins(&file.queries[0], Some(&schema)).unwrap();

        // Should use LATERAL
        assert!(
            sql.sql.contains("LEFT JOIN LATERAL"),
            "Expected LATERAL join, got: {}",
            sql.sql
        );

        // Should have locale filter in the subquery with $1
        assert!(
            sql.sql.contains("\"locale\" = $1"),
            "Expected locale filter in LATERAL subquery, got: {}",
            sql.sql
        );

        // Param should be tracked
        assert_eq!(sql.param_order, vec!["locale"]);
    }

    #[test]
    fn test_insert_many_sql() {
        let source = r#"
BulkCreateProducts @insert-many{
  params {
    handle @string
    status @string
  }
  into products
  values {
    handle $handle
    status $status
    created_at @now
  }
  returning { id, handle, status }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        assert_eq!(file.insert_manys.len(), 1);

        let sql = generate_insert_many_sql(&file.insert_manys[0]);

        // Check INSERT INTO with correct columns
        assert!(
            sql.sql.contains("INSERT INTO \"products\""),
            "SQL: {}",
            sql.sql
        );
        assert!(sql.sql.contains("\"handle\""), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("\"status\""), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("\"created_at\""), "SQL: {}", sql.sql);

        // Check SELECT FROM UNNEST pattern
        assert!(sql.sql.contains("SELECT"), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("FROM UNNEST("), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("$1::text[]"), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("$2::text[]"), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("AS t(handle, status)"), "SQL: {}", sql.sql);

        // Check NOW() function is in select
        assert!(sql.sql.contains("NOW()"), "SQL: {}", sql.sql);

        // Check RETURNING
        assert!(sql.sql.contains("RETURNING"), "SQL: {}", sql.sql);

        // Check params
        assert_eq!(sql.param_order, vec!["handle", "status"]);
    }

    #[test]
    fn test_upsert_many_sql() {
        let source = r#"
BulkUpsertProducts @upsert-many{
  params {
    handle @string
    status @string
  }
  into products
  on-conflict {
    target { handle }
    update { status, updated_at @now }
  }
  values {
    handle $handle
    status $status
    created_at @now
  }
  returning { id, handle, status }
}
"#;
        let file = parse_query_file("<test>", source).unwrap();
        assert_eq!(file.upsert_manys.len(), 1);

        let sql = generate_upsert_many_sql(&file.upsert_manys[0]);

        // Check INSERT INTO
        assert!(
            sql.sql.contains("INSERT INTO \"products\""),
            "SQL: {}",
            sql.sql
        );

        // Check SELECT FROM UNNEST
        assert!(sql.sql.contains("SELECT"), "SQL: {}", sql.sql);
        assert!(sql.sql.contains("FROM UNNEST("), "SQL: {}", sql.sql);

        // Check ON CONFLICT
        assert!(
            sql.sql.contains("ON CONFLICT (\"handle\")"),
            "SQL: {}",
            sql.sql
        );
        assert!(sql.sql.contains("DO UPDATE SET"), "SQL: {}", sql.sql);

        // Check that handle is NOT in update (it's the conflict target)
        let update_part = sql.sql.split("DO UPDATE SET").nth(1).unwrap();
        assert!(
            !update_part.contains("\"handle\" ="),
            "handle should not be in UPDATE SET: {}",
            sql.sql
        );

        // Check that status uses EXCLUDED
        assert!(sql.sql.contains("EXCLUDED.\"status\""), "SQL: {}", sql.sql);

        // Check that updated_at uses NOW()
        assert!(
            update_part.contains("NOW()"),
            "updated_at should use NOW(): {}",
            sql.sql
        );

        // Check RETURNING
        assert!(sql.sql.contains("RETURNING"), "SQL: {}", sql.sql);

        // Check params
        assert_eq!(sql.param_order, vec!["handle", "status"]);
    }
}
