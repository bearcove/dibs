# 003: Nested Relations

**Priority:** High

## Problem

Only single-level relations work. Planner doesn't recurse.

```styx
ProductWithVariantsAndPrices @query{
  from product
  select{
    id
    variants @rel{
      from product_variant
      select{
        id
        sku
        prices @rel{           # NOT SUPPORTED
          from variant_price
          select{ currency_code, amount }
        }
      }
    }
  }
}
```

## Goal

```rust
struct Result {
    id: i64,
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

## SQL

Multiple JOINs:
```sql
SELECT p.id,
       v.id as variant_id, v.sku,
       pr.currency_code, pr.amount
FROM product p
LEFT JOIN product_variant v ON p.id = v.product_id
LEFT JOIN variant_price pr ON v.id = pr.variant_id
```

Then nested grouping in Rust: product → variants → prices

## Fix

1. Make `QueryPlanner::plan()` recursive
2. Generate aliases at each level (t0, t1, t2...)
3. Extend codegen for nested HashMap grouping
4. Track parent keys at each nesting level

Vec grouping foundation exists, just needs recursion.
