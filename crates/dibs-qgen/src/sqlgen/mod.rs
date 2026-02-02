//! SQL generation from query schema types.

mod delete;
mod select;

pub use delete::{GeneratedDelete, generate_delete_sql};
use indexmap::IndexMap;
pub use select::{GeneratedSelect, generate_select_sql};

#[allow(unused_imports)]
use dibs_query_schema::{ParamType, ValueExpr};

use crate::QueryPlan;

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
    pub column_order: IndexMap<String, usize>,
}

/// Format a single filter condition from a column name and FilterValue.
///
/// Returns the SQL condition string and the updated param index.
fn format_filter(
    column: &str,
    filter_value: &dibs_query_schema::FilterValue,
    mut param_idx: usize,
    param_order: &mut Vec<String>,
) -> (String, usize) {
    use dibs_query_schema::FilterValue;

    // Handle dotted column names (e.g., "t0.column") by quoting each part
    let col = if column.contains('.') {
        column
            .split('.')
            .map(|part| format!("\"{}\"", part))
            .collect::<Vec<_>>()
            .join(".")
    } else {
        format!("\"{}\"", column)
    };

    /// Extract param name or literal from a Meta<String>.
    /// Returns (is_param, name_or_value).
    fn parse_arg(arg: &dibs_query_schema::Meta<String>) -> (bool, &str) {
        let s = arg.value.as_str();
        if let Some(name) = s.strip_prefix('$') {
            (true, name)
        } else {
            (false, s)
        }
    }

    /// Format a comparison operator with a single argument.
    fn format_comparison(
        col: &str,
        op: &str,
        args: &[dibs_query_schema::Meta<String>],
        param_idx: &mut usize,
        param_order: &mut Vec<String>,
    ) -> String {
        if let Some(arg) = args.first() {
            let (is_param, value) = parse_arg(arg);
            if is_param {
                param_order.push(value.to_string());
                let s = format!("{} {} ${}", col, op, *param_idx);
                *param_idx += 1;
                s
            } else {
                let escaped = value.replace('\'', "''");
                format!("{} {} '{}'", col, op, escaped)
            }
        } else {
            format!("{} {} NULL", col, op)
        }
    }

    let result = match filter_value {
        FilterValue::Null => format!("{} IS NULL", col),
        FilterValue::NotNull => format!("{} IS NOT NULL", col),
        FilterValue::Eq(args) => format_comparison(&col, "=", args, &mut param_idx, param_order),
        FilterValue::EqBare(opt_meta) => {
            if let Some(meta) = opt_meta {
                let (is_param, value) = parse_arg(meta);
                if is_param {
                    param_order.push(value.to_string());
                    let s = format!("{} = ${}", col, param_idx);
                    param_idx += 1;
                    s
                } else {
                    let escaped = value.replace('\'', "''");
                    format!("{} = '{}'", col, escaped)
                }
            } else {
                format!("{} IS NULL", col)
            }
        }
        FilterValue::Ne(args) => format_comparison(&col, "!=", args, &mut param_idx, param_order),
        FilterValue::Lt(args) => format_comparison(&col, "<", args, &mut param_idx, param_order),
        FilterValue::Lte(args) => format_comparison(&col, "<=", args, &mut param_idx, param_order),
        FilterValue::Gt(args) => format_comparison(&col, ">", args, &mut param_idx, param_order),
        FilterValue::Gte(args) => format_comparison(&col, ">=", args, &mut param_idx, param_order),
        FilterValue::Like(args) => {
            format_comparison(&col, "LIKE", args, &mut param_idx, param_order)
        }
        FilterValue::Ilike(args) => {
            format_comparison(&col, "ILIKE", args, &mut param_idx, param_order)
        }
        FilterValue::In(args) => {
            if let Some(arg) = args.first() {
                let (is_param, value) = parse_arg(arg);
                if is_param {
                    param_order.push(value.to_string());
                    let s = format!("{} = ANY(${})", col, param_idx);
                    param_idx += 1;
                    s
                } else {
                    format!("{} = ANY(ARRAY[{}])", col, value)
                }
            } else {
                format!("{} = ANY(ARRAY[])", col)
            }
        }
        FilterValue::JsonGet(args) => {
            format_comparison(&col, "->", args, &mut param_idx, param_order)
        }
        FilterValue::JsonGetText(args) => {
            format_comparison(&col, "->>", args, &mut param_idx, param_order)
        }
        FilterValue::Contains(args) => {
            format_comparison(&col, "@>", args, &mut param_idx, param_order)
        }
        FilterValue::KeyExists(args) => {
            format_comparison(&col, "?", args, &mut param_idx, param_order)
        }
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

    let mut column_order = HashMap::new();
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

    let mut column_order = HashMap::new();
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
    let mut column_order = HashMap::new();
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
    let mut column_order = HashMap::new();
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
    let mut column_order = HashMap::new();

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
    let mut column_order = HashMap::new();

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
    use dibs_query_schema::Payload;
    match expr {
        ValueExpr::Default => "DEFAULT".to_string(),
        ValueExpr::Other { tag, content } => match (tag, content) {
            // Bare scalar (param reference like $name)
            (None, Some(Payload::Scalar(s))) => {
                let s = s.value();
                if let Some(name) = s.strip_prefix('$') {
                    if param_names.contains(&name) {
                        // Reference the UNNEST column directly
                        name.to_string()
                    } else {
                        // This shouldn't happen for well-formed bulk inserts, but handle it
                        format!("${}", name)
                    }
                } else {
                    // Literal value, pass through
                    s.to_string()
                }
            }
            // Nullary function like @now
            (Some(name), None) => format!("{}()", name.to_uppercase()),
            // Function with args like @coalesce($a, $b)
            (Some(name), Some(Payload::Seq(args))) => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| value_expr_to_unnest_select(_col, a, param_names))
                    .collect();
                format!("{}({})", name.to_uppercase(), arg_strs.join(", "))
            }
            // Other cases shouldn't happen but handle gracefully
            _ => "NULL".to_string(),
        },
    }
}

/// Convert a ValueExpr to an EXCLUDED reference for ON CONFLICT DO UPDATE.
///
/// For params, use EXCLUDED.column. For other expressions, render as SQL.
fn value_expr_to_excluded(col: &str, expr: &ValueExpr, param_names: &[&str]) -> String {
    use dibs_query_schema::Payload;
    match expr {
        ValueExpr::Default => "DEFAULT".to_string(),
        ValueExpr::Other { tag, content } => match (tag, content) {
            // Bare scalar (param reference like $name)
            (None, Some(Payload::Scalar(s))) => {
                let s = s.value();
                if let Some(name) = s.strip_prefix('$') {
                    if param_names.contains(&name) {
                        format!("EXCLUDED.\"{}\"", col)
                    } else {
                        format!("${}", name)
                    }
                } else {
                    // Literal value, pass through
                    s.to_string()
                }
            }
            // Nullary function like @now
            (Some(name), None) => format!("{}()", name.to_uppercase()),
            // Function with args like @coalesce($a, $b)
            (Some(name), Some(Payload::Seq(args))) => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| value_expr_to_excluded(col, a, param_names))
                    .collect();
                format!("{}({})", name.to_uppercase(), arg_strs.join(", "))
            }
            // Other cases shouldn't happen but handle gracefully
            _ => "NULL".to_string(),
        },
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
