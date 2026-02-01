//! Refactored code generation with proper context structures.
//!
//! This is a sketch/experiment for refactoring the assembly functions
//! to use structured contexts instead of passing many individual parameters.

use crate::planner::PlannerSchema;
use crate::schema::*;
use crate::sql::GeneratedSql;
use codegen::Block;
use std::collections::HashMap;

/// Top-level context for generating code for a query.
pub struct QueryGenerationContext<'a> {
    /// Code generation context with schema info.
    pub codegen: &'a crate::codegen::CodegenContext<'a>,

    /// Maps column aliases to their indices in the result row.
    pub column_order: &'a HashMap<String, usize>,

    /// The query plan with JOIN and result mapping info.
    pub plan: &'a crate::planner::QueryPlan,

    /// The root table being queried.
    pub root_table: &'a str,

    /// Whether this query returns only the first result.
    pub is_first: bool,

    /// The name of the result struct being built.
    pub struct_name: &'a str,
}

/// Context for generating code for a specific relation.
pub struct RelationGenerationContext<'a> {
    /// Parent query context (has schema, column_order, plan, etc).
    pub query: &'a QueryGenerationContext<'a>,

    /// The table this relation queries from.
    pub relation_table: &'a str,

    /// Column prefix for aliases (e.g., "users" for relation named "users").
    pub col_prefix: &'a str,

    /// Whether this relation is Option (first) or Vec.
    pub is_first: bool,

    /// The name of the struct for this relation.
    pub struct_name: &'a str,
}

impl<'a> QueryGenerationContext<'a> {
    /// Get the type of a column in the root table.
    fn root_column_type(&self, col_name: &str) -> String {
        self.codegen
            .schema
            .column_type(self.root_table, col_name)
            .unwrap_or_else(|| "i64".to_string())
    }

    /// Look up a column's index by alias.
    fn column_index(&self, alias: &str) -> Option<usize> {
        self.column_order.get(alias).copied()
    }
}

impl<'a> RelationGenerationContext<'a> {
    /// Get the type of a column in this relation's table.
    fn column_type(&self, col_name: &str) -> String {
        self.query
            .codegen
            .schema
            .column_type(self.relation_table, col_name)
            .unwrap_or_else(|| "String".to_string())
    }

    /// Build the alias for a column in this relation.
    fn column_alias(&self, col_name: &str) -> String {
        format!("{}_{}", self.col_prefix, col_name)
    }

    /// Look up a column's index by its alias.
    fn column_index(&self, col_name: &str) -> Option<usize> {
        self.query.column_index(&self.column_alias(col_name))
    }
}

/// Generate code to extract a column value from a row and add it to a block.
fn generate_column_extraction(
    rel_ctx: &RelationGenerationContext,
    block: &mut Block,
    col_name: &str,
    first_alias: &str,
) {
    let alias = rel_ctx.column_alias(col_name);
    let rust_ty = rel_ctx.column_type(col_name);

    let value_expr = if rust_ty.starts_with("Option<") {
        // Already optional, just get it
        format_row_get(&alias, rel_ctx.query.column_order)
    } else if alias == first_alias {
        // This is the first/key column, we already extracted it
        format!("{}_val", first_alias)
    } else {
        // Non-optional, need to unwrap
        format!(
            "row.get::<_, Option<_>>({}).unwrap()",
            format_col_selector(&alias, rel_ctx.query.column_order)
        )
    };

    block.line(format!("{}: {},", col_name, value_expr));
}

/// Generate code to extract a column from the first relation (inside map closure).
fn generate_column_extraction_in_map(
    rel_ctx: &RelationGenerationContext,
    block: &mut Block,
    col_name: &str,
    first_alias: &str,
) {
    let alias = rel_ctx.column_alias(col_name);
    let rust_ty = rel_ctx.column_type(col_name);

    let value_expr = if rust_ty.starts_with("Option<") {
        format!("row.get(\"{}\")", alias)
    } else if alias == first_alias {
        format!("{}_val", first_alias)
    } else {
        format!("row.get::<_, Option<_>>(\"{}\").unwrap()", alias)
    };

    block.line(format!("{}: {},", col_name, value_expr));
}

/// Helper to generate row.get() call.
fn format_row_get(column_name: &str, column_order: &HashMap<String, usize>) -> String {
    if let Some(&idx) = column_order.get(column_name) {
        format!("row.get({}) /* {} */", idx, column_name)
    } else {
        format!("row.get(\"{}\")", column_name)
    }
}

/// Helper to get column selector (index or quoted string).
fn format_col_selector(column_name: &str, column_order: &HashMap<String, usize>) -> String {
    if let Some(&idx) = column_order.get(column_name) {
        format!("{} /* {} */", idx, column_name)
    } else {
        format!("\"{}\"", column_name)
    }
}

/// Generate code to add all column fields from a select to a block.
fn generate_select_columns(
    rel_ctx: &RelationGenerationContext,
    block: &mut Block,
    select: &Select,
    first_alias: &str,
) {
    for (name_meta, field_def) in &select.fields {
        // Only process simple columns (None means simple column)
        if field_def.is_none() {
            let col_name = &name_meta.value;
            generate_column_extraction(rel_ctx, block, col_name, first_alias);
        }
    }
}

/// Generate code to add all column fields from a select to a block (for map closure).
fn generate_select_columns_in_map(
    rel_ctx: &RelationGenerationContext,
    block: &mut Block,
    select: &Select,
    first_alias: &str,
) {
    for (name_meta, field_def) in &select.fields {
        if field_def.is_none() {
            let col_name = &name_meta.value;
            generate_column_extraction_in_map(rel_ctx, block, col_name, first_alias);
        }
    }
}

// ============================================================================
// REFACTORED ASSEMBLY FUNCTION
// ============================================================================

/// Generate assembly code for queries with Vec (has-many) relations.
/// REFACTORED VERSION - demonstrates context usage
pub fn generate_vec_relation_assembly_refactored(
    query_ctx: &QueryGenerationContext,
    select: &Select,
) -> String {
    let mut block = Block::new("");

    // Find the parent key column from the plan
    let parent_key_column = query_ctx
        .plan
        .result_mapping
        .relations
        .values()
        .find_map(|r| r.parent_key_column.as_ref())
        .cloned()
        .unwrap_or_else(|| "id".to_string());

    let parent_key_type = query_ctx.root_column_type(&parent_key_column);

    // Set up the grouping HashMap
    block.line("// Group rows by parent ID for has-many relations");
    block.line(format!(
        "let mut grouped: std::collections::HashMap<{}, {}> = std::collections::HashMap::new();",
        parent_key_type, query_ctx.struct_name
    ));
    block.line("");

    // For loop over rows
    let mut for_block = Block::new("for row in rows.iter()");
    for_block.line(format!(
        "let parent_id: {} = {};",
        parent_key_type,
        format_row_get(&parent_key_column, query_ctx.column_order)
    ));
    for_block.line("");

    // Build the entry struct
    let mut entry_block = Block::new(format!(
        "let entry = grouped.entry(parent_id.clone()).or_insert_with(|| {}",
        query_ctx.struct_name
    ));

    // Process fields: columns, relations, counts
    for (name_meta, field_def) in &select.fields {
        let field_name = &name_meta.value;

        match field_def {
            None => {
                // Simple column - just extract it
                let rel_ctx = RelationGenerationContext {
                    query: query_ctx,
                    relation_table: query_ctx.root_table,
                    col_prefix: "", // No prefix for root table columns
                    is_first: false,
                    struct_name: "",
                };
                generate_column_extraction(&rel_ctx, &mut entry_block, field_name, "");
            }
            Some(FieldDef::Rel(rel)) => {
                // Relation: either Option or Vec
                if rel.is_first() {
                    // Option relation - generate map closure
                    if let Some(rel_select) = &rel.select {
                        let first_col = rel_select.first_column().unwrap_or_default();
                        let first_alias = format!("{}_{}", field_name, first_col);

                        let rel_table = rel.from.as_ref().map(|m| &m.value).unwrap_or(field_name);
                        let nested_struct_name =
                            format!("Nested{}", crate::codegen::to_pascal_case(field_name));

                        let mut map_block = Block::new(format!(
                            "{}: row.get::<_, Option<_>>({}).map(|{}_val| {}",
                            field_name,
                            format_col_selector(&first_alias, query_ctx.column_order),
                            first_alias,
                            nested_struct_name
                        ));

                        let rel_ctx = RelationGenerationContext {
                            query: query_ctx,
                            relation_table: rel_table,
                            col_prefix: field_name,
                            is_first: true,
                            struct_name: &nested_struct_name,
                        };

                        generate_select_columns_in_map(
                            &rel_ctx,
                            &mut map_block,
                            rel_select,
                            &first_alias,
                        );

                        map_block.after("),");
                        entry_block.push_block(map_block);
                    }
                } else {
                    // Vec relation - initialize empty
                    entry_block.line(format!("{}: vec![],", field_name));
                }
            }
            Some(FieldDef::Count(_)) => {
                // Count aggregation
                entry_block.line(format!(
                    "{}: {},",
                    field_name,
                    format_row_get(field_name, query_ctx.column_order)
                ));
            }
        }
    }

    entry_block.after(");");
    for_block.push_block(entry_block);
    for_block.line("");

    // Append to Vec relations
    for (name_meta, field_def) in &select.fields {
        if let Some(FieldDef::Rel(rel)) = field_def {
            if !rel.is_first() {
                // This is a Vec relation
                let field_name = &name_meta.value;
                if let Some(rel_select) = &rel.select {
                    let first_col = rel_select.first_column().unwrap_or_default();
                    let first_alias = format!("{}_{}", field_name, first_col);

                    let rel_table = rel.from.as_ref().map(|m| &m.value).unwrap_or(field_name);
                    let nested_struct_name =
                        format!("Nested{}", crate::codegen::to_pascal_case(field_name));

                    for_block.line(format!(
                        "// Append {} if present (LEFT JOIN may have NULL)",
                        field_name
                    ));

                    let mut if_block = Block::new(format!(
                        "if let Some({}_val) = row.get::<_, Option<_>>(\"{}\")",
                        first_alias, first_alias
                    ));

                    let mut push_block =
                        Block::new(format!("entry.{}.push({}", field_name, nested_struct_name));

                    let rel_ctx = RelationGenerationContext {
                        query: query_ctx,
                        relation_table: rel_table,
                        col_prefix: field_name,
                        is_first: false,
                        struct_name: &nested_struct_name,
                    };

                    generate_select_columns(&rel_ctx, &mut push_block, rel_select, &first_alias);

                    push_block.after(");");
                    if_block.push_block(push_block);
                    for_block.push_block(if_block);
                }
            }
        }
    }

    block.push_block(for_block);
    block.line("");

    // Return based on whether query is first or not
    if query_ctx.is_first {
        block.line("Ok(grouped.into_values().next())");
    } else {
        block.line("Ok(grouped.into_values().collect())");
    }

    format_block_to_string(&block)
}

/// Format a Block to a String.
fn format_block_to_string(block: &Block) -> String {
    let mut output = String::new();
    let mut formatter = codegen::Formatter::new(&mut output);
    block.fmt(&mut formatter).expect("formatting failed");
    output
}
