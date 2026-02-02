//! Rust code generation from query schema types using the `codegen` crate.

use codegen::{Block, Function, Scope, Struct};
use dibs_query_schema::{
    Decl, Delete, FieldDef, Insert, InsertMany, Meta, Params, QueryFile, Returning, Select,
    SelectFields, Update, Upsert, UpsertMany,
};

use std::collections::HashMap;
use std::sync::Arc;

use crate::sqlgen::GeneratedSql;
use crate::{QError, QErrorKind, QSource, QueryPlan};

// TODO: do not use `&str` everywhere here, use `&ColumnNameRef` etc.

// ============================================================================
// Code Generation Contexts
// ============================================================================

/// Top-level context for generating code for a query.
struct QueryGenerationContext<'a> {
    /// Code generation context with schema info.
    codegen: &'a CodegenContext<'a>,

    /// Maps column aliases to their indices in the result row.
    column_order: &'a HashMap<String, usize>,

    /// The query plan with JOIN and result mapping info.
    plan: &'a QueryPlan,

    /// The root table being queried.
    root_table: &'a str,

    /// Whether this query returns only the first result.
    is_first: bool,

    /// The name of the result struct being built.
    struct_name: &'a str,

    /// The original source - used for error reporting.
    source: Option<Arc<QSource>>,
}

impl<'a> QueryGenerationContext<'a> {
    /// Get the type of a column in the root table.
    fn root_column_type(&self, col_name: &str) -> Option<String> {
        self.codegen.schema.column_type(self.root_table, col_name)
    }

    /// Get the type of a column with error reporting.
    /// Takes the Meta wrapper so we have span for error reporting.
    fn root_column_type_with_span(&self, col_name_meta: &Meta<String>) -> Result<String, QError> {
        self.codegen
            .schema
            .column_type(self.root_table, &col_name_meta.value)
            .ok_or_else(|| QError {
                span: col_name_meta.span,
                source: self
                    .source
                    .clone()
                    .expect("source required for error reporting"),
                kind: QErrorKind::ColumnNotFound {
                    table: self.root_table.to_string(),
                    column: col_name_meta.value.clone(),
                },
            })
    }

    /// Look up a column's index by alias.
    fn column_index(&self, alias: &str) -> Option<usize> {
        self.column_order.get(alias).copied()
    }
}

/// Context for generating code for a specific relation.
struct RelationGenerationContext<'a> {
    /// Parent query context (has schema, column_order, plan, etc).
    query: &'a QueryGenerationContext<'a>,
    /// The table this relation queries from.
    relation_table: &'a str,
    /// Column prefix for aliases (e.g., "users" for relation named "users").
    col_prefix: &'a str,
    /// Whether this relation is Option (first) or Vec.
    is_first: bool,
    /// The name of the struct for this relation.
    struct_name: &'a str,
}

impl<'a> RelationGenerationContext<'a> {
    /// Get the type of a column in this relation's table.
    /// Panics if column type cannot be determined - this is a schema mismatch error.
    fn column_type(&self, col_name: &str) -> String {
        self.query
            .codegen
            .schema
            .column_type(self.relation_table, col_name)
            .unwrap_or_else(|| {
                panic!(
                    "schema mismatch: column '{}' not found in relation table '{}'",
                    col_name, self.relation_table
                )
            })
    }

    /// Build the alias for a column in this relation.
    fn column_alias(&self, col_name: &str) -> String {
        format!("{}_{}", self.col_prefix, col_name)
    }

    /// Look up a column's index by its alias.
    fn column_index(&self, col_name: &str) -> Option<usize> {
        self.query.column_index(&self.column_alias(col_name))
    }

    /// Generate code to extract a column value from a row and add it to a block.
    fn generate_column_extraction(&self, block: &mut Block, col_name: &str, first_alias: &str) {
        let alias = self.column_alias(col_name);
        let rust_ty = self.column_type(col_name);

        let value_expr = if rust_ty.starts_with("Option<") {
            // Already optional, just get it
            format_row_get(&alias, self.query.column_order)
        } else if alias == first_alias {
            // This is the first/key column, we already extracted it
            format!("{first_alias}_val")
        } else {
            // Non-optional, need to unwrap
            format!(
                "row.get::<_, Option<_>>({}).unwrap()",
                format_col_selector(&alias, self.query.column_order)
            )
        };

        block.line(format!("{col_name}: {value_expr},"));
    }

    /// Generate code to extract a column from the first relation (inside map closure).
    fn generate_column_extraction_in_map(
        &self,
        block: &mut Block,
        col_name: &str,
        first_alias: &str,
    ) {
        let alias = self.column_alias(col_name);
        let rust_ty = self.column_type(col_name);

        let value_expr = if rust_ty.starts_with("Option<") {
            format!("row.get(\"{alias}\")")
        } else if alias == first_alias {
            format!("{first_alias}_val")
        } else {
            format!("row.get::<_, Option<_>>(\"{alias}\").unwrap()")
        };

        block.line(format!("{col_name}: {value_expr},"));
    }

    /// Generate code to add all column fields from a select to a block.
    fn generate_select_columns(
        &self,
        block: &mut Block,
        select_fields: &SelectFields,
        first_alias: &str,
    ) {
        for (name_meta, field_def) in &select_fields.fields {
            // Only process simple columns (None means simple column)
            if field_def.is_none() {
                let col_name = name_meta.value.as_str();
                self.generate_column_extraction(block, col_name, first_alias);
            }
        }
    }

    /// Generate code to add all column fields from a select to a block (for map closure).
    fn generate_select_columns_in_map(
        &self,
        block: &mut Block,
        select_fields: &SelectFields,
        first_alias: &str,
    ) {
        for (name_meta, field_def) in &select_fields.fields {
            if field_def.is_none() {
                let col_name = name_meta.value.as_str();
                self.generate_column_extraction_in_map(block, col_name, first_alias);
            }
        }
    }
}

// ============================================================================
// Standalone Helper Functions for Row Access
// ============================================================================

/// Helper to generate row.get() call using column index if available, with a comment.
fn format_row_get(column_name: &str, column_order: &HashMap<String, usize>) -> String {
    if let Some(&idx) = column_order.get(column_name) {
        format!("row.get({}) /* {} */", idx, column_name)
    } else {
        format!("row.get(\"{}\")", column_name)
    }
}

/// Helper to get just the column selector (index or quoted string) for use in row.get::<T>(...)
fn format_col_selector(column_name: &str, column_order: &HashMap<String, usize>) -> String {
    if let Some(&idx) = column_order.get(column_name) {
        format!("{} /* {} */", idx, column_name)
    } else {
        format!("\"{}\"", column_name)
    }
}

/// Generated Rust code for a query file.
#[derive(Debug, Clone)]
pub struct GeneratedCode {
    /// Full Rust source code.
    pub code: String,
}

/// Schema information for code generation.
///
/// This provides type information for columns, allowing the codegen
/// to emit correctly-typed result structs.
#[derive(Debug, Clone, Default)]
pub struct SchemaInfo {
    /// Map of table name -> column info.
    pub tables: HashMap<String, TableInfo>,
}

/// Information about a single table.
#[derive(Debug, Clone, Default)]
pub struct TableInfo {
    /// Map of column name -> Rust type string.
    pub columns: HashMap<String, ColumnInfo>,
}

/// Information about a single column.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// Rust type name (e.g., "i64", "String", "bool").
    pub rust_type: String,
    /// Whether the column is nullable (`Option<T>`).
    pub nullable: bool,
}

impl SchemaInfo {
    /// Create a new empty schema.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up the Rust type for a column.
    pub fn column_type(&self, table: &str, column: &str) -> Option<String> {
        let table_info = self.tables.get(table)?;
        let col_info = table_info.columns.get(column)?;
        if col_info.nullable {
            Some(format!("Option<{}>", col_info.rust_type))
        } else {
            Some(col_info.rust_type.clone())
        }
    }
}

/// Context for code generation.
struct CodegenContext<'a> {
    schema: &'a SchemaInfo,
    planner_schema: &'a dibs_db_schema::Schema,
    scope: Scope,
}

/// Generate Rust code for a query file.
pub fn generate_rust_code(file: &QueryFile) -> GeneratedCode {
    generate_rust_code_with_schema(file, &SchemaInfo::default())
}

/// Generate Rust code for a query file with schema information.
pub fn generate_rust_code_with_schema(file: &QueryFile, schema: &SchemaInfo) -> GeneratedCode {
    generate_rust_code_with_planner(file, schema, None)
}

/// Generate Rust code for a query file with full schema and planner info.
///
/// When `planner_schema` is provided, queries with relations will generate
/// JOINs and proper result assembly code.
pub fn generate_rust_code_with_planner(
    file: &QueryFile,
    schema: &SchemaInfo,
    planner_schema: &dibs_db_schema::Schema,
) -> GeneratedCode {
    let mut scope = Scope::new();

    // Add file header as raw code
    scope.raw("// Generated by dibs-qgen. Do not edit.");
    scope.raw("");

    // Imports
    scope.import("dibs_runtime::prelude", "*");
    scope.import("dibs_runtime", "tokio_postgres");

    let ctx = CodegenContext {
        schema,
        planner_schema,
        scope: Scope::new(),
    };

    // Iterate through declarations and generate code for each type
    for (name_meta, decl) in &file.0 {
        match decl {
            Decl::Select(select) => {
                generate_select_code(&ctx, name_meta, select, &mut scope);
            }
            Decl::Insert(insert) => {
                generate_insert_code(&ctx, name_meta, insert, &mut scope);
            }
            Decl::InsertMany(insert_many) => {
                generate_insert_many_code(&ctx, name_meta, insert_many, &mut scope);
            }
            Decl::Upsert(upsert) => {
                generate_upsert_code(&ctx, name_meta, upsert, &mut scope);
            }
            Decl::UpsertMany(upsert_many) => {
                generate_upsert_many_code(&ctx, name_meta, upsert_many, &mut scope);
            }
            Decl::Update(update) => {
                generate_update_code(&ctx, name_meta, update, &mut scope);
            }
            Decl::Delete(delete) => {
                generate_delete_code(&ctx, name_meta, delete, &mut scope);
            }
        }
    }

    GeneratedCode {
        code: scope.to_string(),
    }
}

fn generate_select_code(
    ctx: &CodegenContext,
    name_meta: &Meta<String>,
    select: &Select,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let struct_name = format!("{}Result", name);

    // Generate result struct(s)
    if let Some(from) = &select.from {
        if select.fields.is_some() {
            generate_result_struct(ctx, select, name_meta, &struct_name, from, scope);
        }
    }

    // Generate query function
    generate_select_function(ctx, name_meta, select, &struct_name, scope);
}

fn generate_result_struct(
    ctx: &CodegenContext,
    select: &Select,
    name_meta: &Meta<String>,
    struct_name: &str,
    table: &Meta<dibs_sql::TableName>,
    scope: &mut Scope,
) {
    let mut st = Struct::new(struct_name);
    st.vis("pub");
    st.derive("Debug");
    st.derive("Clone");
    st.derive("Facet");
    st.attr("facet(crate = dibs_runtime::facet)");

    // Regular query - use select fields
    let parent_prefix = &name_meta.value;
    let table_name = table.value.as_str();

    if let Some(select_fields) = &select.fields {
        for (field_name_meta, field_def) in &select_fields.fields {
            let field_name = field_name_meta.value.as_str();
            match field_def {
                None => {
                    // Simple column
                    let rust_ty = ctx
                        .schema
                        .column_type(table_name, field_name)
                        .unwrap_or_else(|| "String".to_string());
                    st.field(format!("pub {}", field_name), &rust_ty);
                }
                Some(FieldDef::Rel(rel)) => {
                    let nested_name = format!("{}{}", parent_prefix, to_pascal_case(field_name));
                    let ty = if rel.first.is_some() {
                        format!("Option<{}>", nested_name)
                    } else {
                        format!("Vec<{}>", nested_name)
                    };
                    st.field(format!("pub {}", field_name), &ty);
                }
                Some(FieldDef::Count(_)) => {
                    st.field(format!("pub {}", field_name), "i64");
                }
            }
        }
    }

    scope.push_struct(st);

    // Generate nested structs for relations (recursively)
    if let Some(select_fields) = &select.fields {
        generate_nested_structs(ctx, parent_prefix, select_fields, scope);
    }
}

/// Recursively generate structs for nested relations.
///
/// `parent_prefix` is used to namespace the struct names to avoid collisions
/// when multiple queries have relations with the same field name.
fn generate_nested_structs(
    ctx: &CodegenContext,
    parent_prefix: &str,
    select_fields: &SelectFields,
    scope: &mut Scope,
) {
    for (field_name_meta, field_def) in &select_fields.fields {
        if let Some(FieldDef::Rel(rel)) = field_def {
            let field_name = field_name_meta.value.as_str();
            let nested_name = format!("{}{}", parent_prefix, to_pascal_case(field_name));
            let rel_table = rel.table_name().unwrap_or(field_name);

            let mut nested_st = Struct::new(&nested_name);
            nested_st.vis("pub");
            nested_st.derive("Debug");
            nested_st.derive("Clone");
            nested_st.derive("Facet");
            nested_st.attr("facet(crate = dibs_runtime::facet)");

            if let Some(rel_fields) = &rel.fields {
                for (rel_field_name_meta, rel_field_def) in &rel_fields.fields {
                    let rel_field_name = rel_field_name_meta.value.as_str();
                    match rel_field_def {
                        None => {
                            // Simple column
                            let rust_ty = ctx
                                .schema
                                .column_type(rel_table, rel_field_name)
                                .unwrap_or_else(|| "String".to_string());
                            nested_st.field(format!("pub {}", rel_field_name), &rust_ty);
                        }
                        Some(FieldDef::Rel(nested_rel)) => {
                            // Nested relation field - namespace with current struct name
                            let nested_rel_name =
                                format!("{}{}", nested_name, to_pascal_case(rel_field_name));
                            let ty = if nested_rel.first.is_some() {
                                format!("Option<{}>", nested_rel_name)
                            } else {
                                format!("Vec<{}>", nested_rel_name)
                            };
                            nested_st.field(format!("pub {}", rel_field_name), &ty);
                        }
                        Some(FieldDef::Count(_)) => {
                            nested_st.field(format!("pub {}", rel_field_name), "i64");
                        }
                    }
                }
            }

            scope.push_struct(nested_st);

            // Recursively generate structs for nested relations
            if let Some(rel_fields) = &rel.fields {
                generate_nested_structs(ctx, &nested_name, rel_fields, scope);
            }
        }
    }
}

fn generate_select_function(
    ctx: &CodegenContext,
    name_meta: &Meta<String>,
    query: &Select,
    struct_name: &str,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);

    let return_ty = if query.first.is_some() {
        format!("Result<Option<{}>, QueryError>", struct_name)
    } else {
        format!("Result<Vec<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");
    // Allow clone_on_copy since we generate .clone() calls on parent IDs that might be Copy types
    func.attr("allow(clippy::clone_on_copy)");

    if let Some(params) = &query.params {
        for (param_name_meta, param_type) in &params.params {
            let param_name = &param_name_meta.value;
            let rust_ty = param_type_to_rust(param_type);
            func.arg(param_name, format!("&{}", rust_ty));
        }
    }

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    // Generate function body
    if let Some(raw_sql_meta) = &query.sql {
        let body = generate_raw_query_body(query, &raw_sql_meta.value);
        func.line(block_to_string(&body));
    } else {
        let body = generate_query_body(ctx, query, struct_name);
        func.line(body);
    };

    scope.push_fn(func);
}

// Note: has_vec_relations and has_nested_vec_relations are now methods on Select/SelectFields

/// Generate query body for all queries (with or without JOINs).
fn generate_query_body(ctx: &CodegenContext, query: &Select, struct_name: &str) -> String {
    let Some(schema) = ctx.planner_schema else {
        // No schema available - generate simple from_row() body
        return "// Warning: No schema available for query planning\nrows.iter().map(|row| Ok(from_row(row)?)).collect()".to_string();
    };

    let generated = match crate::sqlgen::generate_select_sql(query, schema) {
        Ok(g) => g,
        Err(e) => {
            // Fallback: generate a simple error message
            return format!(
                "// Warning: SELECT planning failed: {}\nrows.iter().map(|row| Ok(from_row(row)?)).collect()",
                e
            );
        }
    };

    let mut block = Block::new("");

    // SQL constant
    block.line(format!("const SQL: &str = r#\"{}\"#;", generated.sql));
    block.line("");

    // Build params array - filter out literal placeholders
    let params: Vec<_> = generated
        .param_order
        .iter()
        .filter(|p| !p.as_str().starts_with("__literal_"))
        .collect();

    if params.is_empty() {
        block.line("let rows = client.query(SQL, &[]).await?;");
    } else {
        let params_str = params
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        block.line(format!(
            "let rows = client.query(SQL, &[{}]).await?;",
            params_str
        ));
    }

    let plan = &generated.plan;

    // Convert column_order from HashMap<ColumnName, usize> to HashMap<String, usize>
    let column_order: HashMap<String, usize> = generated
        .column_order
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();

    // If no relations, use from_row() directly
    let Some(select_fields) = &query.fields else {
        // Simple query - use from_row() for direct deserialization
        if query.first.is_some() {
            let mut match_block = Block::new("match rows.into_iter().next()");
            match_block.line("Some(row) => Ok(Some(from_row(&row)?)),");
            match_block.line("None => Ok(None),");
            block.push_block(match_block);
        } else {
            block.line("rows.iter().map(|row| Ok(from_row(row)?)).collect()");
        }
        return block_to_string(&block);
    };

    block.line("");

    // Check if we have Vec relations - if so, use HashMap-based grouping
    let root_table = query
        .from
        .as_ref()
        .map(|m| m.value.as_str())
        .unwrap_or("unknown");
    let is_first = query.is_first();

    if query.has_vec_relations() {
        if query.has_nested_vec_relations() {
            block.line(generate_nested_vec_relation_assembly(
                ctx,
                select_fields,
                struct_name,
                plan,
                &column_order,
                root_table,
                is_first,
            ));
        } else {
            block.line(generate_vec_relation_assembly(
                ctx,
                select_fields,
                struct_name,
                plan,
                &column_order,
                root_table,
                is_first,
            ));
        }
    } else {
        block.line(generate_option_relation_assembly(
            ctx,
            select_fields,
            struct_name,
            &column_order,
            root_table,
        ));
    }

    block_to_string(&block)
}

/// Generate a simple query body using from_row() for direct deserialization.
/// Used when there are no relations that require manual assembly.
fn generate_from_row_body(query: &Select, generated: &GeneratedSql) -> Block {
    let mut block = Block::new("");

    // SQL constant
    block.line(format!("const SQL: &str = r#\"{}\"#;", generated.sql));
    block.line("");

    // Query execution
    let params: Vec<_> = generated
        .param_order
        .iter()
        .filter(|p| !p.starts_with("__literal_"))
        .collect();

    if params.is_empty() {
        block.line("let rows = client.query(SQL, &[]).await?;");
    } else {
        let params_str = params
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        block.line(format!(
            "let rows = client.query(SQL, &[{}]).await?;",
            params_str
        ));
    }

    // Result processing
    if query.first.is_some() {
        let mut match_block = Block::new("match rows.into_iter().next()");
        match_block.line("Some(row) => Ok(Some(from_row(&row)?)),");
        match_block.line("None => Ok(None),");
        block.push_block(match_block);
    } else {
        block.line("rows.iter().map(|row| Ok(from_row(row)?)).collect()");
    }

    block
}

/// Generate assembly code for queries with Vec (has-many) relations.
fn generate_vec_relation_assembly(
    codegen_ctx: &CodegenContext,
    select_fields: &SelectFields,
    struct_name: &str,
    plan: &QueryPlan,
    column_order: &HashMap<String, usize>,
    root_table: &str,
    is_first: bool,
) -> String {
    let mut block = Block::new("");

    // Find the parent key column from the first Vec relation
    let parent_key_column = plan
        .result_mapping
        .relations
        .values()
        .find_map(|r| r.parent_key_column.as_ref())
        .cloned()
        .unwrap_or_else(|| "id".into());

    let parent_key_type = codegen_ctx
        .schema
        .column_type(root_table, parent_key_column.as_str())
        .unwrap_or_else(|| "i64".to_string());

    block.line("// Group rows by parent ID for has-many relations");
    block.line(format!(
        "let mut grouped: std::collections::HashMap<{parent_key_type}, {struct_name}> = std::collections::HashMap::new();",
    ));
    block.line("");

    // For loop over rows
    let mut for_block = Block::new("for row in rows.iter()");
    for_block.line(format!(
        "let parent_id: {parent_key_type} = {};",
        format_row_get(parent_key_column.as_str(), column_order)
    ));
    for_block.line("");

    // Entry insertion with or_insert_with
    let mut entry_block = Block::new(format!(
        "let entry = grouped.entry(parent_id.clone()).or_insert_with(|| {struct_name}"
    ));

    // Iterate over fields in the select clause
    for (field_name_meta, field_def) in &select_fields.fields {
        let field_name = field_name_meta.value.as_str();
        match field_def {
            None => {
                // Simple column
                entry_block.line(format!(
                    "{field_name}: {},",
                    format_row_get(field_name, column_order)
                ));
            }
            Some(FieldDef::Rel(rel)) => {
                if rel.is_first() {
                    // Option relation - will be populated below with map
                    entry_block.line(format!("{field_name}: None,"));
                } else {
                    // Vec relation - initialize empty
                    entry_block.line(format!("{field_name}: vec![],"));
                }
            }
            Some(FieldDef::Count(_)) => {
                entry_block.line(format!(
                    "{field_name}: {},",
                    format_row_get(field_name, column_order)
                ));
            }
        }
    }
    entry_block.after(");");
    for_block.push_block(entry_block);
    for_block.line("");

    // Now populate Option relations and Vec relations
    for (field_name_meta, field_def) in &select_fields.fields {
        let field_name = field_name_meta.value.as_str();

        if let Some(FieldDef::Rel(rel)) = field_def {
            let rel_table = rel.table_name().unwrap_or(field_name);
            if let Some(rel_select) = &rel.fields {
                let first_col = rel_select
                    .first_column()
                    .map(|c| c.as_str())
                    .unwrap_or("id");
                let first_alias = format!("{field_name}_{first_col}");

                if rel.is_first() {
                    // Option relation - populate with map
                    let nested_struct_name = format!(
                        "{}Nested{}",
                        to_pascal_case(&struct_name.replace("Result", "")),
                        to_pascal_case(field_name)
                    );

                    let mut map_block = Block::new(format!(
                        "entry.{field_name} = row.get::<_, Option<_>>({}).map(|{first_alias}_val| {nested_struct_name}",
                        format_col_selector(&first_alias, column_order)
                    ));

                    let rel_ctx = RelationGenerationContext {
                        query: &QueryGenerationContext {
                            codegen: codegen_ctx,
                            column_order,
                            plan,
                            root_table,
                            is_first,
                            struct_name: &nested_struct_name,
                            source: None,
                        },
                        relation_table: rel_table,
                        col_prefix: field_name,
                        is_first: true,
                        struct_name: &nested_struct_name,
                    };

                    rel_ctx.generate_select_columns_in_map(
                        &mut map_block,
                        rel_select,
                        &first_alias,
                    );

                    map_block.after(");");
                    for_block.push_block(map_block);
                } else {
                    // Vec relation - append if present
                    for_block.line(format!(
                        "// Append {field_name} if present (LEFT JOIN may have NULL)"
                    ));

                    let nested_struct_name = format!(
                        "{}Nested{}",
                        to_pascal_case(&struct_name.replace("Result", "")),
                        to_pascal_case(field_name)
                    );

                    let mut if_block = Block::new(format!(
                        "if let Some({first_alias}_val) = row.get::<_, Option<_>>({})",
                        format_col_selector(&first_alias, column_order)
                    ));

                    let mut push_block =
                        Block::new(format!("entry.{field_name}.push({nested_struct_name}"));

                    let rel_ctx = RelationGenerationContext {
                        query: &QueryGenerationContext {
                            codegen: codegen_ctx,
                            column_order,
                            plan,
                            root_table,
                            is_first,
                            struct_name: &nested_struct_name,
                            source: None,
                        },
                        relation_table: rel_table,
                        col_prefix: field_name,
                        is_first: false,
                        struct_name: &nested_struct_name,
                    };

                    rel_ctx.generate_select_columns(&mut push_block, rel_select, &first_alias);

                    push_block.after(");");
                    if_block.push_block(push_block);
                    for_block.push_block(if_block);
                }
            }
        }
    }

    block.push_block(for_block);
    block.line("");

    if is_first {
        block.line("Ok(grouped.into_values().next())");
    } else {
        block.line("Ok(grouped.into_values().collect())");
    }

    block_to_string(&block)
}

/// Generate assembly code for queries with nested Vec relations.
///
/// This handles cases like `product → variants (Vec) → prices (Vec)` where
/// we need multi-level grouping with nested HashMaps.
fn generate_nested_vec_relation_assembly(
    codegen_ctx: &CodegenContext,
    select_fields: &SelectFields,
    struct_name: &str,
    plan: &QueryPlan,
    column_order: &HashMap<String, usize>,
    root_table: &str,
    is_first: bool,
) -> String {
    let mut block = Block::new("");

    // Find the parent key column from the first Vec relation
    let parent_key_column = plan
        .result_mapping
        .relations
        .values()
        .find_map(|r| r.parent_key_column.as_ref())
        .cloned()
        .unwrap_or_else(|| "id".into());

    let parent_key_type = codegen_ctx
        .schema
        .column_type(root_table, parent_key_column.as_str())
        .unwrap_or_else(|| "i64".to_string());

    // We'll track:
    // 1. Parent-level grouping (product_id → ProductResult)
    // 2. For each nested Vec, track intermediate IDs for deduplication

    block.line("// Group rows by parent ID for has-many relations with nested children");
    block.line(format!(
        "let mut grouped: std::collections::HashMap<{parent_key_type}, {struct_name}> = std::collections::HashMap::new();",
    ));

    // For each Vec relation with nested Vec children, we need to track seen IDs
    // to avoid duplicates when the inner relation produces multiple rows
    for (field_name_meta, field_def) in &select_fields.fields {
        if let Some(FieldDef::Rel(rel)) = field_def {
            let field_name = field_name_meta.value.as_str();
            if !rel.is_first() {
                // This is a Vec relation
                if let Some(rel_select) = &rel.fields {
                    let rel_table = rel.table_name().unwrap_or(field_name);
                    // Get the ID column of this relation for deduplication
                    if let Some(id_col) = rel_select.id_column() {
                        let id_type = codegen_ctx
                            .schema
                            .column_type(rel_table, id_col.as_str())
                            .unwrap_or_else(|| "i64".to_string());
                        block.line(format!(
                            "let mut seen_{field_name}: std::collections::HashSet<({parent_key_type}, {id_type})> = std::collections::HashSet::new();",
                        ));

                        // For nested Vec relations, track their seen IDs too
                        for (inner_field_name_meta, inner_field_def) in &rel_select.fields {
                            if let Some(FieldDef::Rel(inner_rel)) = inner_field_def {
                                let inner_field_name = inner_field_name_meta.value.as_str();
                                if !inner_rel.is_first() {
                                    // This is a nested Vec relation
                                    if let Some(inner_rel_select) = &inner_rel.fields {
                                        let inner_table =
                                            inner_rel.table_name().unwrap_or(inner_field_name);
                                        if let Some(inner_id_col) = inner_rel_select.id_column() {
                                            let inner_id_type = codegen_ctx
                                                .schema
                                                .column_type(inner_table, inner_id_col.as_str())
                                                .unwrap_or_else(|| "i64".to_string());
                                            block.line(format!(
                                                "let mut seen_{field_name}_{inner_field_name}: std::collections::HashSet<({parent_key_type}, {id_type}, {inner_id_type})> = std::collections::HashSet::new();",
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    block.line("");

    // For loop over rows
    let mut for_block = Block::new("for row in rows.iter()");
    for_block.line(format!(
        "let parent_id: {parent_key_type} = {};",
        format_row_get(parent_key_column.as_str(), column_order)
    ));
    for_block.line("");

    // Initialize the parent entry
    let mut entry_block = Block::new(format!(
        "let entry = grouped.entry(parent_id.clone()).or_insert_with(|| {struct_name}"
    ));

    for (field_name_meta, field_def) in &select_fields.fields {
        let field_name = field_name_meta.value.as_str();
        match field_def {
            None => {
                // Simple column
                entry_block.line(format!(
                    "{field_name}: {},",
                    format_row_get(field_name, column_order)
                ));
            }
            Some(FieldDef::Rel(rel)) => {
                if rel.is_first() {
                    entry_block.line(format!("{field_name}: None, // populated below"));
                } else {
                    entry_block.line(format!("{field_name}: vec![],"));
                }
            }
            Some(FieldDef::Count(_)) => {
                entry_block.line(format!(
                    "{field_name}: {},",
                    format_row_get(field_name, column_order)
                ));
            }
        }
    }
    entry_block.after(");");
    for_block.push_block(entry_block);
    for_block.line("");

    // Handle each relation (Option and Vec)
    for (field_name_meta, field_def) in &select_fields.fields {
        let field_name = field_name_meta.value.as_str();

        if let Some(FieldDef::Rel(rel)) = field_def {
            let rel_table = rel.table_name().unwrap_or(field_name);
            if let Some(rel_select) = &rel.fields {
                let first_col = rel_select
                    .first_column()
                    .map(|c| c.as_str())
                    .unwrap_or("id");
                let first_alias = format!("{field_name}_{first_col}");

                if rel.is_first() {
                    // Option relation - populate if entry.field is None
                    let nested_struct_name = format!(
                        "{}Nested{}",
                        to_pascal_case(&struct_name.replace("Result", "")),
                        to_pascal_case(field_name)
                    );

                    for_block.line(format!("// Populate {field_name} (Option) if not yet set"));

                    let mut if_none_block = Block::new(format!("if entry.{field_name}.is_none()"));
                    let mut map_block = Block::new(format!(
                        "entry.{field_name} = row.get::<_, Option<_>>({}).map(|{first_alias}_val| {nested_struct_name}",
                        format_col_selector(&first_alias, column_order)
                    ));

                    let rel_ctx = RelationGenerationContext {
                        query: &QueryGenerationContext {
                            codegen: codegen_ctx,
                            column_order,
                            plan,
                            root_table,
                            is_first,
                            struct_name: &nested_struct_name,
                            source: None,
                        },
                        relation_table: rel_table,
                        col_prefix: field_name,
                        is_first: true,
                        struct_name: &nested_struct_name,
                    };

                    rel_ctx.generate_select_columns_in_map(
                        &mut map_block,
                        rel_select,
                        &first_alias,
                    );

                    map_block.after(");");
                    if_none_block.push_block(map_block);
                    for_block.push_block(if_none_block);
                    for_block.line("");
                } else {
                    // Vec relation - check if it has nested Vec children
                    let has_nested_vec = rel_select.has_vec_relations();

                    let nested_struct_name = format!(
                        "{}Nested{}",
                        to_pascal_case(&struct_name.replace("Result", "")),
                        to_pascal_case(field_name)
                    );

                    if has_nested_vec {
                        // Vec relation with nested Vec children - needs deduplication
                        generate_nested_vec_with_dedup(
                            codegen_ctx,
                            &mut for_block,
                            field_name,
                            rel_table,
                            &nested_struct_name,
                            rel_select,
                            column_order,
                        );
                    } else {
                        // Simple Vec relation without nested Vec children
                        for_block.line(format!(
                            "// Append {field_name} if present (LEFT JOIN may have NULL)"
                        ));

                        let mut if_some_block = Block::new(format!(
                            "if let Some({first_alias}_val) = row.get::<_, Option<_>>({})",
                            format_col_selector(&first_alias, column_order)
                        ));

                        let mut push_block =
                            Block::new(format!("entry.{field_name}.push({nested_struct_name}"));

                        let rel_ctx = RelationGenerationContext {
                            query: &QueryGenerationContext {
                                codegen: codegen_ctx,
                                column_order,
                                plan,
                                root_table,
                                is_first,
                                struct_name: &nested_struct_name,
                                source: None,
                            },
                            relation_table: rel_table,
                            col_prefix: field_name,
                            is_first: false,
                            struct_name: &nested_struct_name,
                        };

                        rel_ctx.generate_select_columns(&mut push_block, rel_select, &first_alias);

                        push_block.after(");");
                        if_some_block.push_block(push_block);
                        for_block.push_block(if_some_block);
                        for_block.line("");
                    }
                }
            }
        }
    }

    block.push_block(for_block);
    block.line("");

    if is_first {
        block.line("Ok(grouped.into_values().next())");
    } else {
        block.line("Ok(grouped.into_values().collect())");
    }

    block_to_string(&block)
}

/// Helper to generate nested Vec relation with deduplication logic.
fn generate_nested_vec_with_dedup(
    codegen_ctx: &CodegenContext,
    for_block: &mut Block,
    field_name: &str,
    rel_table: &str,
    nested_struct_name: &str,
    select_fields: &SelectFields,
    column_order: &HashMap<String, usize>,
) {
    let first_col = select_fields
        .first_column()
        .map(|c| c.as_str())
        .unwrap_or("id");
    let id_col = select_fields
        .id_column()
        .map(|c| c.as_str())
        .unwrap_or(first_col);
    let id_alias = format!("{field_name}_{id_col}");
    let id_type = codegen_ctx
        .schema
        .column_type(rel_table, id_col)
        .unwrap_or_else(|| "i64".to_string());

    for_block.line(format!(
        "// Append {field_name} if present (with deduplication for nested relations)"
    ));

    let mut if_some_block = Block::new(format!(
        "if let Some({id_alias}_val) = row.get::<_, Option<{id_type}>>({}) ",
        format_col_selector(&id_alias, column_order)
    ));

    if_some_block.line(format!(
        "let key = (parent_id.clone(), {id_alias}_val.clone());"
    ));

    let mut if_insert_block = Block::new(format!("if seen_{field_name}.insert(key)"));
    if_insert_block.line(format!("// First time seeing this {field_name}"));

    // Build the nested struct with all fields
    let mut push_block = Block::new(format!("entry.{field_name}.push({nested_struct_name}"));

    for (inner_field_name_meta, inner_field_def) in &select_fields.fields {
        let inner_field_name = inner_field_name_meta.value.as_str();

        match inner_field_def {
            None => {
                // Simple column
                let alias = format!("{field_name}_{inner_field_name}");
                push_block.line(format!(
                    "{inner_field_name}: {},",
                    format_row_get(&alias, column_order)
                ));
            }
            Some(FieldDef::Rel(inner_rel)) => {
                if inner_rel.is_first() {
                    push_block.line(format!("{inner_field_name}: None, // populated below"));
                } else {
                    push_block.line(format!("{inner_field_name}: vec![],"));
                }
            }
            Some(FieldDef::Count(_)) => {
                let alias = format!("{field_name}_{inner_field_name}");
                push_block.line(format!(
                    "{inner_field_name}: {},",
                    format_row_get(&alias, column_order)
                ));
            }
        }
    }

    push_block.after(");");
    if_insert_block.push_block(push_block);
    if_some_block.push_block(if_insert_block);
    if_some_block.line("");

    // Now handle nested relations - find the parent we just created or already exists
    if_some_block.line(format!(
        "// Find the {field_name} entry to append nested children"
    ));

    let mut if_find_block = Block::new(format!(
        "if let Some({field_name}_entry) = entry.{field_name}.iter_mut().find(|e| e.{id_col} == {id_alias}_val)"
    ));

    // Handle nested relations
    for (inner_field_name_meta, inner_field_def) in &select_fields.fields {
        if let Some(FieldDef::Rel(inner_rel)) = inner_field_def {
            let inner_field_name = inner_field_name_meta.value.as_str();
            if let Some(inner_select) = &inner_rel.fields {
                let inner_table = inner_rel.table_name().unwrap_or(inner_field_name);
                let inner_nested_name = format!(
                    "{}Nested{}",
                    nested_struct_name,
                    to_pascal_case(inner_field_name)
                );
                let inner_first_col = inner_select
                    .first_column()
                    .map(|c| c.as_str())
                    .unwrap_or("id");
                let inner_first_alias =
                    format!("{field_name}_{inner_field_name}_{inner_first_col}");

                if inner_rel.is_first() {
                    // Option nested relation
                    let mut if_inner_none = Block::new(format!(
                        "if {field_name}_entry.{inner_field_name}.is_none()"
                    ));

                    let mut inner_map_block = Block::new(format!(
                        "{field_name}_entry.{inner_field_name} = row.get::<_, Option<_>>({}).map(|_val| {inner_nested_name}",
                        format_col_selector(&inner_first_alias, column_order)
                    ));

                    // Extract columns for the inner relation
                    for (inner_col_meta, inner_field_def) in &inner_select.fields {
                        if inner_field_def.is_none() {
                            let inner_col_name = inner_col_meta.value.as_str();
                            let alias = format!("{field_name}_{inner_field_name}_{inner_col_name}");
                            let rust_ty = codegen_ctx
                                .schema
                                .column_type(inner_table, inner_col_name)
                                .unwrap_or_else(|| "String".to_string());

                            let value_expr = if rust_ty.starts_with("Option<") {
                                format!("row.get(\"{alias}\")")
                            } else if alias == inner_first_alias {
                                "_val".to_string()
                            } else {
                                format!("row.get::<_, Option<_>>(\"{alias}\").unwrap()")
                            };

                            inner_map_block.line(format!("{inner_col_name}: {value_expr},"));
                        }
                    }

                    inner_map_block.after(");");
                    if_inner_none.push_block(inner_map_block);
                    if_find_block.push_block(if_inner_none);
                } else {
                    // Vec nested relation - need deduplication
                    let inner_id_col = inner_select
                        .id_column()
                        .map(|c| c.as_str())
                        .unwrap_or(inner_first_col);
                    let inner_id_alias = format!("{field_name}_{inner_field_name}_{inner_id_col}");
                    let inner_id_type = codegen_ctx
                        .schema
                        .column_type(inner_table, inner_id_col)
                        .unwrap_or_else(|| "i64".to_string());

                    let mut if_inner_some = Block::new(format!(
                        "if let Some({inner_id_alias}_val) = row.get::<_, Option<{inner_id_type}>>({}) ",
                        format_col_selector(&inner_id_alias, column_order)
                    ));

                    if_inner_some.line(format!(
                        "let inner_key = (parent_id.clone(), {id_alias}_val.clone(), {inner_id_alias}_val.clone());"
                    ));

                    let mut if_inner_insert = Block::new(format!(
                        "if seen_{field_name}_{inner_field_name}.insert(inner_key)"
                    ));

                    let mut inner_push_block = Block::new(format!(
                        "{field_name}_entry.{inner_field_name}.push({inner_nested_name}"
                    ));

                    // Extract columns for the inner relation (Vec case)
                    for (inner_col_meta, inner_field_def) in &inner_select.fields {
                        if inner_field_def.is_none() {
                            let inner_col_name = inner_col_meta.value.as_str();
                            let alias = format!("{field_name}_{inner_field_name}_{inner_col_name}");
                            let rust_ty = codegen_ctx
                                .schema
                                .column_type(inner_table, inner_col_name)
                                .unwrap_or_else(|| "String".to_string());

                            let value_expr = if rust_ty.starts_with("Option<") {
                                format_row_get(&alias, column_order)
                            } else {
                                format!(
                                    "row.get::<_, Option<_>>({}).unwrap()",
                                    format_col_selector(&alias, column_order)
                                )
                            };

                            inner_push_block.line(format!("{inner_col_name}: {value_expr},"));
                        }
                    }

                    inner_push_block.after(");");
                    if_inner_insert.push_block(inner_push_block);
                    if_inner_some.push_block(if_inner_insert);
                    if_find_block.push_block(if_inner_some);
                }
            }
        }
    }

    if_some_block.push_block(if_find_block);
    for_block.push_block(if_some_block);
    for_block.line("");
}

/// Get the "id" column from a list of fields, if present.
fn get_id_column(select: &Select) -> Option<String> {
    select
        .fields
        .as_ref()
        .and_then(|sf| sf.id_column())
        .map(|c| c.to_string())
}

/// Generate assembly code for queries with only Option relations.
fn generate_option_relation_assembly(
    codegen_ctx: &CodegenContext,
    select_fields: &SelectFields,
    struct_name: &str,
    column_order: &HashMap<String, usize>,
    root_table: &str,
) -> String {
    let mut block = Block::new("");

    block.line("// Assemble flat rows into nested structs");

    let mut map_block =
        Block::new("let results: Result<Vec<_>, QueryError> = rows.iter().map(|row| {");

    // Build the result struct initialization
    let mut result_block = Block::new(format!("Ok({struct_name}"));

    // Iterate over all fields and extract/assemble them
    for (field_name_meta, field_def) in &select_fields.fields {
        let field_name = field_name_meta.value.as_str();

        match field_def {
            None => {
                // Simple column
                result_block.line(format!(
                    "{field_name}: {},",
                    format_row_get(field_name, column_order)
                ));
            }
            Some(FieldDef::Rel(rel)) => {
                if rel.is_first() {
                    // Option relation - map it inline
                    if let Some(rel_select) = &rel.fields {
                        let rel_table = rel.table_name().unwrap_or(field_name);
                        let first_col = rel_select
                            .first_column()
                            .map(|c| c.as_str())
                            .unwrap_or("id");
                        let first_alias = format!("{field_name}_{first_col}");
                        let nested_struct_name = format!(
                            "{}Nested{}",
                            to_pascal_case(&struct_name.replace("Result", "")),
                            to_pascal_case(field_name)
                        );

                        let mut map_block_inner = Block::new(format!(
                            "{field_name}: row.get::<_, Option<_>>({}).map(|{first_alias}_val| {nested_struct_name}",
                            format_col_selector(&first_alias, column_order)
                        ));

                        // Extract columns for this relation
                        for (inner_col_meta, inner_field_def) in &rel_select.fields {
                            if inner_field_def.is_none() {
                                let inner_col_name = inner_col_meta.value.as_str();
                                let alias = format!("{field_name}_{inner_col_name}");
                                let rust_ty = codegen_ctx
                                    .schema
                                    .column_type(rel_table, inner_col_name)
                                    .unwrap_or_else(|| "String".to_string());

                                let value_expr = if rust_ty.starts_with("Option<") {
                                    format!("row.get(\"{alias}\")")
                                } else if alias == first_alias {
                                    format!("{first_alias}_val")
                                } else {
                                    format!("row.get::<_, Option<_>>(\"{alias}\").unwrap()")
                                };

                                map_block_inner.line(format!("{inner_col_name}: {value_expr},"));
                            }
                        }

                        map_block_inner.after("),");
                        result_block.push_block(map_block_inner);
                    }
                } else {
                    // Vec relation - should not happen in option_relation_assembly
                    // But if it does, initialize as empty vec
                    result_block.line(format!("{field_name}: vec![],"));
                }
            }
            Some(FieldDef::Count(_)) => {
                result_block.line(format!(
                    "{field_name}: {},",
                    format_row_get(field_name, column_order)
                ));
            }
        }
    }

    result_block.after(")");
    map_block.push_block(result_block);
    map_block.after("}).collect();");
    block.push_block(map_block);
    block.line("");

    // Check if first.is_some() to determine return type
    block.line("if results.is_empty() || results.as_ref().ok().map_or(false, |r| r.is_empty()) {");
    block.line("Ok(None)");
    block.line("} else {");
    block.line("Ok(results?.into_iter().next())");
    block.line("}");

    block_to_string(&block)
}

fn generate_raw_query_body(query: &Select, raw_sql: &str) -> Block {
    let cleaned: String = raw_sql
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n");

    let mut block = Block::new("");

    // SQL constant
    block.line(format!("const SQL: &str = r#\"{}\"#;", cleaned.trim()));
    block.line("");

    // Query execution
    if let Some(params) = &query.params {
        let param_names: Vec<&str> = params.iter().map(|(meta, _)| meta.value.as_str()).collect();
        if !param_names.is_empty() {
            let params_str = param_names.join(", ");
            block.line(format!(
                "let rows = client.query(SQL, &[{}]).await?;",
                params_str
            ));
        } else {
            block.line("let rows = client.query(SQL, &[]).await?;");
        }
    } else {
        block.line("let rows = client.query(SQL, &[]).await?;");
    }

    // Result processing
    if query.first.is_some() {
        let mut match_block = Block::new("match rows.into_iter().next()");
        match_block.line("Some(row) => Ok(Some(from_row(&row)?)),");
        match_block.line("None => Ok(None),");
        block.push_block(match_block);
    } else {
        block.line("rows.iter().map(|row| Ok(from_row(row)?)).collect()");
    }

    block
}

fn get_first_column(select: &Select) -> String {
    select
        .fields
        .as_ref()
        .and_then(|sf| sf.first_column())
        .map(|c| c.to_string())
        .unwrap_or_default()
}

fn param_type_to_rust(ty: &dibs_query_schema::ParamType) -> String {
    use dibs_query_schema::ParamType;
    match ty {
        ParamType::String => "String".to_string(),
        ParamType::Int => "i64".to_string(),
        ParamType::Bool => "bool".to_string(),
        ParamType::Uuid => "Uuid".to_string(),
        ParamType::Decimal => "Decimal".to_string(),
        ParamType::Timestamp => "Timestamp".to_string(),
        ParamType::Bytes => "Vec<u8>".to_string(),
        ParamType::Optional(inner_vec) => {
            if let Some(inner) = inner_vec.first() {
                format!("Option<{}>", param_type_to_rust(inner))
            } else {
                "Option<String>".to_string()
            }
        }
    }
}

/// Helper to format a Block to a String.
fn block_to_string(block: &Block) -> String {
    let mut output = String::new();
    let mut formatter = codegen::Formatter::new(&mut output);
    block.fmt(&mut formatter).expect("formatting failed");
    output
}

fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();

    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }

    result
}

// ============================================================================
// Mutation code generation
// ============================================================================

fn generate_insert_code(
    _ctx: &CodegenContext,
    name_meta: &Meta<String>,
    insert: &Insert,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);
    let generated = crate::sqlgen::generate_insert_sql(insert);

    // Generate result struct if RETURNING is used
    let has_returning = insert.returning.is_some();
    let return_ty = if !has_returning {
        "Result<u64, QueryError>".to_string()
    } else {
        let struct_name = format!("{}Result", name);
        if let Some(returning) = &insert.returning {
            generate_mutation_result_struct(
                _ctx,
                &struct_name,
                insert.into.value.as_str(),
                returning,
                scope,
            );
        }
        format!("Result<Option<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");

    if let Some(params) = &insert.params {
        for (param_name_meta, param_type) in &params.params {
            let param_name = param_name_meta.value.as_str();
            let rust_ty = param_type_to_rust(param_type);
            func.arg(param_name, format!("&{}", rust_ty));
        }
    }

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    let body = generate_mutation_body(&generated.sql, &generated.params, !has_returning);
    func.line(block_to_string(&body));

    scope.push_fn(func);
}

fn generate_upsert_code(
    _ctx: &CodegenContext,
    name_meta: &Meta<String>,
    upsert: &Upsert,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);
    let generated = crate::sqlgen::generate_upsert_sql(upsert);

    let has_returning = upsert.returning.is_some();
    let return_ty = if !has_returning {
        "Result<u64, QueryError>".to_string()
    } else {
        let struct_name = format!("{}Result", name);
        if let Some(returning) = &upsert.returning {
            generate_mutation_result_struct(
                _ctx,
                &struct_name,
                upsert.into.value.as_str(),
                returning,
                scope,
            );
        }
        format!("Result<Option<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");

    if let Some(params) = &upsert.params {
        for (param_name_meta, param_type) in &params.params {
            let param_name = param_name_meta.value.as_str();
            let rust_ty = param_type_to_rust(param_type);
            func.arg(param_name, format!("&{}", rust_ty));
        }
    }

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    let body = generate_mutation_body(&generated.sql, &generated.params, !has_returning);
    func.line(block_to_string(&body));

    scope.push_fn(func);
}

fn generate_insert_many_code(
    ctx: &CodegenContext,
    name_meta: &Meta<String>,
    insert: &InsertMany,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);
    let generated = crate::sqlgen::generate_insert_many_sql(insert);

    // Generate params struct
    let params_struct_name = format!("{}Params", name);
    if let Some(params) = &insert.params {
        generate_bulk_params_struct(
            ctx,
            &params_struct_name,
            insert.into.value.as_str(),
            params,
            scope,
        );
    }

    // Generate result struct if RETURNING is used
    let has_returning = insert.returning.is_some();
    let return_ty = if !has_returning {
        "Result<u64, QueryError>".to_string()
    } else {
        let struct_name = format!("{}Result", name);
        if let Some(returning) = &insert.returning {
            generate_mutation_result_struct(
                ctx,
                &struct_name,
                insert.into.value.as_str(),
                returning,
                scope,
            );
        }
        format!("Result<Vec<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");
    func.arg("items", format!("&[{}]", params_struct_name));

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    let body = generate_bulk_mutation_body(&generated.sql, insert.params.as_ref(), !has_returning);
    func.line(block_to_string(&body));

    scope.push_fn(func);
}

fn generate_upsert_many_code(
    ctx: &CodegenContext,
    name_meta: &Meta<String>,
    upsert: &UpsertMany,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);
    let generated = crate::sqlgen::generate_upsert_many_sql(upsert);

    // Generate params struct
    let params_struct_name = format!("{}Params", name);
    if let Some(params) = &upsert.params {
        generate_bulk_params_struct(
            ctx,
            &params_struct_name,
            upsert.into.value.as_str(),
            params,
            scope,
        );
    }

    // Generate result struct if RETURNING is used
    let has_returning = upsert.returning.is_some();
    let return_ty = if !has_returning {
        "Result<u64, QueryError>".to_string()
    } else {
        let struct_name = format!("{}Result", name);
        if let Some(returning) = &upsert.returning {
            generate_mutation_result_struct(
                ctx,
                &struct_name,
                upsert.into.value.as_str(),
                returning,
                scope,
            );
        }
        format!("Result<Vec<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");
    func.arg("items", format!("&[{}]", params_struct_name));

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    let body = generate_bulk_mutation_body(&generated.sql, upsert.params.as_ref(), !has_returning);
    func.line(block_to_string(&body));

    scope.push_fn(func);
}

/// Generate a params struct for bulk operations.
fn generate_bulk_params_struct(
    ctx: &CodegenContext,
    struct_name: &str,
    table: &str,
    params: &Params,
    scope: &mut Scope,
) {
    let mut st = Struct::new(struct_name);
    st.vis("pub");
    st.derive("Debug");
    st.derive("Clone");

    for (param_name_meta, param_type) in &params.params {
        let param_name = param_name_meta.value.as_str();
        let rust_ty = ctx
            .schema
            .column_type(table, param_name)
            .unwrap_or_else(|| param_type_to_rust(param_type));
        st.field(format!("pub {}", param_name), &rust_ty);
    }

    scope.push_struct(st);
}

/// Generate body for bulk mutation (INSERT MANY / UPSERT MANY).
fn generate_bulk_mutation_body(sql: &str, params: Option<&Params>, execute_only: bool) -> Block {
    let mut block = Block::new("");

    // SQL constant
    block.line(format!("const SQL: &str = r#\"{}\"#;", sql));
    block.line("");

    // Convert slice of structs to parallel arrays
    if let Some(params) = params {
        block.line("// Convert items to parallel arrays for UNNEST");
        for (param_name_meta, param_type) in &params.params {
            let param_name = param_name_meta.value.as_str();
            let rust_ty = param_type_to_rust(param_type);
            block.line(format!(
                "let {}_arr: Vec<{}> = items.iter().map(|i| i.{}.clone()).collect();",
                param_name, rust_ty, param_name
            ));
        }
        block.line("");

        // Build the params reference array
        let param_refs: Vec<String> = params
            .params
            .keys()
            .map(|p| format!("&{}_arr", p.value))
            .collect();

        if execute_only {
            // No RETURNING - use execute
            block.line(format!(
                "let affected = client.execute(SQL, &[{}]).await?;",
                param_refs.join(", ")
            ));
            block.line("Ok(affected)");
        } else {
            // Has RETURNING - use query
            block.line(format!(
                "let rows = client.query(SQL, &[{}]).await?;",
                param_refs.join(", ")
            ));
            block.line("rows.iter().map(|row| Ok(from_row(row)?)).collect()");
        }
    }

    block
}

fn generate_update_code(
    _ctx: &CodegenContext,
    name_meta: &Meta<String>,
    update: &Update,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);
    let generated = crate::sqlgen::generate_update_sql(update);

    let has_returning = update.returning.is_some();
    let return_ty = if !has_returning {
        "Result<u64, QueryError>".to_string()
    } else {
        let struct_name = format!("{}Result", name);
        if let Some(returning) = &update.returning {
            generate_mutation_result_struct(
                _ctx,
                &struct_name,
                update.table.value.as_str(),
                returning,
                scope,
            );
        }
        format!("Result<Option<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");

    if let Some(params) = &update.params {
        for (param_name_meta, param_type) in &params.params {
            let param_name = param_name_meta.value.as_str();
            let rust_ty = param_type_to_rust(param_type);
            func.arg(param_name, format!("&{}", rust_ty));
        }
    }

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    let body = generate_mutation_body(&generated.sql, &generated.params, !has_returning);
    func.line(block_to_string(&body));

    scope.push_fn(func);
}

fn generate_delete_code(
    _ctx: &CodegenContext,
    name_meta: &Meta<String>,
    delete: &Delete,
    scope: &mut Scope,
) {
    let name = &name_meta.value;
    let fn_name = to_snake_case(name);
    let generated = crate::sqlgen::generate_delete_sql(delete);

    let has_returning = delete.returning.is_some();
    let return_ty = if !has_returning {
        "Result<u64, QueryError>".to_string()
    } else {
        let struct_name = format!("{}Result", name);
        if let Some(returning) = &delete.returning {
            generate_mutation_result_struct(
                _ctx,
                &struct_name,
                delete.from.value.as_str(),
                returning,
                scope,
            );
        }
        format!("Result<Option<{}>, QueryError>", struct_name)
    };

    let mut func = Function::new(&fn_name);
    if let Some(doc) = &name_meta.doc {
        let doc_str = doc.join("\n");
        func.doc(&doc_str);
    }
    func.vis("pub");
    func.set_async(true);
    func.generic("C");
    func.arg("client", "&C");

    if let Some(params) = &delete.params {
        for (param_name_meta, param_type) in &params.params {
            let param_name = param_name_meta.value.as_str();
            let rust_ty = param_type_to_rust(param_type);
            func.arg(param_name, format!("&{}", rust_ty));
        }
    }

    func.ret(&return_ty);
    func.bound("C", "tokio_postgres::GenericClient");

    let body = generate_mutation_body(&generated.sql, &generated.params, !has_returning);
    func.line(block_to_string(&body));

    scope.push_fn(func);
}

fn generate_mutation_result_struct(
    ctx: &CodegenContext,
    struct_name: &str,
    table: &str,
    returning: &Returning,
    scope: &mut Scope,
) {
    let mut st = Struct::new(struct_name);
    st.vis("pub");
    st.derive("Debug");
    st.derive("Clone");
    st.derive("Facet");
    st.attr("facet(crate = dibs_runtime::facet)");

    for (col_name_meta, _) in &returning.columns {
        let col_name = col_name_meta.value.as_str();
        let rust_ty = ctx
            .schema
            .column_type(table, col_name)
            .unwrap_or_else(|| "String".to_string());
        st.field(format!("pub {col_name}"), &rust_ty);
    }

    scope.push_struct(st);
}

fn generate_mutation_body(
    sql: &str,
    param_order: &[dibs_sql::ParamName],
    execute_only: bool,
) -> Block {
    let mut block = Block::new("");

    // SQL constant
    block.line(format!("const SQL: &str = r#\"{}\"#;", sql));
    block.line("");

    let params: Vec<_> = param_order
        .iter()
        .filter(|p| !p.as_str().starts_with("__literal_"))
        .collect();

    if execute_only {
        // No RETURNING - use execute
        if params.is_empty() {
            block.line("let affected = client.execute(SQL, &[]).await?;");
        } else {
            let params_str = params
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            block.line(format!(
                "let affected = client.execute(SQL, &[{}]).await?;",
                params_str
            ));
        }
        block.line("Ok(affected)");
    } else {
        // Has RETURNING - use query
        if params.is_empty() {
            block.line("let rows = client.query(SQL, &[]).await?;");
        } else {
            let params_str = params
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            block.line(format!(
                "let rows = client.query(SQL, &[{}]).await?;",
                params_str
            ));
        }
        let mut match_block = Block::new("match rows.into_iter().next()");
        match_block.line("Some(row) => Ok(Some(from_row(&row)?)),");
        match_block.line("None => Ok(None),");
        block.push_block(match_block);
    }

    block
}

#[cfg(test)]
mod tests;
