# LSP Extension Refactor: styx_tree::Value → Typed Schema

## Current State

The LSP extension in `lsp_extension.rs` operates on `styx_tree::Value` - the raw untyped AST from the Styx parser. It manually traverses the tree checking for keys like `"params"`, `"from"`, `"where"`, etc.

## Goal

Parse the styx content into `QueryFile` (typed schema) using `facet_styx::from_str`, then operate on typed structs. The `Meta<T>` wrapper provides spans, doc comments, and tags for each value.

## Type Mappings

```
styx_tree::Value        → QueryFile, Decl, Query, Insert, etc.
styx_tree::Span         → facet_reflect::Span (re-exported as dibs_query_schema::Span)
entry.key.as_str()      → meta_key.as_str()
entry.value.span        → meta_value.span
value.tag.name          → meta_value.tag or match on Decl variant
```

## Current LSP Features & How to Implement with Typed Schema

### 1. Diagnostics (`collect_diagnostics`)

#### 1.1 Unknown table validation
**Current**: Checks `entry.key.as_str() == "from" | "into" | "table"`, then validates table name exists in schema.
**Typed**: 
```rust
// For Query
if let Some(from) = &query.from {
    if !schema.tables.iter().any(|t| t.name == from.as_str()) {
        if let Some(span) = from.span {
            diagnostics.push(/* unknown table */);
        }
    }
}
// Similar for Insert.into, Update.table, Delete.from
```

#### 1.2 OFFSET without LIMIT warning
**Current**: Tracks `has_offset`, `has_limit` booleans while iterating entries.
**Typed**:
```rust
if query.offset.is_some() && query.limit.is_none() {
    if let Some(span) = query.offset.as_ref().and_then(|o| o.span) {
        diagnostics.push(/* offset without limit */);
    }
}
```

#### 1.3 Large OFFSET warning
**Current**: Parses offset value string, checks if > 1000.
**Typed**:
```rust
if let Some(offset) = &query.offset {
    if let Ok(val) = offset.as_str().parse::<i64>() {
        if val > 1000 {
            if let Some(span) = offset.span {
                diagnostics.push(/* large offset */);
            }
        }
    }
}
```

#### 1.4 LIMIT without ORDER BY warning
**Current**: Checks `has_limit && !has_order_by` for @query.
**Typed**:
```rust
if query.limit.is_some() && query.order_by.is_none() {
    if let Some(span) = query.limit.as_ref().and_then(|l| l.span) {
        diagnostics.push(/* limit without order-by */);
    }
}
```

#### 1.5 FIRST without ORDER BY warning
**Current**: Checks `has_first && !has_order_by` for @query.
**Typed**:
```rust
// Using extension traits: .value() for Copy types, .value_as_deref() for String→&str
if query.first.value().unwrap_or(false) && query.order_by.is_none() {
    if let Some(span) = query.first.meta_span() {
        diagnostics.push(/* first without order-by */);
    }
}
```

#### 1.6 @update/@delete without WHERE error
**Current**: Checks `!has_where` for update/delete tags.
**Typed**:
```rust
// For Update
if update.where_clause.is_none() {
    // Need span of the declaration - use the table span or declaration key span
    diagnostics.push(/* mutation without where */);
}
// For Delete
if delete.where_clause.is_none() {
    diagnostics.push(/* mutation without where */);
}
```

#### 1.7 Unused params warning
**Current**: Collects declared params, then `collect_param_refs` walks tree to find $param usages.
**Typed**:
```rust
fn collect_param_refs_typed(query: &Query) -> Vec<String> {
    let mut refs = Vec::new();
    // Check where clause
    if let Some(where_clause) = &query.where_clause {
        for (key, filter) in &where_clause.filters {
            // Key itself might be a param ref if it's shorthand
            // Check filter value for $param references
            collect_refs_from_filter(filter, &mut refs);
        }
    }
    // Check select, values, etc.
    refs
}

// Then compare declared vs used
for (param_name, param_type) in &params.params {
    if !used_params.contains(param_name.as_str()) {
        if let Some(span) = param_name.span {
            diagnostics.push(/* unused param */);
        }
    }
}
```

#### 1.8 Soft delete checks (deleted_at column)
**Current**: Checks if table has `deleted_at` column, warns if query doesn't filter on it.
**Typed**:
```rust
let table_name = query.from.as_ref().map(|f| f.as_str());
if let Some(table) = table_name.and_then(|n| schema.tables.iter().find(|t| t.name == n)) {
    let has_deleted_at = table.columns.iter().any(|c| c.name == "deleted_at");
    if has_deleted_at {
        let filters_deleted_at = query.where_clause
            .as_ref()
            .map(|w| w.filters.contains_key("deleted_at"))
            .unwrap_or(false);
        if !filters_deleted_at {
            diagnostics.push(/* missing deleted_at filter */);
        }
    }
}
```

#### 1.9 @delete on soft-delete table warning
**Current**: Warns when using @delete on table with deleted_at.
**Typed**:
```rust
// In Delete handling
if has_deleted_at_column {
    // Use delete.from.span for the diagnostic location
    diagnostics.push(/* hard delete on soft delete table */);
}
```

#### 1.10 Empty select block warning
**Current**: Checks if select object has no non-rel entries.
**Typed**:
```rust
if let Some(select) = &query.select {
    let has_columns = select.fields.iter().any(|(_, field_def)| {
        !matches!(field_def, Some(FieldDef::Rel(_)))
    });
    if !has_columns && select.fields.is_empty() {
        // Need span of select block
        diagnostics.push(/* empty select */);
    }
}
```

#### 1.11 Unknown column validation
**Current**: Validates column names in select/where/order-by against table schema.
**Typed**:
```rust
fn validate_columns_typed(
    select: &Select,
    table: &TableInfo,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (col_name, field_def) in &select.fields {
        // Skip @rel fields
        if matches!(field_def, Some(FieldDef::Rel(_))) {
            continue;
        }
        if !table.columns.iter().any(|c| c.name == col_name.as_str()) {
            if let Some(span) = col_name.span {
                diagnostics.push(/* unknown column */);
            }
        }
    }
}
```

#### 1.12 Param type mismatch validation
**Current**: Compares declared param types against column types in where clause.
**Typed**:
```rust
fn validate_param_types_typed(
    where_clause: &Where,
    table: &TableInfo,
    params: &Params,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (col_name, filter) in &where_clause.filters {
        if let Some(column) = table.columns.iter().find(|c| c.name == col_name.as_str()) {
            // Extract param name from filter, check type compatibility
            // FilterValue::Eq(s) where s starts with '$'
            // FilterValue::Ilike(vec) etc.
        }
    }
}
```

#### 1.13 Redundant param refs hint
**Current**: Finds `column $column` that can be shortened to `column`.
**Typed**:
```rust
fn find_redundant_refs_typed(values: &Values) -> Vec<RedundantParamRef> {
    let mut results = Vec::new();
    for (col_name, value_expr) in &values.columns {
        if let Some(ValueExpr::Other { tag: None, content: Some(Payload::Scalar(s)) }) = value_expr {
            if let Some(param_name) = s.strip_prefix('$') {
                if param_name == col_name.as_str() {
                    // This is redundant
                    results.push(RedundantParamRef {
                        name: col_name.value.clone(),
                        // Need spans from Meta
                    });
                }
            }
        }
    }
    results
}
```

#### 1.14 @rel validation (FK relationship check)
**Current**: Validates FK exists between parent table and relation table.
**Typed**:
```rust
fn lint_relation_typed(
    rel: &Relation,
    parent_table: Option<&str>,
    schema: &SchemaInfo,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // first without order-by
    if rel.first.as_ref().map(|f| f.value).unwrap_or(false) && rel.order_by.is_none() {
        diagnostics.push(/* rel first without order-by */);
    }
    
    // FK relationship check
    if let (Some(parent), Some(from)) = (parent_table, rel.from.as_ref()) {
        if !has_fk_relationship(parent, from.as_str(), schema) {
            if let Some(span) = from.span {
                diagnostics.push(/* no FK relationship */);
            }
        }
    }
}
```

### 2. Inlay Hints (`collect_inlay_hints`)

**Current**: Walks tree to find column references, adds type hints after column names.
**Typed**:
```rust
fn collect_inlay_hints_typed(
    query_file: &QueryFile,
    schema: &SchemaInfo,
    hints: &mut Vec<InlayHint>,
) {
    for (name, decl) in &query_file.0 {
        let table_name = match decl {
            Decl::Query(q) => q.from.as_ref().map(|f| f.as_str()),
            Decl::Insert(i) => Some(i.into.as_str()),
            // etc.
        };
        
        if let Some(table) = table_name.and_then(|n| schema.tables.iter().find(|t| t.name == n)) {
            // Add hints for select fields
            if let Decl::Query(q) = decl {
                if let Some(select) = &q.select {
                    for (col_name, _) in &select.fields {
                        if let Some(col) = table.columns.iter().find(|c| c.name == col_name.as_str()) {
                            if let Some(span) = col_name.span {
                                hints.push(InlayHint {
                                    position: offset_to_position(span.end()),
                                    label: format!(": {}", col.sql_type),
                                    // ...
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
```

### 3. Completions (`completions`)

**Current**: Uses path segments to determine context, offers table/column/field completions.
**Typed**: Completions still need path context from the LSP host. The typed schema helps with:
- Schema type completions (query fields like "from", "select", etc.) - already uses `facet_styx::schema_from_type`
- Column completions when we know the table

### 4. Hover (`hover`)

**Current**: Provides hover info for tables and columns.
**Typed**: Similar approach - use path to find what's being hovered, look up in schema.

### 5. Definition (`definition`)

**Current**: Finds $param at cursor, jumps to param declaration.
**Typed**:
```rust
// Parse to QueryFile
// Find which declaration contains the cursor offset
// If cursor is on a $param reference, find the param declaration in that query's params
// Return the span of the param declaration
```

## Implementation Strategy

1. **Add `parse_query_file` helper** ✅ Done
2. **Rewrite `collect_diagnostics`** to use typed schema
   - Parse content to QueryFile
   - Iterate over declarations
   - For each Decl variant, run appropriate checks
   - Use Meta<T>.span for diagnostic locations
3. **Rewrite `collect_inlay_hints`** similarly
4. **Update helper methods** (`validate_columns`, `validate_param_types`, etc.) to take typed args
5. **Remove old styx_tree traversal code**
6. **Keep completions/hover mostly as-is** since they rely on LSP host path context

## Span Access Pattern

The key pattern throughout:
```rust
// Old
if let Some(span) = &entry.value.span { ... }

// New  
if let Some(span) = meta_value.span { ... }
// or for Option<Meta<T>>
if let Some(span) = opt_meta.as_ref().and_then(|m| m.span) { ... }
```

## Notes

- `Meta<T>` now has: `value`, `span`, `doc`, `tag`
- `Meta<String>` has `as_str()` method
- `Meta<T>` implements `Borrow<str>` for `Meta<String>` so IndexMap lookups work with `&str`
- Parsing failures return None - styx-lsp handles syntax errors
