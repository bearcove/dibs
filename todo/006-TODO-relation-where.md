# 006: Relation-Level WHERE Clauses

**Status:** TODO
**Priority:** High

## Problem

The `where` clause inside a relation is parsed but completely ignored during SQL generation.

### Current Behavior (Wrong)

```styx
ProductWithEnglishTranslation @query{
  params{ handle @string }
  from product
  where{ handle $handle }
  select{
    id
    translation @rel{
      from product_translation
      where{ locale "en" }  # THIS IS IGNORED!
      first true
      select{ title }
    }
  }
}
```

Currently generates:
```sql
SELECT ... FROM "product" AS "t0"
LEFT JOIN "product_translation" AS "t1" ON t0.id = t1.product_id
WHERE "t0"."handle" = $1
-- No filter on t1.locale!
```

### Expected Behavior

```sql
SELECT ... FROM "product" AS "t0"
LEFT JOIN "product_translation" AS "t1"
  ON t0.id = t1.product_id AND "t1"."locale" = 'en'
WHERE "t0"."handle" = $1
```

Or alternatively with WHERE:
```sql
SELECT ... FROM "product" AS "t0"
LEFT JOIN "product_translation" AS "t1" ON t0.id = t1.product_id
WHERE "t0"."handle" = $1 AND ("t1"."id" IS NULL OR "t1"."locale" = 'en')
```

## Implementation

### Option A: Add filters to JOIN ON clause

For LEFT JOIN to preserve parent rows, filters should go in the ON clause:
```sql
LEFT JOIN child ON parent.id = child.parent_id AND child.filter_col = value
```

### Option B: Use subquery

```sql
LEFT JOIN (
  SELECT * FROM product_translation WHERE locale = 'en'
) AS "t1" ON t0.id = t1.product_id
```

## Recommendation

**Option A** - cleaner SQL, better performance.

## Implementation Steps

1. In `QueryPlanner::plan()`, pass relation filters to `JoinClause`
2. Update `JoinClause` to hold optional filter conditions
3. In `QueryPlan::from_sql()`, append relation filters to ON clause
4. Handle both literal values and parameters in relation filters

## Files to Modify

- `crates/dibs-query-gen/src/planner.rs` - Add filters to JoinClause
- `crates/dibs-query-gen/src/sql.rs` - Generate ON clause with filters

## Testing

- Test with literal filter: `where{ locale "en" }`
- Test with parameter filter: `where{ locale $locale }`
- Test that parent rows without matching children still appear (LEFT JOIN behavior)
