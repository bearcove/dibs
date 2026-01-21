# 007: DISTINCT

**Priority:** Low

## Syntax

**Simple DISTINCT:**
```styx
UniqueStatuses @query{
  from product
  distinct true
  select{ status }
}
```
→ `SELECT DISTINCT status FROM product`

**DISTINCT ON (PostgreSQL):**
```styx
LatestPerCategory @query{
  from product
  distinct_on{ category_id }
  order_by{ category_id asc, created_at desc }
  select{ id, category_id, handle }
}
```
→ `SELECT DISTINCT ON (category_id) ... ORDER BY category_id, created_at DESC`

## Implementation

1. Add to `Query` in schema: `distinct: Option<bool>`, `distinct_on: Option<Vec<String>>`
2. Add to AST
3. Generate in `sql.rs`

Note: `DISTINCT ON` requires compatible `ORDER BY`.
