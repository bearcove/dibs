# 004: Bulk Insert

**Priority:** Medium

## Goal

Insert multiple rows with a single query, where SQL stays constant and arrays are passed.

## Syntax

```styx
BulkCreateProducts @insert-many{
  params {handle, status}
  into product
  values {..$, created_at @now}
  returning {id, ..$}
}
```

## Features

### `@insert-many` tag

Distinct from `@insert` - generates code accepting `&[Params]` instead of individual params.

### Type inference for params

```styx
params {handle, status}
```

Types inferred from target table columns (via `into product`). No need for `@string`, `@int` etc.

LSP can show inlay hints: `params {handle: TEXT, status: TEXT}`

### Spread operator `..$`

Spreads all params. Context-dependent:

| Context | Meaning |
|---------|---------|
| `values {..$}` | Insert param values into same-named columns |
| `returning {..$}` | Select columns matching param names |

Can mix with other values:

```styx
values {..$, created_at @now, updated_at @now, active true}
returning {id, ..$, created_at}
```

## Generated SQL

Uses `UNNEST` for constant SQL with variable row count:

```sql
INSERT INTO product (handle, status, created_at)
SELECT handle, status, NOW()
FROM UNNEST($1::text[], $2::text[]) AS t(handle, status)
RETURNING id, handle, status
```

## Generated Rust

Option A - Parallel arrays:
```rust
pub async fn bulk_create_products(
    client: &impl GenericClient,
    handles: &[String],
    statuses: &[String],
) -> Result<Vec<BulkCreateProductsResult>, Error>
```

Option B - Slice of structs (nicer API):
```rust
pub struct BulkCreateProductsParams {
    pub handle: String,
    pub status: String,
}

pub async fn bulk_create_products(
    client: &impl GenericClient,
    items: &[BulkCreateProductsParams],
) -> Result<Vec<BulkCreateProductsResult>, Error> {
    // Internally converts to parallel arrays for UNNEST
    let handles: Vec<_> = items.iter().map(|i| &i.handle).collect();
    let statuses: Vec<_> = items.iter().map(|i| &i.status).collect();
    // ...
}
```

Option B is probably better UX.

## Also applies to `@insert`

The spread operator and type inference should work for single inserts too:

```styx
CreateProduct @insert{
  params {handle, status}
  into product
  values {..$, created_at @now}
  returning {id, ..$}
}
```

## Future: `@update-many`, `@upsert-many`?

Same pattern could apply to bulk updates/upserts.
