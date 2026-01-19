# 007: Relation-Level ORDER BY

**Status:** TODO
**Priority:** High

## Problem

The `order_by` clause inside a relation is parsed but ignored during SQL generation.

### Current Behavior (Wrong)

```styx
ProductWithLatestTranslation @query{
  from product
  select{
    id
    translation @rel{
      from product_translation
      order_by{ created_at desc }  # IGNORED!
      first true
      select{ title }
    }
  }
}
```

Returns an arbitrary translation, not necessarily the latest.

### Expected Behavior

For `first: true`, should return the first row according to the specified ordering.

## Implementation Options

### Option A: Window function with ROW_NUMBER()

```sql
SELECT p.*, t.title
FROM product p
LEFT JOIN (
  SELECT *,
         ROW_NUMBER() OVER (PARTITION BY product_id ORDER BY created_at DESC) as rn
  FROM product_translation
) t ON p.id = t.product_id AND t.rn = 1
```

**Pros:** Correct semantics, single row per relation
**Cons:** More complex SQL, PostgreSQL-specific

### Option B: DISTINCT ON (PostgreSQL)

```sql
SELECT DISTINCT ON (p.id) p.*, t.title
FROM product p
LEFT JOIN product_translation t ON p.id = t.product_id
ORDER BY p.id, t.created_at DESC
```

**Pros:** Simple, efficient
**Cons:** PostgreSQL-specific, affects overall query ordering

### Option C: Subquery with LIMIT 1

```sql
SELECT p.*,
       (SELECT title FROM product_translation t
        WHERE t.product_id = p.id
        ORDER BY created_at DESC LIMIT 1) as translation_title
FROM product p
```

**Pros:** Clear semantics
**Cons:** Doesn't work well for multiple columns from relation

### Option D: Post-process in Rust

Keep all rows, sort in Rust, take first per parent.

**Pros:** Database-agnostic
**Cons:** Transfers more data than needed

## Recommendation

**Option A** (window function) for `first: true` relations - it's the most general solution that works with multiple columns.

For `first: false` (Vec), ordering is simpler - just add ORDER BY to the overall query.

## Implementation Steps

1. Store `order_by` in `JoinClause` or similar
2. For `first: true`: generate window function subquery
3. For `first: false`: append to overall ORDER BY (with table alias)

## Files to Modify

- `crates/dibs-query-gen/src/planner.rs` - Store relation ordering
- `crates/dibs-query-gen/src/sql.rs` - Generate ordered JOINs

## Testing

- Test `first: true` with `order_by{ created_at desc }` returns latest
- Test `first: false` with ordering returns sorted children
