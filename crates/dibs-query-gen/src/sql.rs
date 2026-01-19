//! SQL generation from query AST.

use crate::ast::*;

/// Generated SQL with parameter placeholders.
#[derive(Debug, Clone)]
pub struct GeneratedSql {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,
    /// Parameter names in order (maps to $1, $2, etc.).
    pub param_order: Vec<String>,
}

/// Generate SQL for a simple single-table query (no relations).
pub fn generate_simple_sql(query: &Query) -> GeneratedSql {
    let mut sql = String::new();
    let mut param_order = Vec::new();
    let mut param_idx = 1;

    // SELECT
    sql.push_str("SELECT ");
    let columns: Vec<_> = query
        .select
        .iter()
        .filter_map(|f| match f {
            Field::Column { name, .. } => Some(format!("\"{}\"", name)),
            _ => None, // Skip relations/aggregates for simple query
        })
        .collect();

    if columns.is_empty() {
        sql.push('*');
    } else {
        sql.push_str(&columns.join(", "));
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
            }
            _ => sql.push_str("20"), // fallback
        }
    }
    // Suppress unused warning - param_idx is used during iteration
    let _ = param_idx;

    GeneratedSql { sql, param_order }
}

fn format_filter(
    filter: &Filter,
    mut param_idx: usize,
    param_order: &mut Vec<String>,
) -> (String, usize) {
    let col = format!("\"{}\"", filter.column);

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
        (FilterOp::Eq, Expr::String(_s)) => {
            param_order.push(format!("__literal_{}", param_idx));
            let result = format!("{} = ${}", col, param_idx);
            param_idx += 1;
            result
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
        _ => format!("{} = TRUE", col), // fallback
    };

    (result, param_idx)
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
  select{ id, handle, status }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert_eq!(sql.sql, r#"SELECT "id", "handle", "status" FROM "product""#);
        assert!(sql.param_order.is_empty());
    }

    #[test]
    fn test_select_with_where() {
        let source = r#"
ActiveProducts @query{
  from product
  where{ status "published", active true }
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains("WHERE"));
        assert!(sql.sql.contains(r#""status" = $1"#));
        assert!(sql.sql.contains(r#""active" = true"#));
    }

    #[test]
    fn test_select_with_params() {
        let source = r#"
ProductByHandle @query{
  params{ handle @string }
  from product
  where{ handle $handle }
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""handle" = $1"#));
        assert_eq!(sql.param_order, vec!["handle"]);
    }

    #[test]
    fn test_select_with_order_and_limit() {
        let source = r#"
RecentProducts @query{
  from product
  order_by{ created_at desc }
  limit 20
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#"ORDER BY "created_at" DESC"#));
        assert!(sql.sql.contains("LIMIT 20"));
    }

    #[test]
    fn test_null_filter() {
        let source = r#"
ActiveProducts @query{
  from product
  where{ deleted_at @null }
  select{ id }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""deleted_at" IS NULL"#));
    }

    #[test]
    fn test_ilike_filter() {
        let source = r#"
SearchProducts @query{
  params{ q @string }
  from product
  where{ handle @ilike($q) }
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains(r#""handle" ILIKE $1"#));
        assert_eq!(sql.param_order, vec!["q"]);
    }
}
