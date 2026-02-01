//! Query planner for JOIN resolution.
//!
//! This module handles:
//! - FK relationship resolution between tables
//! - JOIN clause generation
//! - Column aliasing to avoid collisions
//! - Result assembly mapping

use crate::ast::{Expr, Field, Filter, FilterOp, OrderBy, Query, SortDir};
use dibs_query_schema::Span;
use std::collections::HashMap;

/// Schema information needed for query planning.
/// This mirrors dibs::Schema but avoids the dependency.
#[derive(Debug, Clone, Default)]
pub struct PlannerSchema {
    pub tables: HashMap<String, PlannerTable>,
}

/// Table information for planning.
#[derive(Debug, Clone, Default)]
pub struct PlannerTable {
    pub name: String,
    pub columns: Vec<String>,
    pub foreign_keys: Vec<PlannerForeignKey>,
}

/// FK information for planning.
#[derive(Debug, Clone)]
pub struct PlannerForeignKey {
    /// Column(s) in this table (e.g., "product_id")
    pub columns: Vec<String>,
    /// Referenced table (e.g., "product")
    pub references_table: String,
    /// Referenced column(s) (e.g., "id")
    pub references_columns: Vec<String>,
}

/// A planned query with JOINs resolved.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// The base table
    pub from_table: String,
    /// Alias for the base table
    pub from_alias: String,
    /// JOIN clauses
    pub joins: Vec<JoinClause>,
    /// Column selections with their aliases
    pub select_columns: Vec<SelectColumn>,
    /// COUNT subqueries
    pub count_subqueries: Vec<CountSubquery>,
    /// Mapping from result columns to nested struct paths
    pub result_mapping: ResultMapping,
}

/// A JOIN clause in the query plan.
#[derive(Debug, Clone)]
pub struct JoinClause {
    /// JOIN type (LEFT, INNER)
    pub join_type: JoinType,
    /// Table to join
    pub table: String,
    /// Alias for the joined table
    pub alias: String,
    /// ON condition: (left_col, right_col)
    pub on_condition: (String, String),
    /// Additional filters for this JOIN (from relation's where clause)
    pub filters: Vec<Filter>,
    /// ORDER BY for this relation
    pub order_by: Vec<OrderBy>,
    /// Whether this is a first:true relation (affects LATERAL generation)
    pub first: bool,
    /// Columns selected from this join (needed for LATERAL subquery)
    pub select_columns: Vec<String>,
}

/// JOIN type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinType {
    Left,
    Inner,
}

/// A column in the SELECT clause.
#[derive(Debug, Clone)]
pub struct SelectColumn {
    /// Table alias
    pub table_alias: String,
    /// Column name
    pub column: String,
    /// Result alias (for AS clause)
    pub result_alias: String,
}

/// A COUNT subquery in the SELECT clause.
#[derive(Debug, Clone)]
pub struct CountSubquery {
    /// Result alias (e.g., "variant_count")
    pub result_alias: String,
    /// Table to count from (e.g., "product_variant")
    pub count_table: String,
    /// FK column in the count table (e.g., "product_id")
    pub fk_column: String,
    /// Parent table alias (e.g., "t0")
    pub parent_alias: String,
    /// Parent key column (e.g., "id")
    pub parent_key: String,
}

/// Mapping of result columns to nested struct paths.
#[derive(Debug, Clone, Default)]
pub struct ResultMapping {
    /// Map from result alias to struct path (e.g., "t_title" -> ["translation", "title"])
    pub columns: HashMap<String, Vec<String>>,
    /// Nested relations and their mappings
    pub relations: HashMap<String, RelationMapping>,
}

/// Mapping for a single relation.
#[derive(Debug, Clone)]
pub struct RelationMapping {
    /// Relation name
    pub name: String,
    /// Whether it's first (`Option<T>`) or many (`Vec<T>`)
    pub first: bool,
    /// Column mappings within this relation
    pub columns: HashMap<String, String>,
    /// Parent's primary key column name (used for grouping Vec relations)
    pub parent_key_column: Option<String>,
    /// Table alias for this relation (e.g., "t1", "t2")
    pub table_alias: String,
    /// Nested relations within this relation
    pub nested_relations: HashMap<String, RelationMapping>,
}

/// Error type for query planning.
#[derive(Debug)]
pub enum PlanError {
    /// Table not found in schema
    TableNotFound { table: String },
    /// No FK relationship found between tables
    NoForeignKey { from: String, to: String },
    /// Relation requires explicit 'from' clause
    RelationNeedsFrom { relation: String },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::TableNotFound { table } => write!(f, "table not found: {}", table),
            PlanError::NoForeignKey { from, to } => {
                write!(f, "no FK relationship between {} and {}", from, to)
            }
            PlanError::RelationNeedsFrom { relation } => {
                write!(f, "relation '{}' requires explicit 'from' clause", relation)
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// Query planner that resolves JOINs.
pub struct QueryPlanner<'a> {
    schema: &'a PlannerSchema,
}

impl<'a> QueryPlanner<'a> {
    pub fn new(schema: &'a PlannerSchema) -> Self {
        Self { schema }
    }

    /// Plan a query, resolving all relations to JOINs.
    pub fn plan(&self, query: &Query) -> Result<QueryPlan, PlanError> {
        let from_table = &query.from;
        let from_alias = "t0".to_string();

        let mut joins = Vec::new();
        let mut select_columns = Vec::new();
        let mut count_subqueries = Vec::new();
        let mut result_mapping = ResultMapping::default();
        let mut alias_counter = 1;

        // Process top-level fields (columns and relations)
        self.process_fields(
            &query.select,
            from_table,
            &from_alias,
            &[], // empty path for top-level
            &mut joins,
            &mut select_columns,
            &mut count_subqueries,
            &mut result_mapping.columns,
            &mut result_mapping.relations,
            &mut alias_counter,
        )?;

        Ok(QueryPlan {
            from_table: from_table.clone(),
            from_alias,
            joins,
            select_columns,
            count_subqueries,
            result_mapping,
        })
    }

    /// Process fields recursively, handling nested relations.
    #[allow(clippy::too_many_arguments)]
    fn process_fields(
        &self,
        fields: &[Field],
        parent_table: &str,
        parent_alias: &str,
        path: &[String], // path to this relation (e.g., ["variants", "prices"])
        joins: &mut Vec<JoinClause>,
        select_columns: &mut Vec<SelectColumn>,
        count_subqueries: &mut Vec<CountSubquery>,
        column_mappings: &mut HashMap<String, Vec<String>>,
        relation_mappings: &mut HashMap<String, RelationMapping>,
        alias_counter: &mut usize,
    ) -> Result<(), PlanError> {
        for field in fields {
            match field {
                Field::Column { name, .. } => {
                    // Build result alias: for nested relations, prefix with path
                    let result_alias = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}_{}", path.join("_"), name)
                    };

                    select_columns.push(SelectColumn {
                        table_alias: parent_alias.to_string(),
                        column: name.clone(),
                        result_alias: result_alias.clone(),
                    });

                    // Build full path for column mapping
                    let mut full_path = path.to_vec();
                    full_path.push(name.clone());
                    column_mappings.insert(result_alias, full_path);
                }
                Field::Relation {
                    name,
                    from,
                    first,
                    select,
                    filters,
                    order_by,
                    ..
                } => {
                    // Resolve the relation
                    let relation_table =
                        from.as_ref().ok_or_else(|| PlanError::RelationNeedsFrom {
                            relation: name.clone(),
                        })?;

                    // Find FK relationship
                    let fk_resolution =
                        self.resolve_fk(parent_table, relation_table, *alias_counter)?;
                    let relation_alias = fk_resolution.join_clause.alias.clone();
                    *alias_counter += 1;

                    // Collect column names for the join (only direct columns, not nested relations)
                    let join_select_columns: Vec<String> = select
                        .iter()
                        .filter_map(|f| match f {
                            Field::Column { name, .. } => Some(name.clone()),
                            _ => None,
                        })
                        .collect();

                    // Build join with proper ON condition referencing parent alias
                    let mut join = fk_resolution.join_clause.clone();
                    // Fix the ON condition to use actual parent alias instead of t0
                    join.on_condition.0 = format!(
                        "{}.{}",
                        parent_alias,
                        join.on_condition.0.split('.').next_back().unwrap_or("id")
                    );
                    join.filters = filters.clone();
                    join.order_by = order_by.clone();
                    join.first = *first;
                    join.select_columns = join_select_columns;

                    joins.push(join);

                    // Build path for nested fields
                    let mut nested_path = path.to_vec();
                    nested_path.push(name.clone());

                    // Process nested columns and relations
                    let mut relation_columns = HashMap::new();
                    let mut nested_relations = HashMap::new();

                    for rel_field in select {
                        match rel_field {
                            Field::Column { name: col_name, .. } => {
                                let result_alias =
                                    format!("{}_{}", nested_path.join("_"), col_name);
                                select_columns.push(SelectColumn {
                                    table_alias: relation_alias.clone(),
                                    column: col_name.clone(),
                                    result_alias: result_alias.clone(),
                                });
                                relation_columns.insert(col_name.clone(), result_alias.clone());

                                let mut full_path = nested_path.clone();
                                full_path.push(col_name.clone());
                                column_mappings.insert(result_alias, full_path);
                            }
                            Field::Relation { .. } => {
                                // Recursively process nested relation
                                self.process_fields(
                                    std::slice::from_ref(rel_field),
                                    relation_table,
                                    &relation_alias,
                                    &nested_path,
                                    joins,
                                    select_columns,
                                    count_subqueries,
                                    column_mappings,
                                    &mut nested_relations,
                                    alias_counter,
                                )?;
                            }
                            Field::Count { .. } => {
                                // COUNT in nested relations - could add support later
                            }
                        }
                    }

                    // For Vec relations (first=false), store parent key for grouping
                    let parent_key_column = if *first {
                        None
                    } else {
                        Some(fk_resolution.parent_key_column)
                    };

                    relation_mappings.insert(
                        name.clone(),
                        RelationMapping {
                            name: name.clone(),
                            first: *first,
                            columns: relation_columns,
                            parent_key_column,
                            table_alias: relation_alias,
                            nested_relations,
                        },
                    );
                }
                Field::Count { name, table, .. } => {
                    // Resolve FK from count_table to parent table
                    if let Ok(fk_resolution) = self.resolve_fk(parent_table, table, *alias_counter)
                    {
                        let fk_column = fk_resolution
                            .join_clause
                            .on_condition
                            .1
                            .split('.')
                            .next_back()
                            .unwrap_or("id")
                            .to_string();

                        count_subqueries.push(CountSubquery {
                            result_alias: name.clone(),
                            count_table: table.clone(),
                            fk_column,
                            parent_alias: parent_alias.to_string(),
                            parent_key: fk_resolution.parent_key_column,
                        });

                        column_mappings.insert(name.clone(), vec![name.clone()]);
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve FK relationship between two tables.
    /// Returns the FkResolution with JoinClause, direction, and parent key column.
    fn resolve_fk(
        &self,
        from_table: &str,
        to_table: &str,
        alias_counter: usize,
    ) -> Result<FkResolution, PlanError> {
        let to_table_info =
            self.schema
                .tables
                .get(to_table)
                .ok_or_else(|| PlanError::TableNotFound {
                    table: to_table.to_string(),
                })?;

        // Check if to_table has FK pointing to from_table (reverse/has-many)
        for fk in &to_table_info.foreign_keys {
            if fk.references_table == from_table {
                // Found: to_table.fk_col -> from_table.ref_col
                // JOIN to_table ON from_table.ref_col = to_table.fk_col
                let alias = format!("t{}", alias_counter);
                let parent_key_column = fk.references_columns[0].clone();
                return Ok(FkResolution {
                    join_clause: JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.to_string(),
                        alias: alias.clone(),
                        on_condition: (
                            format!("t0.{}", parent_key_column),
                            format!("{}.{}", alias, fk.columns[0]),
                        ),
                        filters: vec![],
                        order_by: vec![],
                        first: false,
                        select_columns: vec![],
                    },
                    direction: FkDirection::Reverse,
                    parent_key_column,
                });
            }
        }

        // Check if from_table has FK pointing to to_table (forward/belongs-to)
        let from_table_info =
            self.schema
                .tables
                .get(from_table)
                .ok_or_else(|| PlanError::TableNotFound {
                    table: from_table.to_string(),
                })?;

        for fk in &from_table_info.foreign_keys {
            if fk.references_table == to_table {
                // Found: from_table.fk_col -> to_table.ref_col
                // JOIN to_table ON from_table.fk_col = to_table.ref_col
                let alias = format!("t{}", alias_counter);
                // For forward (belongs-to), parent key is the FK column in from_table
                let parent_key_column = fk.columns[0].clone();
                return Ok(FkResolution {
                    join_clause: JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.to_string(),
                        alias: alias.clone(),
                        on_condition: (
                            format!("t0.{}", parent_key_column),
                            format!("{}.{}", alias, fk.references_columns[0]),
                        ),
                        filters: vec![],
                        order_by: vec![],
                        first: false,
                        select_columns: vec![],
                    },
                    direction: FkDirection::Forward,
                    parent_key_column,
                });
            }
        }

        Err(PlanError::NoForeignKey {
            from: from_table.to_string(),
            to: to_table.to_string(),
        })
    }
}

/// Direction of FK relationship.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FkDirection {
    /// FK is in from_table pointing to to_table (belongs-to)
    Forward,
    /// FK is in to_table pointing to from_table (has-many)
    Reverse,
}

/// Result of FK resolution.
#[derive(Debug, Clone)]
pub struct FkResolution {
    /// The JOIN clause
    pub join_clause: JoinClause,
    /// Direction of the relationship
    pub direction: FkDirection,
    /// Parent's primary key column (used for grouping Vec relations)
    pub parent_key_column: String,
}

impl QueryPlan {
    /// Generate SQL SELECT clause.
    pub fn select_sql(&self) -> String {
        let mut parts: Vec<String> = self
            .select_columns
            .iter()
            .map(|col| {
                format!(
                    "\"{}\".\"{}\" AS \"{}\"",
                    col.table_alias, col.column, col.result_alias
                )
            })
            .collect();

        // Add COUNT subqueries
        for count in &self.count_subqueries {
            parts.push(format!(
                "(SELECT COUNT(*) FROM \"{}\" WHERE \"{}\" = \"{}\".\"{}\" ) AS \"{}\"",
                count.count_table,
                count.fk_column,
                count.parent_alias,
                count.parent_key,
                count.result_alias
            ));
        }

        parts.join(", ")
    }

    /// Generate SQL FROM clause with JOINs.
    pub fn from_sql(&self) -> String {
        self.from_sql_with_params(&mut Vec::new(), &mut 1)
    }

    /// Generate SQL FROM clause with JOINs, tracking parameter order.
    ///
    /// Returns the SQL and appends any parameter names to `param_order`.
    /// `param_idx` is updated to track the next $N placeholder.
    pub fn from_sql_with_params(
        &self,
        param_order: &mut Vec<String>,
        param_idx: &mut usize,
    ) -> String {
        let mut sql = format!("\"{}\" AS \"{}\"", self.from_table, self.from_alias);

        for join in &self.joins {
            // Use LATERAL for first:true relations with order_by
            if join.first && !join.order_by.is_empty() {
                sql.push_str(&self.format_lateral_join(join, param_order, param_idx));
            } else {
                // Regular JOIN
                let join_type = match join.join_type {
                    JoinType::Left => "LEFT JOIN",
                    JoinType::Inner => "INNER JOIN",
                };
                sql.push_str(&format!(
                    " {} \"{}\" AS \"{}\" ON {} = {}",
                    join_type, join.table, join.alias, join.on_condition.0, join.on_condition.1
                ));

                // Add relation filters to ON clause
                for filter in &join.filters {
                    let filter_sql =
                        format_join_filter(filter, &join.alias, param_order, param_idx);
                    sql.push_str(&format!(" AND {}", filter_sql));
                }
            }
        }

        sql
    }

    /// Generate a LATERAL join for first:true relations with ORDER BY.
    fn format_lateral_join(
        &self,
        join: &JoinClause,
        param_order: &mut Vec<String>,
        param_idx: &mut usize,
    ) -> String {
        // Build SELECT columns for the subquery
        let select_cols: Vec<String> = join
            .select_columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect();
        let select_clause = if select_cols.is_empty() {
            "*".to_string()
        } else {
            select_cols.join(", ")
        };

        // Build WHERE clause: FK condition + filters
        // on_condition is (parent_col, child_col) like ("t0.id", "t1.product_id")
        // For LATERAL, we reference parent directly and use child column without alias
        let fk_col = join
            .on_condition
            .1
            .split('.')
            .next_back()
            .unwrap_or(&join.on_condition.1);
        let mut where_parts = vec![format!("\"{}\" = {}", fk_col, join.on_condition.0)];

        // Add filters
        for filter in &join.filters {
            // In LATERAL subquery, we don't use table alias for columns
            let filter_sql = format_lateral_filter(filter, param_order, param_idx);
            where_parts.push(filter_sql);
        }

        // Build ORDER BY
        let order_by_parts: Vec<String> = join
            .order_by
            .iter()
            .map(|o| {
                let dir = match o.direction {
                    SortDir::Asc => "ASC",
                    SortDir::Desc => "DESC",
                };
                format!("\"{}\" {}", o.column, dir)
            })
            .collect();
        let order_by_clause = order_by_parts.join(", ");

        format!(
            " LEFT JOIN LATERAL (SELECT {} FROM \"{}\" WHERE {} ORDER BY {} LIMIT 1) AS \"{}\" ON true",
            select_clause,
            join.table,
            where_parts.join(" AND "),
            order_by_clause,
            join.alias
        )
    }
}

/// Format an Expr value as a SQL literal or placeholder.
/// For parameters, updates param_order and param_idx.
fn format_filter_value(
    value: &Expr,
    param_order: &mut Vec<String>,
    param_idx: &mut usize,
) -> String {
    match value {
        Expr::Param(name) => {
            param_order.push(name.clone());
            let placeholder = format!("${}", *param_idx);
            *param_idx += 1;
            placeholder
        }
        _ => value.to_string(),
    }
}

/// Format a filter for a LATERAL subquery (no table alias).
fn format_lateral_filter(
    filter: &Filter,
    param_order: &mut Vec<String>,
    param_idx: &mut usize,
) -> String {
    let col = format!("\"{}\"", filter.column);

    match (&filter.op, &filter.value) {
        (FilterOp::IsNull, _) | (FilterOp::Eq, Expr::Null) => format!("{} IS NULL", col),
        (FilterOp::IsNotNull, _) => format!("{} IS NOT NULL", col),
        (FilterOp::Eq, value) => format!(
            "{} = {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::ILike, value) => format!(
            "{} ILIKE {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::JsonGet, value) => format!(
            "{} -> {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::JsonGetText, value) => format!(
            "{} ->> {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Contains, value) => format!(
            "{} @> {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::KeyExists, value) => format!(
            "{} ? {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        _ => format!("{} = TRUE", col), // fallback
    }
}

/// Format a filter for a JOIN ON clause.
fn format_join_filter(
    filter: &Filter,
    table_alias: &str,
    param_order: &mut Vec<String>,
    param_idx: &mut usize,
) -> String {
    let col = format!("\"{}\".\"{}\"", table_alias, filter.column);

    match (&filter.op, &filter.value) {
        (FilterOp::IsNull, _) | (FilterOp::Eq, Expr::Null) => format!("{} IS NULL", col),
        (FilterOp::IsNotNull, _) => format!("{} IS NOT NULL", col),
        (FilterOp::Eq, value) => format!(
            "{} = {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Ne, value) => format!(
            "{} != {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Lt, value) => format!(
            "{} < {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Lte, value) => format!(
            "{} <= {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Gt, value) => format!(
            "{} > {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Gte, value) => format!(
            "{} >= {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Like, value) => format!(
            "{} LIKE {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::ILike, value) => format!(
            "{} ILIKE {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::In, value) => format!(
            "{} = ANY({})",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::JsonGet, value) => format!(
            "{} -> {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::JsonGetText, value) => format!(
            "{} ->> {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::Contains, value) => format!(
            "{} @> {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
        (FilterOp::KeyExists, value) => format!(
            "{} ? {}",
            col,
            format_filter_value(value, param_order, param_idx)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_schema() -> PlannerSchema {
        let mut schema = PlannerSchema::default();

        // product table
        schema.tables.insert(
            "product".to_string(),
            PlannerTable {
                name: "product".to_string(),
                columns: vec!["id".to_string(), "handle".to_string(), "status".to_string()],
                foreign_keys: vec![],
            },
        );

        // product_translation table with FK to product
        schema.tables.insert(
            "product_translation".to_string(),
            PlannerTable {
                name: "product_translation".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "locale".to_string(),
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

        // product_variant with FK to product
        schema.tables.insert(
            "product_variant".to_string(),
            PlannerTable {
                name: "product_variant".to_string(),
                columns: vec![
                    "id".to_string(),
                    "product_id".to_string(),
                    "sku".to_string(),
                    "title".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["product_id".to_string()],
                    references_table: "product".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        schema
    }

    #[test]
    fn test_plan_simple_query() {
        let schema = test_schema();
        let planner = QueryPlanner::new(&schema);

        let query = Query {
            name: "GetProduct".to_string(),
            doc_comment: None,
            span: Span::default(),
            params: vec![],
            from: "product".to_string(),
            filters: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            first: false,
            distinct: false,
            distinct_on: vec![],
            select: vec![
                Field::Column {
                    name: "id".to_string(),
                    span: Span::default(),
                },
                Field::Column {
                    name: "handle".to_string(),
                    span: Span::default(),
                },
            ],
            raw_sql: None,
            returns: vec![],
        };

        let plan = planner.plan(&query).unwrap();

        assert_eq!(plan.from_table, "product");
        assert!(plan.joins.is_empty());
        assert_eq!(plan.select_columns.len(), 2);
    }

    #[test]
    fn test_plan_with_relation() {
        let schema = test_schema();
        let planner = QueryPlanner::new(&schema);

        let query = Query {
            name: "GetProductWithTranslations".to_string(),
            doc_comment: None,
            span: Span::default(),
            params: vec![],
            from: "product".to_string(),
            filters: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            first: false,
            distinct: false,
            distinct_on: vec![],
            select: vec![
                Field::Column {
                    name: "id".to_string(),
                    span: Span::default(),
                },
                Field::Relation {
                    name: "translation".to_string(),
                    span: Span::default(),
                    from: Some("product_translation".to_string()),
                    filters: vec![],
                    order_by: vec![],
                    first: true,
                    select: vec![
                        Field::Column {
                            name: "title".to_string(),
                            span: Span::default(),
                        },
                        Field::Column {
                            name: "description".to_string(),
                            span: Span::default(),
                        },
                    ],
                },
            ],
            raw_sql: None,
            returns: vec![],
        };

        let plan = planner.plan(&query).unwrap();

        assert_eq!(plan.from_table, "product");
        assert_eq!(plan.joins.len(), 1);
        assert_eq!(plan.joins[0].table, "product_translation");
        assert_eq!(plan.joins[0].join_type, JoinType::Left);

        // Check SELECT columns
        assert_eq!(plan.select_columns.len(), 3); // id, translation_title, translation_description

        // Generate SQL
        let select = plan.select_sql();
        assert!(select.contains("\"t0\".\"id\""));
        assert!(select.contains("\"t1\".\"title\""));

        let from = plan.from_sql();
        assert!(from.contains("LEFT JOIN \"product_translation\""));
        assert!(from.contains("ON t0.id = t1.product_id"));
    }

    #[test]
    fn test_plan_with_nested_relations() {
        let mut schema = test_schema();

        // Add variant_price table with FK to product_variant
        schema.tables.insert(
            "variant_price".to_string(),
            PlannerTable {
                name: "variant_price".to_string(),
                columns: vec![
                    "id".to_string(),
                    "variant_id".to_string(),
                    "currency_code".to_string(),
                    "amount".to_string(),
                ],
                foreign_keys: vec![PlannerForeignKey {
                    columns: vec!["variant_id".to_string()],
                    references_table: "product_variant".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
            },
        );

        let planner = QueryPlanner::new(&schema);

        let query = Query {
            name: "GetProductWithVariantsAndPrices".to_string(),
            doc_comment: None,
            span: Span::default(),
            params: vec![],
            from: "product".to_string(),
            filters: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            first: false,
            distinct: false,
            distinct_on: vec![],
            select: vec![
                Field::Column {
                    name: "id".to_string(),
                    span: Span::default(),
                },
                Field::Relation {
                    name: "variants".to_string(),
                    span: Span::default(),
                    from: Some("product_variant".to_string()),
                    filters: vec![],
                    order_by: vec![],
                    first: false,
                    select: vec![
                        Field::Column {
                            name: "id".to_string(),
                            span: Span::default(),
                        },
                        Field::Column {
                            name: "sku".to_string(),
                            span: Span::default(),
                        },
                        Field::Relation {
                            name: "prices".to_string(),
                            span: Span::default(),
                            from: Some("variant_price".to_string()),
                            filters: vec![],
                            order_by: vec![],
                            first: false,
                            select: vec![
                                Field::Column {
                                    name: "currency_code".to_string(),
                                    span: Span::default(),
                                },
                                Field::Column {
                                    name: "amount".to_string(),
                                    span: Span::default(),
                                },
                            ],
                        },
                    ],
                },
            ],
            raw_sql: None,
            returns: vec![],
        };

        let plan = planner.plan(&query).unwrap();

        // Should have 2 JOINs: product_variant and variant_price
        assert_eq!(plan.joins.len(), 2, "Should have 2 JOINs");
        assert_eq!(plan.joins[0].table, "product_variant");
        assert_eq!(plan.joins[1].table, "variant_price");

        // Check aliases
        assert_eq!(plan.joins[0].alias, "t1");
        assert_eq!(plan.joins[1].alias, "t2");

        // Check SELECT columns: id, variants_id, variants_sku, variants_prices_currency_code, variants_prices_amount
        assert_eq!(plan.select_columns.len(), 5, "Should have 5 SELECT columns");

        // Check column aliases
        let aliases: Vec<_> = plan
            .select_columns
            .iter()
            .map(|c| c.result_alias.as_str())
            .collect();
        assert!(aliases.contains(&"id"));
        assert!(aliases.contains(&"variants_id"));
        assert!(aliases.contains(&"variants_sku"));
        assert!(aliases.contains(&"variants_prices_currency_code"));
        assert!(aliases.contains(&"variants_prices_amount"));

        // Check nested relation mapping
        let variants_rel = plan.result_mapping.relations.get("variants").unwrap();
        assert!(!variants_rel.first);
        assert!(variants_rel.nested_relations.contains_key("prices"));

        let prices_rel = variants_rel.nested_relations.get("prices").unwrap();
        assert!(!prices_rel.first);
        assert_eq!(prices_rel.table_alias, "t2");

        // Check generated SQL
        let from = plan.from_sql();
        assert!(
            from.contains("LEFT JOIN \"product_variant\" AS \"t1\" ON t0.id = t1.product_id"),
            "from: {}",
            from
        );
        assert!(
            from.contains("LEFT JOIN \"variant_price\" AS \"t2\" ON t1.id = t2.variant_id"),
            "from: {}",
            from
        );
    }
}
