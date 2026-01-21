# 002: Relation-Level ORDER BY

**Priority:** High

## Problem

Relation `order_by` is parsed but ignored. Returns arbitrary row instead of ordered.

```styx
ProductWithLatestTranslation @query{
  from product
  select{
    id
    translation @rel{
      from product_translation
      order_by{ updated_at desc }  # IGNORED
      first true
      select{ title }
    }
  }
}
```

## Where It Breaks

**Schema** (`dibs-query-schema`): `Relation` struct doesn't have `order_by` field!

**Parsing** (`parse.rs`): hardcoded to empty:
```rust
Field::Relation {
    order_by: Vec::new(),  // BUG
}
```

## Fix

1. Add `order_by: Option<OrderBy>` to `Relation` in `dibs-query-schema`
2. Update `parse.rs` to convert it
3. For `first: true`: use window function or LATERAL
4. For `first: false`: append to overall ORDER BY

## SQL for `first: true`

**Window function:**
```sql
LEFT JOIN (
  SELECT *, ROW_NUMBER() OVER (PARTITION BY product_id ORDER BY updated_at DESC) as rn
  FROM product_translation
) t ON p.id = t.product_id AND t.rn = 1
```

**LATERAL:**
```sql
LEFT JOIN LATERAL (
  SELECT * FROM product_translation t
  WHERE t.product_id = p.id
  ORDER BY updated_at DESC LIMIT 1
) t ON true
```

## SQL for `first: false`

Append to query ORDER BY with table alias:
```sql
ORDER BY t0.id, t1.updated_at DESC
```
