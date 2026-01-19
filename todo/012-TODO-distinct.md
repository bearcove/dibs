# 012: DISTINCT Keyword

**Status:** TODO
**Priority:** Low

## Goal

Support `DISTINCT` and `DISTINCT ON` in queries.

## Desired Syntax

### Simple DISTINCT

```styx
UniqueStatuses @query{
  from product
  distinct true
  select{ status }
}
```

Generates:
```sql
SELECT DISTINCT status FROM product
```

### DISTINCT ON (PostgreSQL)

```styx
LatestPerCategory @query{
  from product
  distinct_on{ category_id }
  order_by{ category_id asc, created_at desc }
  select{ id, category_id, handle, created_at }
}
```

Generates:
```sql
SELECT DISTINCT ON (category_id) id, category_id, handle, created_at
FROM product
ORDER BY category_id ASC, created_at DESC
```

## Implementation

### AST Changes

Add to `Query`:
```rust
/// Whether to use DISTINCT
pub distinct: bool,
/// Columns for DISTINCT ON (PostgreSQL-specific)
pub distinct_on: Vec<String>,
```

### SQL Generation

```rust
// In generate_simple_sql:
sql.push_str("SELECT ");
if query.distinct {
    sql.push_str("DISTINCT ");
} else if !query.distinct_on.is_empty() {
    let cols = query.distinct_on.iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");
    sql.push_str(&format!("DISTINCT ON ({}) ", cols));
}
```

## Files to Modify

- `crates/dibs-query-gen/src/ast.rs` - Add distinct fields
- `crates/dibs-query-gen/src/parse.rs` - Parse distinct syntax
- `crates/dibs-query-gen/src/sql.rs` - Generate DISTINCT SQL

## Testing

- Test simple `distinct true`
- Test `distinct_on` with ordering
- Verify DISTINCT ON requires compatible ORDER BY
