# 008: COUNT Aggregates

**Status:** TODO
**Priority:** Medium

## Problem

COUNT fields are parsed in the AST but generate placeholder code:

```rust
Field::Count { name, .. } => {
    code.push_str(&format!("            {}: 0, // TODO: COUNT\n", name));
}
```

### Desired Syntax

```styx
ProductStats @query{
  from product
  where{ status "published" }
  select{
    total @count
    active_count @count{ where{ active true } }
  }
}
```

Or for relation counts:
```styx
ProductWithVariantCount @query{
  from product
  select{
    id
    handle
    variant_count @count{
      from product_variant
    }
  }
}
```

## Implementation

### Simple COUNT (all rows)

```sql
SELECT COUNT(*) as total FROM product WHERE status = 'published'
```

### COUNT with filter

```sql
SELECT
  COUNT(*) as total,
  COUNT(*) FILTER (WHERE active = true) as active_count
FROM product
WHERE status = 'published'
```

### Relation COUNT

```sql
SELECT p.id, p.handle,
       (SELECT COUNT(*) FROM product_variant v WHERE v.product_id = p.id) as variant_count
FROM product p
```

Or with LEFT JOIN + GROUP BY:
```sql
SELECT p.id, p.handle, COUNT(v.id) as variant_count
FROM product p
LEFT JOIN product_variant v ON p.id = v.product_id
GROUP BY p.id, p.handle
```

## AST

Current AST already has:
```rust
Field::Count {
    name: String,
    span: Option<Span>,
    from: Option<String>,
    filters: Vec<Filter>,
}
```

## Implementation Steps

1. For simple count: Generate `COUNT(*)` in SELECT
2. For filtered count: Use `COUNT(*) FILTER (WHERE ...)`
3. For relation count: Use correlated subquery or JOIN + GROUP BY
4. Update codegen to extract count value from row

## Files to Modify

- `crates/dibs-query-gen/src/sql.rs` - Generate COUNT expressions
- `crates/dibs-query-gen/src/codegen.rs` - Generate count field extraction
- `crates/dibs-query-gen/src/planner.rs` - Handle counts in query plan

## Testing

- Test simple `@count`
- Test `@count{ where{ ... } }`
- Test relation `@count{ from ... }`
