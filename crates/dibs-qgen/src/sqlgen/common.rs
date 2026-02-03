//! Common SQL generation utilities shared across statement types.

use dibs_query_schema::{FilterValue, Meta, Payload, UpdateValue, ValueExpr, Where};
use dibs_sql::{BinOp, ColumnName, Expr, TableName};

/// Convert a column name (possibly qualified like "t0.id") to an Expr.
///
/// If the column name contains a dot, it's treated as "table.column".
/// Otherwise, it's an unqualified column reference.
fn column_name_to_expr(column: &ColumnName) -> Expr {
    let s = column.as_str();
    if let Some(dot_pos) = s.find('.') {
        let table: TableName = s[..dot_pos].into();
        let col: ColumnName = s[dot_pos + 1..].into();
        Expr::qualified_column(table, col)
    } else {
        Expr::column(column.clone())
    }
}

/// Extract the unqualified column name from a possibly qualified name.
///
/// "t0.id" -> "id"
/// "id" -> "id"
fn extract_column_name(column: &ColumnName) -> &str {
    let s = column.as_str();
    if let Some(dot_pos) = s.rfind('.') {
        &s[dot_pos + 1..]
    } else {
        s
    }
}

/// Convert a Meta<String> argument to an Expr.
///
/// If it starts with '$', it's a parameter reference.
/// Otherwise, it's a string literal, integer, or boolean.
pub fn meta_string_to_expr(meta: &Meta<String>) -> Expr {
    let s = &meta.value;
    if let Some(param_name) = s.strip_prefix('$') {
        Expr::param(param_name.into())
    } else {
        // Try to parse as integer
        if let Ok(n) = s.parse::<i64>() {
            Expr::int(n)
        } else if s == "true" {
            Expr::bool(true)
        } else if s == "false" {
            Expr::bool(false)
        } else {
            Expr::string(s)
        }
    }
}

/// Convert a FilterValue to a dibs_sql::Expr.
///
/// The column can be either a simple name like "id" or a qualified name like "t0.id".
/// Qualified names are parsed and converted to qualified column expressions.
pub fn filter_value_to_expr(column: &ColumnName, filter: &FilterValue) -> Option<Expr> {
    let col = column_name_to_expr(column);

    // For EqBare shorthand, we need the unqualified column name for the param
    let unqualified_name = extract_column_name(column);

    match filter {
        FilterValue::Null => Some(col.is_null()),
        FilterValue::NotNull => Some(col.is_not_null()),

        FilterValue::Eq(args) => {
            let arg = args.first()?;
            Some(col.eq(meta_string_to_expr(arg)))
        }

        FilterValue::EqBare(opt_meta) => {
            if let Some(meta) = opt_meta {
                Some(col.eq(meta_string_to_expr(meta)))
            } else {
                // Shorthand: {id} means {id $id} - use unqualified column name as param name
                Some(col.eq(Expr::param(unqualified_name.into())))
            }
        }

        FilterValue::Ne(args) => {
            let arg = args.first()?;
            Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Ne,
                right: Box::new(meta_string_to_expr(arg)),
            })
        }

        FilterValue::Lt(args) => {
            let arg = args.first()?;
            Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Lt,
                right: Box::new(meta_string_to_expr(arg)),
            })
        }

        FilterValue::Lte(args) => {
            let arg = args.first()?;
            Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Le,
                right: Box::new(meta_string_to_expr(arg)),
            })
        }

        FilterValue::Gt(args) => {
            let arg = args.first()?;
            Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Gt,
                right: Box::new(meta_string_to_expr(arg)),
            })
        }

        FilterValue::Gte(args) => {
            let arg = args.first()?;
            Some(Expr::BinOp {
                left: Box::new(col),
                op: BinOp::Ge,
                right: Box::new(meta_string_to_expr(arg)),
            })
        }

        FilterValue::Like(args) => {
            let arg = args.first()?;
            Some(col.like(meta_string_to_expr(arg)))
        }

        FilterValue::Ilike(args) => {
            let arg = args.first()?;
            Some(col.ilike(meta_string_to_expr(arg)))
        }

        FilterValue::In(args) => {
            let arg = args.first()?;
            Some(col.any(meta_string_to_expr(arg)))
        }

        FilterValue::JsonGet(args) => {
            let arg = args.first()?;
            Some(col.json_get(meta_string_to_expr(arg)))
        }

        FilterValue::JsonGetText(args) => {
            let arg = args.first()?;
            Some(col.json_get_text(meta_string_to_expr(arg)))
        }

        FilterValue::Contains(args) => {
            let arg = args.first()?;
            Some(col.contains(meta_string_to_expr(arg)))
        }

        FilterValue::KeyExists(args) => {
            let arg = args.first()?;
            Some(col.key_exists(meta_string_to_expr(arg)))
        }
    }
}

/// Convert a WHERE clause to a dibs_sql::Expr.
pub fn where_to_expr(where_clause: &Where) -> Option<Expr> {
    let mut exprs: Vec<Expr> = vec![];

    for (col_meta, filter_value) in &where_clause.filters {
        let col_name = &col_meta.value;
        if let Some(expr) = filter_value_to_expr(col_name, filter_value) {
            exprs.push(expr);
        }
    }

    // AND all expressions together
    let mut iter = exprs.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, expr| acc.and(expr)))
}

/// Convert a ValueExpr to a dibs_sql::Expr.
///
/// Handles:
/// - `@default` -> DEFAULT
/// - `@funcname` -> FUNCNAME()
/// - `@funcname(args...)` -> FUNCNAME(args...)
/// - `$param` -> parameter reference
/// - literals -> string/int/bool literals
pub fn value_expr_to_expr(column: &ColumnName, expr: &Option<ValueExpr>) -> Expr {
    match expr {
        None => {
            // Shorthand: {col} means {col $col}
            Expr::param(column.as_str().into())
        }
        Some(ValueExpr::Default) => Expr::Default,
        Some(ValueExpr::Other { tag, content }) => match (tag, content) {
            // Bare scalar (param reference like $name or literal)
            (None, Some(Payload::Scalar(s))) => meta_string_to_expr(s),
            // Nullary function like @now
            (Some(name), None) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![],
            },
            // Function with args like @coalesce($a, $b)
            (Some(name), Some(Payload::Seq(args))) => {
                let sql_args: Vec<Expr> = args
                    .iter()
                    .map(|a| value_expr_to_expr(column, &Some(a.clone())))
                    .collect();
                Expr::FnCall {
                    name: name.to_uppercase(),
                    args: sql_args,
                }
            }
            // Function with single scalar arg
            (Some(name), Some(Payload::Scalar(s))) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![meta_string_to_expr(s)],
            },
            // Shouldn't happen but handle gracefully
            (None, None) => Expr::Null,
            // Sequence without tag - shouldn't happen
            (None, Some(Payload::Seq(_))) => Expr::Null,
        },
    }
}

/// Convert an UpdateValue to a dibs_sql::Expr.
///
/// Similar to value_expr_to_expr but for the update clause in upserts.
pub fn update_value_to_expr(column: &ColumnName, expr: &Option<UpdateValue>) -> Expr {
    match expr {
        None => {
            // Shorthand: {col} means use EXCLUDED.col
            Expr::excluded(column.clone())
        }
        Some(UpdateValue::Default) => Expr::Default,
        Some(UpdateValue::Other { tag, content }) => match (tag, content) {
            // Bare scalar (param reference like $name or literal)
            (None, Some(Payload::Scalar(s))) => meta_string_to_expr(s),
            // Nullary function like @now
            (Some(name), None) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![],
            },
            // Function with args
            (Some(name), Some(Payload::Seq(args))) => {
                // Convert each arg - but we need ValueExpr not UpdateValue
                // For now, handle the common cases
                let sql_args: Vec<Expr> = args
                    .iter()
                    .map(|a| value_expr_to_expr(column, &Some(a.clone())))
                    .collect();
                Expr::FnCall {
                    name: name.to_uppercase(),
                    args: sql_args,
                }
            }
            // Function with single scalar arg
            (Some(name), Some(Payload::Scalar(s))) => Expr::FnCall {
                name: name.to_uppercase(),
                args: vec![meta_string_to_expr(s)],
            },
            // Shouldn't happen but handle gracefully
            (None, None) => Expr::Null,
            // Sequence without tag - shouldn't happen
            (None, Some(Payload::Seq(_))) => Expr::Null,
        },
    }
}
