# 003: LIMIT/OFFSET Pagination

**Status:** DONE

## Summary

Support pagination with literal and parameterized LIMIT/OFFSET.

## What Was Implemented

- `limit N` - literal limit
- `limit $param` - parameterized limit
- `offset N` - literal offset
- `offset $param` - parameterized offset
- SQL generation for both cases

## Example

```styx
ProductsPaginated @query{
  params{ page_size @int, page_offset @int }
  from product
  where{ deleted_at @null }
  order_by{ handle asc }
  limit $page_size
  offset $page_offset
  select{ id, handle }
}
```

Generates:
```sql
SELECT "id", "handle" FROM "product"
WHERE "deleted_at" IS NULL
ORDER BY "handle" ASC
LIMIT $1 OFFSET $2
```

## Integration Test

`test_products_paginated` in `examples/my-app-queries/tests/query_integration.rs`
