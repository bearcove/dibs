//! SQL generation from query AST.

use crate::ast::*;
use crate::planner::{PlannerSchema, QueryPlan, QueryPlanner};

/// Generated SQL with parameter placeholders.
#[derive(Debug, Clone)]
pub struct GeneratedSql {
    /// The SQL string with $1, $2, etc. placeholders.
    pub sql: String,
    /// Parameter names in order (maps to $1, $2, etc.).
    pub param_order: Vec<String>,
    /// Query plan (if JOINs are involved).
    pub plan: Option<QueryPlan>,
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
    }
}

/// Generate SQL for a query with JOINs using the planner.
pub fn generate_sql_with_joins(
    query: &Query,
    schema: &PlannerSchema,
) -> Result<GeneratedSql, crate::planner::PlanError> {
    // Check if query needs the planner (has relations or COUNT fields)
    let needs_planner = query
        .select
        .iter()
        .any(|f| matches!(f, Field::Relation { .. }) || matches!(f, Field::Count { .. }));

    if !needs_planner {
        // Fall back to simple SQL generation
        return Ok(generate_simple_sql(query));
    }

    // Plan the query
    let planner = QueryPlanner::new(schema);
    let plan = planner.plan(query)?;

    let mut sql = String::new();
    let mut param_order = Vec::new();
    let mut param_idx = 1;

    // SELECT with aliased columns
    sql.push_str("SELECT ");
    sql.push_str(&plan.select_sql());

    // FROM with JOINs
    sql.push_str(" FROM ");
    sql.push_str(&plan.from_sql());

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
  where{ status "published", active true }
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
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

    #[test]
    fn test_pagination_literals() {
        let source = r#"
PaginatedProducts @query{
  from product
  order_by{ created_at desc }
  limit 20
  offset 40
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
        let sql = generate_simple_sql(&file.queries[0]);

        assert!(sql.sql.contains("LIMIT 20"));
        assert!(sql.sql.contains("OFFSET 40"));
        assert!(sql.param_order.is_empty());
    }

    #[test]
    fn test_pagination_params() {
        let source = r#"
PaginatedProducts @query{
  params{ page_size @int, page_offset @int }
  from product
  order_by{ created_at desc }
  limit $page_size
  offset $page_offset
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
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
  params{ handle @string }
  from product
  where{ handle $handle }
  select{
    id
    handle
    translation @rel{
      from product_translation
      first true
      select{ title, description }
    }
  }
}
"#;
        let file = parse_query_file(source).unwrap();

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

        let sql = generate_sql_with_joins(&file.queries[0], &schema).unwrap();

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
}
