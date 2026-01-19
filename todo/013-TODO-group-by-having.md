# 013: GROUP BY / HAVING

**Status:** TODO
**Priority:** Low

## Goal

Support GROUP BY clauses with aggregate functions and HAVING filters.

## Desired Syntax

```styx
SalesByCategory @query{
  from order_item
  group_by{ category_id }
  having{ total_sales @gt(1000) }
  select{
    category_id
    total_sales @sum{ column "amount" }
    order_count @count
    avg_order @avg{ column "amount" }
  }
}
```

Generates:
```sql
SELECT category_id,
       SUM(amount) as total_sales,
       COUNT(*) as order_count,
       AVG(amount) as avg_order
FROM order_item
GROUP BY category_id
HAVING SUM(amount) > 1000
```

## Aggregate Functions to Support

| Function | Syntax | SQL |
|----------|--------|-----|
| COUNT | `@count` | `COUNT(*)` |
| COUNT column | `@count{ column "col" }` | `COUNT(col)` |
| SUM | `@sum{ column "col" }` | `SUM(col)` |
| AVG | `@avg{ column "col" }` | `AVG(col)` |
| MIN | `@min{ column "col" }` | `MIN(col)` |
| MAX | `@max{ column "col" }` | `MAX(col)` |
| ARRAY_AGG | `@array_agg{ column "col" }` | `ARRAY_AGG(col)` |
| STRING_AGG | `@string_agg{ column "col", separator ", " }` | `STRING_AGG(col, ', ')` |

## AST Changes

Add to `Query`:
```rust
/// GROUP BY columns
pub group_by: Vec<String>,
/// HAVING filters (applied to aggregates)
pub having: Vec<Filter>,
```

Add aggregate field type:
```rust
pub enum Field {
    // ... existing variants ...
    Aggregate {
        name: String,
        function: AggregateFunction,
        column: Option<String>,
        distinct: bool,
    },
}

pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    ArrayAgg,
    StringAgg { separator: String },
}
```

## Implementation Steps

1. Parse `group_by{ col1, col2 }` syntax
2. Parse aggregate field syntax (`@sum{ column "x" }`)
3. Parse `having{ ... }` syntax (similar to `where`)
4. Generate GROUP BY clause
5. Generate aggregate expressions in SELECT
6. Generate HAVING clause

## Validation

- All non-aggregate SELECT columns must appear in GROUP BY
- HAVING can only reference aggregates or GROUP BY columns

## Files to Modify

- `crates/dibs-query-gen/src/ast.rs` - Add GROUP BY, HAVING, aggregates
- `crates/dibs-query-gen/src/parse.rs` - Parse new syntax
- `crates/dibs-query-gen/src/sql.rs` - Generate GROUP BY SQL

## Testing

- Test simple GROUP BY with COUNT
- Test multiple aggregates
- Test HAVING filter
- Test validation errors for invalid queries
