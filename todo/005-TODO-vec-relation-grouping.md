# 005: Fix Vec<T> Relation Grouping

**Status:** TODO
**Priority:** Critical

## Problem

When a relation has `first: false` (returns `Vec<T>`), the current implementation is broken. It creates a single-element vec per row instead of grouping children by parent ID.

### Current Behavior (Wrong)

Query with has-many relation:
```styx
ProductWithVariants @query{
  from product
  select{
    id
    variants @rel{
      from product_variant
      first false  # Vec<ProductVariant>
      select{ id, sku, title }
    }
  }
}
```

If product has 3 variants, SQL returns 3 rows. Current code produces:
```rust
// WRONG: 3 separate results, each with 1 variant
[
  ProductWithVariantsResult { id: 1, variants: [Variant { sku: "A" }] },
  ProductWithVariantsResult { id: 1, variants: [Variant { sku: "B" }] },
  ProductWithVariantsResult { id: 1, variants: [Variant { sku: "C" }] },
]
```

### Expected Behavior

```rust
// CORRECT: 1 result with all variants
[
  ProductWithVariantsResult {
    id: 1,
    variants: [
      Variant { sku: "A" },
      Variant { sku: "B" },
      Variant { sku: "C" },
    ]
  },
]
```

## Solution Options

### Option A: Post-process grouping in Rust

After fetching flat rows, group by parent columns and collect children:

```rust
let mut results: HashMap<ParentKey, ParentResult> = HashMap::new();
for row in rows {
    let key = (row.get("id"),);
    let entry = results.entry(key).or_insert_with(|| ParentResult { ... });
    if let Some(child) = extract_child(&row) {
        entry.children.push(child);
    }
}
results.into_values().collect()
```

**Pros:** Works with current SQL, no DB-specific features
**Cons:** More data transfer, memory usage for large result sets

### Option B: Use SQL aggregation with JSON

```sql
SELECT p.id, p.handle,
       COALESCE(json_agg(json_build_object(
         'id', v.id, 'sku', v.sku, 'title', v.title
       )) FILTER (WHERE v.id IS NOT NULL), '[]') AS variants
FROM product p
LEFT JOIN product_variant v ON p.id = v.product_id
GROUP BY p.id, p.handle
```

**Pros:** Single row per parent, DB does the grouping
**Cons:** PostgreSQL-specific, JSON parsing overhead

### Option C: Lateral subquery

```sql
SELECT p.*,
       (SELECT json_agg(row_to_json(v))
        FROM product_variant v
        WHERE v.product_id = p.id) AS variants
FROM product p
```

**Pros:** Clean separation, can add ordering/filtering per relation
**Cons:** PostgreSQL-specific, may be slower

## Recommendation

**Option A** for initial implementation - it's database-agnostic and straightforward. Can optimize to Option B/C later if performance is an issue.

## Implementation Steps

1. Identify parent columns (those not from relations) - these form the grouping key
2. Generate HashMap-based collection code
3. After iteration, convert HashMap values to Vec
4. Handle the `first: true` case for consistency (take first from group)

## Files to Modify

- `crates/dibs-query-gen/src/codegen.rs` - `generate_join_query_body()`

## Testing

- Add `test_product_with_variants` integration test
- Test with 0, 1, and multiple children per parent
- Test with multiple parents
