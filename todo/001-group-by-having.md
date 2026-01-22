# 008: GROUP BY / HAVING

**Priority:** Low

## Syntax

```styx
SalesByCategory @query{
  from order_item
  group_by{ category_id }
  having{ total @gt(1000) }
  select{
    category_id
    total @sum{ column "amount" }
    count @count
    avg_amount @avg{ column "amount" }
  }
}
```

→
```sql
SELECT category_id,
       SUM(amount) as total,
       COUNT(*) as count,
       AVG(amount) as avg_amount
FROM order_item
GROUP BY category_id
HAVING SUM(amount) > 1000
```

## Aggregates to Support

| Syntax | SQL |
|--------|-----|
| `@count` | `COUNT(*)` |
| `@count{ column "x" }` | `COUNT(x)` |
| `@sum{ column "x" }` | `SUM(x)` |
| `@avg{ column "x" }` | `AVG(x)` |
| `@min{ column "x" }` | `MIN(x)` |
| `@max{ column "x" }` | `MAX(x)` |

## Implementation

1. Add `group_by`, `having` to Query
2. Add aggregate Field variants
3. Validate: non-aggregate columns must be in GROUP BY
