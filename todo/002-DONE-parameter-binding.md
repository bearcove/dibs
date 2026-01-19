# 002: Parameter Binding

**Status:** DONE

## Summary

Support parameterized queries with `$name` syntax, generating `$1, $2, ...` placeholders.

## What Was Implemented

- `params{ name @type }` declaration in queries
- Parameter types: `@string`, `@int`, `@bool`, `@uuid`, `@decimal`, `@timestamp`
- Optional parameters: `@optional(@string)`
- `$name` references in WHERE, LIMIT, OFFSET
- SQL placeholder generation (`$1`, `$2`, etc.)
- Parameter order tracking for runtime binding

## Security

- Parameters are never interpolated into SQL
- String literals are escaped (single quotes doubled)
- Table/column names come from compile-time .styx files, not user input

## Example

```styx
ProductByHandle @query{
  params{ handle @string }
  from product
  where{ handle $handle }
  first true
  select{ id, handle }
}
```

Generates:
```sql
SELECT "id", "handle" FROM "product" WHERE "handle" = $1
```

```rust
pub async fn product_by_handle<C>(
    client: &C,
    handle: &String,
) -> Result<Option<ProductByHandleResult>, QueryError>
```
