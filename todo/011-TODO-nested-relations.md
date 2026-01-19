# 011: Nested Relations

**Status:** TODO
**Priority:** Medium

## Goal

Support relations within relations for multi-level data fetching.

## Desired Syntax

```styx
ProductWithVariantsAndPrices @query{
  from product
  select{
    id
    handle
    variants @rel{
      from product_variant
      select{
        id
        sku
        prices @rel{
          from variant_price
          select{ currency_code, amount }
        }
      }
    }
  }
}
```

Should generate nested Rust structs:
```rust
struct ProductWithVariantsAndPricesResult {
    id: i64,
    handle: String,
    variants: Vec<Variant>,
}

struct Variant {
    id: i64,
    sku: String,
    prices: Vec<Price>,
}

struct Price {
    currency_code: String,
    amount: Decimal,
}
```

## SQL Generation Options

### Option A: Multiple JOINs

```sql
SELECT p.id, p.handle,
       v.id as variant_id, v.sku,
       pr.currency_code, pr.amount
FROM product p
LEFT JOIN product_variant v ON p.id = v.product_id
LEFT JOIN variant_price pr ON v.id = pr.variant_id
```

Then group in Rust: product -> variants -> prices

**Pros:** Single query
**Cons:** Data duplication, complex grouping logic

### Option B: Separate queries

```sql
-- Query 1: Products
SELECT id, handle FROM product

-- Query 2: Variants for product IDs
SELECT * FROM product_variant WHERE product_id = ANY($1)

-- Query 3: Prices for variant IDs
SELECT * FROM variant_price WHERE variant_id = ANY($1)
```

Then assemble in Rust.

**Pros:** No data duplication, simpler queries
**Cons:** Multiple round trips, N+1 if not batched

### Option C: JSON aggregation

```sql
SELECT p.id, p.handle,
       (SELECT json_agg(json_build_object(
         'id', v.id,
         'sku', v.sku,
         'prices', (SELECT json_agg(json_build_object(
           'currency_code', pr.currency_code,
           'amount', pr.amount
         )) FROM variant_price pr WHERE pr.variant_id = v.id)
       )) FROM product_variant v WHERE v.product_id = p.id) as variants
FROM product p
```

**Pros:** Single query, proper nesting
**Cons:** Complex SQL, PostgreSQL-specific

## Recommendation

**Option A** for initial implementation, with careful grouping logic. Can optimize to Option C later.

## Implementation Steps

1. Extend planner to handle nested relations recursively
2. Generate multiple JOINs with proper aliasing (t0, t1, t2, ...)
3. Generate nested grouping code in Rust
4. Track nesting depth to build correct struct paths

## Complexity

This depends on #005 (Vec grouping) being solved first. Nested relations multiply the grouping complexity.

## Files to Modify

- `crates/dibs-query-gen/src/planner.rs` - Recursive relation planning
- `crates/dibs-query-gen/src/sql.rs` - Multiple JOIN generation
- `crates/dibs-query-gen/src/codegen.rs` - Nested struct assembly
