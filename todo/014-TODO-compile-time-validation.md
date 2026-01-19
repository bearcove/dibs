# 014: Compile-Time Validation

**Status:** TODO
**Priority:** Low

## Goal

Provide better error messages and warnings at compile time for unsupported or problematic query patterns.

## Current Problems

### Silent Failures

Features that are parsed but ignored:
- Relation-level `where` clauses (see #006)
- Relation-level `order_by` (see #007)
- `@count` fields (see #008)

Users write these expecting them to work, but they're silently ignored.

### Missing Validation

- No check that `from` table exists in schema
- No check that selected columns exist
- No check that FK relationships exist for relations
- No type checking for filter values

## Desired Behavior

### Warnings for Unsupported Features

```
warning: relation-level 'where' clause is not yet implemented and will be ignored
  --> queries.styx:15:7
   |
15 |       where{ locale "en" }
   |       ^^^^^^^^^^^^^^^^^^^
   |
   = note: see https://github.com/.../issues/006
```

### Errors for Invalid Queries

```
error: column 'nonexistent' does not exist in table 'product'
  --> queries.styx:8:5
   |
8  |     nonexistent
   |     ^^^^^^^^^^^
   |
   = help: available columns: id, handle, status, active, ...
```

```
error: no foreign key relationship between 'product' and 'user'
  --> queries.styx:12:5
   |
12 |     author @rel{
   |     ^^^^^^
   |
   = help: add a foreign key or specify the join condition explicitly
```

## Implementation

### Validation Pass

Add a validation step after parsing, before SQL generation:

```rust
pub fn validate_query(query: &Query, schema: &PlannerSchema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Check table exists
    if !schema.tables.contains_key(&query.from) {
        diagnostics.push(Diagnostic::error(
            query.span,
            format!("table '{}' does not exist", query.from),
        ));
    }

    // Check columns exist
    for field in &query.select {
        if let Field::Column { name, span } = field {
            if !table_has_column(schema, &query.from, name) {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!("column '{}' does not exist", name),
                ));
            }
        }
    }

    // Warn about unsupported features
    for field in &query.select {
        if let Field::Relation { filters, order_by, span, .. } = field {
            if !filters.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    *span,
                    "relation-level 'where' is not yet implemented",
                ));
            }
            // ...
        }
    }

    diagnostics
}
```

### Diagnostic Type

```rust
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub span: Option<Span>,
    pub message: String,
    pub notes: Vec<String>,
}

pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}
```

### Integration with Build

In build.rs, fail on errors, emit warnings:

```rust
let diagnostics = validate_query(&query, &schema);
for d in &diagnostics {
    match d.level {
        DiagnosticLevel::Error => {
            eprintln!("cargo::error={}", d.format());
            has_errors = true;
        }
        DiagnosticLevel::Warning => {
            eprintln!("cargo::warning={}", d.format());
        }
    }
}
if has_errors {
    std::process::exit(1);
}
```

## Files to Modify

- `crates/dibs-query-gen/src/validate.rs` - New validation module
- `crates/dibs-query-gen/src/lib.rs` - Export validation
- Build scripts - Call validation

## Testing

- Test error on unknown table
- Test error on unknown column
- Test warning on unsupported feature
- Test that valid queries pass validation
