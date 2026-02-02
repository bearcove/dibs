//! Query planner for JOIN resolution.
//!
//! This module handles:
//! - FK relationship resolution between tables
//! - JOIN clause generation
//! - Column aliasing to avoid collisions
//! - Result assembly mapping

use crate::{Query, Select};

// A planned query with JOINs resolved.
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
        let from_alias = "t0".to_string();

        let mut joins = Vec::new();
        let mut select_columns = Vec::new();
        let mut count_subqueries = Vec::new();
        let mut result_mapping = ResultMapping::default();
        let mut alias_counter = 1;

        // Process top-level fields (columns and relations)
        if let Some(select) = &query.select {
            self.process_select(
                select,
                &from_table,
                &from_alias,
                &[], // empty path for top-level
                &mut joins,
                &mut select_columns,
                &mut count_subqueries,
                &mut result_mapping.columns,
                &mut result_mapping.relations,
                &mut alias_counter,
            )?;
        }

        Ok(QueryPlan {
            from_table,
            from_alias,
            joins,
            select_columns,
            count_subqueries,
            result_mapping,
        })
    }

    /// Process select fields recursively, handling nested relations.
    #[allow(clippy::too_many_arguments)]
    fn process_select(
        &self,
        select: &Select,
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
        // Process simple columns
        for (name_meta, _field_def) in select.columns() {
            let name = &name_meta.value;
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
            let fk_resolution = self.resolve_fk(parent_table, &relation_table, *alias_counter)?;
            let relation_alias = fk_resolution.join_clause.alias.clone();
            *alias_counter += 1;

            // Collect column names for the join (only direct columns, not nested relations)
            let join_select_columns: Vec<String> = relation
                .select
                .as_ref()
                .map(|sel| sel.columns().map(|(n, _)| n.value.clone()).collect())
                .unwrap_or_default();

            // Build join with proper ON condition referencing parent alias
            let mut join = fk_resolution.join_clause.clone();
            // Fix the ON condition to use actual parent alias instead of t0
            join.on_condition.0 = format!(
                "{}.{}",
                parent_alias,
                join.on_condition.0.split('.').next_back().unwrap_or("id")
            );
            join.first = relation.is_first();
            join.select_columns = join_select_columns;

            joins.push(join);

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
                    joins,
                    select_columns,
                    count_subqueries,
                    &mut relation_columns,
                    &mut nested_relations,
                    alias_counter,
                )?;
            }

            // For Vec relations (first=false), store parent key for grouping
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

        // Process count aggregations
        for (name_meta, _tables) in select.counts() {
            let name = &name_meta.value;
            // For now, skip count processing - would need to map table names
            count_subqueries.push(CountSubquery {
                result_alias: name.clone(),
                count_table: format!("{}_count", parent_table), // placeholder
                fk_column: format!("{}_id", parent_table),      // placeholder
                parent_alias: parent_alias.to_string(),
                parent_key: "id".to_string(), // placeholder
            });
            column_mappings.insert(name.clone(), vec![name.clone()]);
        }

        Ok(())
    }

    /// Process nested select fields (used for relations).
    #[allow(clippy::too_many_arguments)]
    fn process_select_nested(
        &self,
        select: &Select,
        parent_table: &str,
        parent_alias: &str,
        path: &[String],
        joins: &mut Vec<JoinClause>,
        select_columns: &mut Vec<SelectColumn>,
        count_subqueries: &mut Vec<CountSubquery>,
        column_mappings: &mut HashMap<String, String>,
        relation_mappings: &mut HashMap<String, RelationMapping>,
        alias_counter: &mut usize,
    ) -> Result<(), PlanError> {
        // Process simple columns in nested select
        for (name_meta, _field_def) in select.columns() {
            let col_name = &name_meta.value;
            let result_alias = format!("{}_{}", path.join("_"), col_name);
            select_columns.push(SelectColumn {
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

            let fk_resolution = self.resolve_fk(parent_table, &relation_table, *alias_counter)?;
            let relation_alias = fk_resolution.join_clause.alias.clone();
            *alias_counter += 1;

            let join_select_columns: Vec<String> = relation
                .select
                .as_ref()
                .map(|sel| sel.columns().map(|(n, _)| n.value.clone()).collect())
                .unwrap_or_default();

            let mut join = fk_resolution.join_clause.clone();
            join.on_condition.0 = format!(
                "{}.{}",
                parent_alias,
                join.on_condition.0.split('.').next_back().unwrap_or("id")
            );
            join.first = relation.is_first();
            join.select_columns = join_select_columns;

            joins.push(join);

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
                    joins,
                    select_columns,
                    count_subqueries,
                    &mut relation_columns,
                    &mut nested_relations,
                    alias_counter,
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
