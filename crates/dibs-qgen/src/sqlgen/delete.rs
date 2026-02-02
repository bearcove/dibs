//! SQL generation for DELETE statements.

use dibs_query_schema::{Delete, FilterValue, Meta, Where};
use dibs_sql::{ColumnName, DeleteStmt, Expr, ParamName, render};

/// Generated SQL with parameter info.
#[derive(Debug, Clone)]
pub struct GeneratedDelete {
    /// The rendered SQL string with $1, $2, etc. placeholders.
    pub sql: String,
    /// Parameter names in order (maps to $1, $2, etc.).
    pub params: Vec<ParamName>,
    /// Column names in RETURNING order (for index-based access).
    pub returning_columns: Vec<ColumnName>,
}

/// Generate SQL for a DELETE statement.
pub fn generate_delete_sql(delete: &Delete) -> GeneratedDelete {
    let mut stmt = DeleteStmt::new(delete.from.value.clone());

    // WHERE clause
    if let Some(where_clause) = &delete.where_clause {
        if let Some(expr) = where_to_expr(where_clause) {
            stmt = stmt.where_(expr);
        }
    }

    // RETURNING clause
    let returning_columns: Vec<ColumnName> = if let Some(returning) = &delete.returning {
        returning.columns.keys().map(|k| k.value.clone()).collect()
    } else {
        vec![]
    };

    for col in &returning_columns {
        stmt = stmt.returning([col.clone()]);
    }

    let rendered = render(&stmt);

    GeneratedDelete {
        sql: rendered.sql,
        params: rendered.params,
        returning_columns,
    }
}

/// Convert a WHERE clause to a dibs_sql::Expr.
fn where_to_expr(where_clause: &Where) -> Option<Expr> {
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

/// Convert a FilterValue to a dibs_sql::Expr.
fn filter_value_to_expr(column: &ColumnName, filter: &FilterValue) -> Option<Expr> {
    let col = Expr::column(column.clone());

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
                // Shorthand: {id} means {id $id} - use column name as param name
                Some(col.eq(Expr::param(column.as_str().into())))
            }
        }

        FilterValue::Ne(args) => {
            let arg = args.first()?;
            let right = meta_string_to_expr(arg);
            Some(Expr::BinOp {
                left: Box::new(col),
                op: dibs_sql::BinOp::Ne,
                right: Box::new(right),
            })
        }

        FilterValue::Lt(args) => {
            let arg = args.first()?;
            let right = meta_string_to_expr(arg);
            Some(Expr::BinOp {
                left: Box::new(col),
                op: dibs_sql::BinOp::Lt,
                right: Box::new(right),
            })
        }

        FilterValue::Lte(args) => {
            let arg = args.first()?;
            let right = meta_string_to_expr(arg);
            Some(Expr::BinOp {
                left: Box::new(col),
                op: dibs_sql::BinOp::Le,
                right: Box::new(right),
            })
        }

        FilterValue::Gt(args) => {
            let arg = args.first()?;
            let right = meta_string_to_expr(arg);
            Some(Expr::BinOp {
                left: Box::new(col),
                op: dibs_sql::BinOp::Gt,
                right: Box::new(right),
            })
        }

        FilterValue::Gte(args) => {
            let arg = args.first()?;
            let right = meta_string_to_expr(arg);
            Some(Expr::BinOp {
                left: Box::new(col),
                op: dibs_sql::BinOp::Ge,
                right: Box::new(right),
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

/// Convert a Meta<String> argument to an Expr.
///
/// If it starts with '$', it's a parameter reference.
/// Otherwise, it's a string literal.
fn meta_string_to_expr(meta: &Meta<String>) -> Expr {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_query_file;

    fn get_first_delete(source: &str) -> Delete {
        let file = parse_query_file("<test>", source).unwrap();
        for (_, decl) in file.0.iter() {
            if let dibs_query_schema::Decl::Delete(d) = decl {
                return d.clone();
            }
        }
        panic!("No delete found in source");
    }

    #[test]
    fn test_simple_delete() {
        let source = r#"
DeleteUser @delete {
    params { id @int }
    from users
    where { id $id }
    returning { id }
}
"#;
        let delete = get_first_delete(source);
        let result = generate_delete_sql(&delete);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn test_delete_no_returning() {
        let source = r#"
DeleteOldSessions @delete {
    from sessions
    where { expired_at @lt($now) }
}
"#;
        let delete = get_first_delete(source);
        let result = generate_delete_sql(&delete);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn test_delete_multiple_conditions() {
        let source = r#"
DeleteUserPosts @delete {
    params { user_id @int, status @string }
    from posts
    where { user_id $user_id, status $status, deleted_at @null }
    returning { id, title }
}
"#;
        let delete = get_first_delete(source);
        let result = generate_delete_sql(&delete);
        insta::assert_debug_snapshot!(result);
    }
}
