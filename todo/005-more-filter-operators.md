# 006: More Filter Operators

**Priority:** Medium

## Current Operators

- `@null` → `IS NULL`
- `@ilike($x)` → `ILIKE $1`
- `@like($x)` → `LIKE $1`
- `@gt($x)` → `> $1`
- `@lt($x)` → `< $1`
- Bare value → `= $1`

## Missing Operators

| Syntax | SQL |
|--------|-----|
| `@ne($x)` | `!= $1` |
| `@gte($x)` | `>= $1` |
| `@lte($x)` | `<= $1` |
| `@in($x)` | `= ANY($1)` |
| `@not_in($x)` | `!= ALL($1)` |
| `@between($a, $b)` | `BETWEEN $1 AND $2` |
| `@not_null` | `IS NOT NULL` |
| `@starts_with($x)` | `LIKE $1 || '%'` |
| `@ends_with($x)` | `LIKE '%' || $1` |

## Implementation

1. Add variants to `FilterValue` enum in `dibs-query-schema`
2. Add variants to `FilterOp` enum in `ast.rs`
3. Update `convert_filter_value()` in `parse.rs`
4. Update `format_filter()` in `sql.rs`

## Example

```styx
where{
  price @gte($min_price)
  price @lte($max_price)
  status @in($statuses)
  deleted_at @not_null
}
```
