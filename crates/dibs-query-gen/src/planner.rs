//! Query planner for JOIN resolution.
//!
//! This module handles:
//! - FK relationship resolution between tables
//! - JOIN clause generation
//! - Column aliasing to avoid collisions
//! - Result assembly mapping

use crate::ast::{Field, Query};
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
    /// Whether it's first (Option<T>) or many (Vec<T>)
    pub first: bool,
    /// Column mappings within this relation
    pub columns: HashMap<String, String>,
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
                write!(
                    f,
                    "relation '{}' requires explicit 'from' clause",
                    relation
                )
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
        let mut result_mapping = ResultMapping::default();
        let mut alias_counter = 1;

        // Add columns from the base table
        for field in &query.select {
            match field {
                Field::Column { name, .. } => {
                    let result_alias = name.clone();
                    select_columns.push(SelectColumn {
                        table_alias: from_alias.clone(),
                        column: name.clone(),
                        result_alias: result_alias.clone(),
                    });
                    result_mapping
                        .columns
                        .insert(result_alias, vec![name.clone()]);
                }
                Field::Relation {
                    name,
                    from,
                    first,
                    select,
                    ..
                } => {
                    // Resolve the relation
                    let relation_table = from.as_ref().ok_or_else(|| PlanError::RelationNeedsFrom {
                        relation: name.clone(),
                    })?;

                    // Find FK relationship
                    let (join_clause, fk_direction) =
                        self.resolve_fk(from_table, relation_table, alias_counter)?;
                    let relation_alias = join_clause.alias.clone();
                    alias_counter += 1;

                    // Determine join type based on relation cardinality
                    let join_type = if *first {
                        JoinType::Left // Option<T> - may not exist
                    } else {
                        JoinType::Left // Vec<T> - also LEFT to preserve parent rows
                    };

                    joins.push(JoinClause {
                        join_type,
                        ..join_clause
                    });

                    // Add columns from the relation
                    let mut relation_columns = HashMap::new();
                    for rel_field in select {
                        if let Field::Column {
                            name: col_name, ..
                        } = rel_field
                        {
                            let result_alias = format!("{}_{}", name, col_name);
                            select_columns.push(SelectColumn {
                                table_alias: relation_alias.clone(),
                                column: col_name.clone(),
                                result_alias: result_alias.clone(),
                            });
                            relation_columns.insert(col_name.clone(), result_alias.clone());
                            result_mapping
                                .columns
                                .insert(result_alias, vec![name.clone(), col_name.clone()]);
                        }
                    }

                    result_mapping.relations.insert(
                        name.clone(),
                        RelationMapping {
                            name: name.clone(),
                            first: *first,
                            columns: relation_columns,
                        },
                    );

                    // Suppress unused variable warning
                    let _ = fk_direction;
                }
                Field::Count { .. } => {
                    // TODO: Handle COUNT aggregates
                }
            }
        }

        Ok(QueryPlan {
            from_table: from_table.clone(),
            from_alias,
            joins,
            select_columns,
            result_mapping,
        })
    }

    /// Resolve FK relationship between two tables.
    /// Returns the JoinClause and whether FK is "forward" (from->to) or "reverse" (to->from).
    fn resolve_fk(
        &self,
        from_table: &str,
        to_table: &str,
        alias_counter: usize,
    ) -> Result<(JoinClause, FkDirection), PlanError> {
        let to_table_info = self
            .schema
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
                return Ok((
                    JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.to_string(),
                        alias: alias.clone(),
                        on_condition: (
                            format!("t0.{}", fk.references_columns[0]),
                            format!("{}.{}", alias, fk.columns[0]),
                        ),
                    },
                    FkDirection::Reverse,
                ));
            }
        }

        // Check if from_table has FK pointing to to_table (forward/belongs-to)
        let from_table_info = self.schema.tables.get(from_table).ok_or_else(|| {
            PlanError::TableNotFound {
                table: from_table.to_string(),
            }
        })?;

        for fk in &from_table_info.foreign_keys {
            if fk.references_table == to_table {
                // Found: from_table.fk_col -> to_table.ref_col
                // JOIN to_table ON from_table.fk_col = to_table.ref_col
                let alias = format!("t{}", alias_counter);
                return Ok((
                    JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.to_string(),
                        alias: alias.clone(),
                        on_condition: (
                            format!("t0.{}", fk.columns[0]),
                            format!("{}.{}", alias, fk.references_columns[0]),
                        ),
                    },
                    FkDirection::Forward,
                ));
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

impl QueryPlan {
    /// Generate SQL SELECT clause.
    pub fn select_sql(&self) -> String {
        self.select_columns
            .iter()
            .map(|col| {
                format!(
                    "\"{}\".\"{}\" AS \"{}\"",
                    col.table_alias, col.column, col.result_alias
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Generate SQL FROM clause with JOINs.
    pub fn from_sql(&self) -> String {
        let mut sql = format!("\"{}\" AS \"{}\"", self.from_table, self.from_alias);

        for join in &self.joins {
            let join_type = match join.join_type {
                JoinType::Left => "LEFT JOIN",
                JoinType::Inner => "INNER JOIN",
            };
            sql.push_str(&format!(
                " {} \"{}\" AS \"{}\" ON {} = {}",
                join_type, join.table, join.alias, join.on_condition.0, join.on_condition.1
            ));
        }

        sql
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
                columns: vec![
                    "id".to_string(),
                    "handle".to_string(),
                    "status".to_string(),
                ],
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
            span: None,
            params: vec![],
            from: "product".to_string(),
            filters: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            first: false,
            select: vec![
                Field::Column {
                    name: "id".to_string(),
                    span: None,
                },
                Field::Column {
                    name: "handle".to_string(),
                    span: None,
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
            name: "GetProductWithTranslation".to_string(),
            span: None,
            params: vec![],
            from: "product".to_string(),
            filters: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            first: false,
            select: vec![
                Field::Column {
                    name: "id".to_string(),
                    span: None,
                },
                Field::Relation {
                    name: "translation".to_string(),
                    span: None,
                    from: Some("product_translation".to_string()),
                    filters: vec![],
                    order_by: vec![],
                    first: true,
                    select: vec![
                        Field::Column {
                            name: "title".to_string(),
                            span: None,
                        },
                        Field::Column {
                            name: "description".to_string(),
                            span: None,
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
}
