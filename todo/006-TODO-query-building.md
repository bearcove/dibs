# Phase 006: Query Building (Stretch)

Type-safe query building based on schema definitions.

## Idea

Since we know the schema via facet reflection, we can generate type-safe queries:

```rust
let users = User::select()
    .where_(User::email.eq("alice@example.com"))
    .fetch_one(&client)
    .await?;

let count = User::select()
    .where_(User::tenant_id.eq(42))
    .count(&client)
    .await?;
```

## Tasks

TBD — this is a stretch goal after core migration functionality works.

## Acceptance criteria

- [ ] Basic SELECT with WHERE
- [ ] INSERT returning
- [ ] UPDATE with WHERE
- [ ] DELETE with WHERE
- [ ] JOINs based on foreign keys
