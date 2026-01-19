# 004: Single-Level JOINs (first: true)

**Status:** DONE

## Summary

Support relations in queries that resolve to LEFT JOINs via FK relationships.

## What Was Implemented

- Query planner module (`crates/dibs-query-gen/src/planner.rs`)
- FK resolution from schema annotations (`#[facet(dibs::fk = "table.column")]`)
- Bidirectional FK lookup (forward/belongs-to and reverse/has-many)
- LEFT JOIN generation with table aliasing (t0, t1, ...)
- Column aliasing for nested struct assembly
- Result assembly code generation (flat rows -> nested structs)
- Proper handling of nullable columns from JOINs

## Limitations (see #005, #006, #007)

- Only `first: true` works correctly
- `first: false` doesn't group by parent ID
- Relation-level `where` is ignored
- Relation-level `order_by` is ignored

## Example

```styx
ProductWithTranslation @query{
  params{ handle @string }
  from product
  where{ handle $handle }
  first true
  select{
    id
    handle
    translation @rel{
      from product_translation
      first true
      select{ locale, title, description }
    }
  }
}
```

Generates:
```sql
SELECT "t0"."id" AS "id", "t0"."handle" AS "handle",
       "t1"."locale" AS "translation_locale",
       "t1"."title" AS "translation_title",
       "t1"."description" AS "translation_description"
FROM "product" AS "t0"
LEFT JOIN "product_translation" AS "t1" ON t0.id = t1.product_id
WHERE "t0"."handle" = $1
```

## Files

- `crates/dibs-query-gen/src/planner.rs` - Query planner
- `crates/dibs-query-gen/src/sql.rs` - `generate_sql_with_joins()`
- `crates/dibs-query-gen/src/codegen.rs` - `generate_join_query_body()`

## Integration Test

`test_product_with_translation` in `examples/my-app-queries/tests/query_integration.rs`
