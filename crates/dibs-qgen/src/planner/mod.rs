//! Query planner for JOIN resolution.
//!
//! This module handles:
//! - FK relationship resolution between tables
//! - JOIN clause generation
//! - Column aliasing to avoid collisions
//! - Result assembly mapping

mod types;

use dibs_db_schema::Schema;
pub use types::*;

use crate::{Query, Select};
use std::collections::HashMap;

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
    schema: &'a Schema,
}

impl<'a> QueryPlanner<'a> {
    pub fn new(schema: &'a Schema) -> Self {
        Self { schema }
    }

    /// Plan a query, resolving all relations to JOINs.
    pub fn plan(&self, query: &Query) -> Result<QueryPlan, PlanError> {
        let from_table_meta = query
            .from
            .as_ref()
            .ok_or_else(|| PlanError::TableNotFound {
                table: "<unknown>".to_string(),
            })?;
        let from_table = from_table_meta.value.clone();

        let mut plan = QueryPlan::new(from_table.clone());

        // Process top-level fields (columns and relations)
        if let Some(select) = &query.select {
            self.process_select(
                select,
                &from_table,
                &plan.from_alias.clone(),
                &[],
                &mut plan,
            )?;
        }

        Ok(plan)
    }

    /// Process select fields recursively, handling nested relations.
    fn process_select(
        &self,
        select: &Select,
        parent_table: &str,
        parent_alias: &str,
        path: &[String], // path to this relation (e.g., ["variants", "prices"])
        plan: &mut QueryPlan,
    ) -> Result<(), PlanError> {
        // Process simple columns
        for (name_meta, _field_def) in select.columns() {
            let name = &name_meta.value;
            // Build result alias: for nested relations, prefix with path
            let result_alias = if path.is_empty() {
                name.clone()
            } else {
                format!("{}_{}", path.join("_"), name)
            };

            // Build full path for column mapping
            let mut full_path = path.to_vec();
            full_path.push(name.clone());

            plan.add_column(parent_alias, name, result_alias, full_path);
        }

        // Process relations
        for (name_meta, relation) in select.relations() {
            let name = &name_meta.value;

            // Resolve the relation table name
            let relation_table = relation
                .table_name()
                .map(|s| s.to_string())
                .or_else(|| Some(name.clone()))
                .ok_or_else(|| PlanError::RelationNeedsFrom {
                    relation: name.clone(),
                })?;

            // Find FK relationship
            let relation_alias = plan.next_alias();
            let fk_resolution =
                self.resolve_fk(parent_table, &relation_table, &relation_alias, parent_alias)?;

            // Collect column names for the join (only direct columns, not nested relations)
            let join_select_columns: Vec<String> = relation
                .select
                .as_ref()
                .map(|sel| sel.columns().map(|(n, _)| n.value.clone()).collect())
                .unwrap_or_default();

            // Build join with proper ON condition
            let mut join = fk_resolution.join_clause;
            join.first = relation.is_first();
            join.select_columns = join_select_columns;

            plan.add_join(join);

            // Build path for nested fields
            let mut nested_path = path.to_vec();
            nested_path.push(name.clone());

            // Process nested columns and relations
            let mut relation_columns = HashMap::new();
            let mut nested_relations = HashMap::new();

            if let Some(nested_select) = &relation.select {
                self.process_select_nested(
                    nested_select,
                    &relation_table,
                    &relation_alias,
                    &nested_path,
                    plan,
                    &mut relation_columns,
                    &mut nested_relations,
                )?;
            }

            // For Vec relations (first=false), store parent key for grouping
            let parent_key_column = if relation.is_first() {
                None
            } else {
                Some(fk_resolution.parent_key_column)
            };

            plan.add_relation(
                name.clone(),
                RelationMapping {
                    name: name.clone(),
                    first: relation.is_first(),
                    columns: relation_columns,
                    parent_key_column,
                    table_alias: relation_alias,
                    nested_relations,
                },
            );
        }

        // Process count aggregations
        for (name_meta, _tables) in select.counts() {
            let name = &name_meta.value;
            // For now, skip count processing - would need to map table names
            let subquery = CountSubquery {
                result_alias: name.clone(),
                count_table: format!("{}_count", parent_table), // placeholder
                fk_column: format!("{}_id", parent_table),      // placeholder
                parent_alias: parent_alias.to_string(),
                parent_key: "id".to_string(), // placeholder
            };
            plan.add_count(subquery, vec![name.clone()]);
        }

        Ok(())
    }

    /// Process nested select fields (used for relations).
    fn process_select_nested(
        &self,
        select: &Select,
        parent_table: &str,
        parent_alias: &str,
        path: &[String],
        plan: &mut QueryPlan,
        column_mappings: &mut HashMap<String, String>,
        relation_mappings: &mut HashMap<String, RelationMapping>,
    ) -> Result<(), PlanError> {
        // Process simple columns in nested select
        for (name_meta, _field_def) in select.columns() {
            let col_name = &name_meta.value;
            let result_alias = format!("{}_{}", path.join("_"), col_name);

            plan.select_columns.push(SelectColumn {
                table_alias: parent_alias.to_string(),
                column: col_name.clone(),
                result_alias: result_alias.clone(),
            });
            column_mappings.insert(col_name.clone(), result_alias);
        }

        // Process relations in nested select
        for (name_meta, relation) in select.relations() {
            let name = &name_meta.value;

            let relation_table = relation
                .table_name()
                .map(|s| s.to_string())
                .or_else(|| Some(name.clone()))
                .ok_or_else(|| PlanError::RelationNeedsFrom {
                    relation: name.clone(),
                })?;

            let relation_alias = plan.next_alias();
            let fk_resolution =
                self.resolve_fk(parent_table, &relation_table, &relation_alias, parent_alias)?;

            let join_select_columns: Vec<String> = relation
                .select
                .as_ref()
                .map(|sel| sel.columns().map(|(n, _)| n.value.clone()).collect())
                .unwrap_or_default();

            let mut join = fk_resolution.join_clause;
            join.first = relation.is_first();
            join.select_columns = join_select_columns;

            plan.add_join(join);

            let mut nested_path = path.to_vec();
            nested_path.push(name.clone());

            let mut relation_columns = HashMap::new();
            let mut nested_relations = HashMap::new();

            if let Some(nested_select) = &relation.select {
                self.process_select_nested(
                    nested_select,
                    &relation_table,
                    &relation_alias,
                    &nested_path,
                    plan,
                    &mut relation_columns,
                    &mut nested_relations,
                )?;
            }

            let parent_key_column = if relation.is_first() {
                None
            } else {
                Some(fk_resolution.parent_key_column)
            };

            relation_mappings.insert(
                name.clone(),
                RelationMapping {
                    name: name.clone(),
                    first: relation.is_first(),
                    columns: relation_columns,
                    parent_key_column,
                    table_alias: relation_alias,
                    nested_relations,
                },
            );
        }

        Ok(())
    }

    /// Resolve FK relationship between two tables.
    /// Returns the FkResolution with JoinClause, direction, and parent key column.
    fn resolve_fk(
        &self,
        from_table: &str,
        to_table: &str,
        alias: &str,
        parent_alias: &str,
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
                let parent_key_column = fk.references_columns[0].clone();
                return Ok(FkResolution {
                    join_clause: JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.to_string(),
                        alias: alias.to_string(),
                        on_condition: (
                            format!("{}.{}", parent_alias, parent_key_column),
                            format!("{}.{}", alias, fk.columns[0]),
                        ),
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
                // For forward (belongs-to), parent key is the FK column in from_table
                let parent_key_column = fk.columns[0].clone();
                return Ok(FkResolution {
                    join_clause: JoinClause {
                        join_type: JoinType::Left,
                        table: to_table.to_string(),
                        alias: alias.to_string(),
                        on_condition: (
                            format!("{}.{}", parent_alias, parent_key_column),
                            format!("{}.{}", alias, fk.references_columns[0]),
                        ),
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
        _param_order: &mut Vec<String>,
        _param_idx: &mut usize,
    ) -> String {
        let mut sql = format!("\"{}\" AS \"{}\"", self.from_table, self.from_alias);

        for join in &self.joins {
            // Regular JOIN
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
