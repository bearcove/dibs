# 009: Compile-Time Validation

**Priority:** Low

## Problem

Features that are parsed but ignored fail silently:
- Relation `where` (see #001)
- Relation `order_by` (see #002)

No validation for:
- Unknown tables
- Unknown columns
- Missing FK relationships

## Goal

```
warning: relation-level 'where' is not yet implemented
  --> queries.styx:15:7
   |
15 |       where{ locale "en" }
   |       ^^^^^^^^^^^^^^^^^^^
```

```
error: column 'nonexistent' does not exist in table 'product'
  --> queries.styx:8:5
```

## Implementation

Add validation pass after parsing:

```rust
pub fn validate_query(query: &Query, schema: &PlannerSchema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    
    // Check table exists
    // Check columns exist
    // Check FK relationships
    // Warn on unsupported features
    
    diagnostics
}
```

Integrate with build to fail on errors, emit warnings.
