//! SQL generation from query schema types.

use crate::{QError, QueryPlan, QueryPlanner};
use dibs_db_schema::Schema;
use dibs_query_schema::{ParamType, Query, ValueExpr};
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

/// Generate SQL for a query with optional JOINs using the planner.
///
/// If schema is None or the query has no relations/COUNT fields, falls back to simple SQL generation.
pub fn generate_sql(
    query: &Query,
    schema: Option<&Schema>,
) -> Result<GeneratedSql, crate::planner::PlanError> {
    // Check if query needs the planner (has relations or COUNT fields)
    let needs_planner = query
        .select
        .as_ref()
        .map(|sel| sel.has_relations() || sel.has_count())
        .unwrap_or(false);

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

/// Format a filter value for the new schema structure.
fn format_filter_value(
    col: &str,
    filter_value: &FilterValue,
    mut param_idx: usize,
    param_order: &mut Vec<String>,
) -> Result<(String, usize), QError> {
    let col_quoted = format!("\"{col}\"");
    let mut used_param = false;

    let result = match filter_value {
        FilterValue::Null => format!("{col_quoted} IS NULL"),
        FilterValue::NotNull => format!("{col_quoted} IS NOT NULL"),
        FilterValue::EqBare(opt_meta) => {
            if let Some(meta) = opt_meta {
                let val_str = meta.as_str();
                if val_str.starts_with('$') {
                    param_order.push(val_str[1..].to_string());
                    used_param = true;
                    format!("{col_quoted} = ${param_idx}")
                } else {
                    let escaped = escape_sql_string(val_str);
                    format!("{col_quoted} = '{escaped}'")
                }
            } else {
                format!("{col_quoted} IS NULL")
            }
        }
        FilterValue::Eq(args) => {
            let parsed = EQ_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} = ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} = '{escaped}'")
                }
            }
        }
        FilterValue::Ne(args) => {
            let parsed = NE_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} != ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} != '{escaped}'")
                }
            }
        }
        FilterValue::Lt(args) => {
            let parsed = LT_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} < ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} < '{escaped}'")
                }
            }
        }
        FilterValue::Lte(args) => {
            let parsed = LTE_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} <= ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} <= '{escaped}'")
                }
            }
        }
        FilterValue::Gt(args) => {
            let parsed = GT_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} > ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} > '{escaped}'")
                }
            }
        }
        FilterValue::Gte(args) => {
            let parsed = GTE_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} >= ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} >= '{escaped}'")
                }
            }
        }
        FilterValue::Like(args) => {
            let parsed = LIKE_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} LIKE ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} LIKE '{escaped}'")
                }
            }
        }
        FilterValue::Ilike(args) => {
            let parsed = ILIKE_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} ILIKE ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} ILIKE '{escaped}'")
                }
            }
        }
        FilterValue::In(args) => {
            let parsed = IN_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} = ANY(${param_idx})")
                }
                FilterArg::Literal(value) => {
                    format!("{col_quoted} = ANY(ARRAY[{value}])")
                }
            }
        }
        FilterValue::JsonGet(args) => {
            let parsed = JSON_GET_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} -> ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} -> '{escaped}'")
                }
            }
        }
        FilterValue::JsonGetText(args) => {
            let parsed = JSON_GET_TEXT_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} ->> ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} ->> '{escaped}'")
                }
            }
        }
        FilterValue::Contains(args) => {
            let parsed = CONTAINS_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} @> ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} @> '{escaped}'")
                }
            }
        }
        FilterValue::KeyExists(args) => {
            let parsed = KEY_EXISTS_SPEC.parse_args(args)?;
            match &parsed[0] {
                FilterArg::Variable(var_name) => {
                    param_order.push(var_name.clone());
                    used_param = true;
                    format!("{col_quoted} ? ${param_idx}")
                }
                FilterArg::Literal(value) => {
                    let escaped = escape_sql_string(value);
                    format!("{col_quoted} ? '{escaped}'")
                }
            }
        }
    };

    if used_param {
        param_idx += 1;
    }
    Ok((result, param_idx))
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
        ParamType::Optional(inner_vec) => {
            // For optional, use the inner type's array (from first element if available)
            if let Some(inner) = inner_vec.first() {
                param_type_to_pg_array(inner)
            } else {
                "text[]" // fallback for empty optional
            }
        }
    }
}

#[cfg(test)]
mod tests;
