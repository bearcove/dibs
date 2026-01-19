# Integration Tests

## Overview

We need integration tests that run against a real Postgres database to verify:
1. Schema introspection
2. Diff generation
3. Migration generation
4. Migration execution
5. Rename detection

## Infrastructure

Use `testcontainers` (already in dev-dependencies) to spin up Postgres.

```rust
use testcontainers::{clients::Cli, images::postgres::Postgres};

#[tokio::test]
async fn test_migration_rename() {
    let docker = Cli::default();
    let pg = docker.run(Postgres::default());
    let conn_str = format!(
        "postgres://postgres:postgres@localhost:{}/postgres",
        pg.get_host_port_ipv4(5432)
    );
    // ... test logic
}
```

## Test Categories

### 1. Introspection Tests

Verify we correctly read schema from Postgres:
- [ ] Tables with all column types
- [ ] Primary keys (single and composite)
- [ ] Foreign keys
- [ ] Unique constraints
- [ ] Indices
- [ ] Default values
- [ ] Nullable vs non-nullable

### 2. Diff Tests

Verify diff detects all change types:
- [ ] Add table
- [ ] Drop table
- [ ] Rename table (new!)
- [ ] Add column
- [ ] Drop column
- [ ] Rename column (future)
- [ ] Change column type
- [ ] Change nullability
- [ ] Change default
- [ ] Add/drop FK
- [ ] Add/drop index

### 3. Rename Detection Tests

Verify rename detection works correctly:
- [ ] `users` -> `user` (basic plural to singular)
- [ ] `categories` -> `category` (ies -> y)
- [ ] `post_tags` -> `post_tag` (compound name)
- [ ] Multiple renames in one diff
- [ ] Rename with column changes
- [ ] Rename with FK updates
- [ ] Non-rename (completely different table)

### 4. Migration Execution Tests

Verify migrations run correctly:
- [ ] Simple table creation
- [ ] Table with FKs (correct order)
- [ ] Table rename
- [ ] Multiple renames with FK dependencies
- [ ] Rollback on failure (transaction)

### 5. End-to-End Tests

Full workflow:
- [ ] Start with empty DB
- [ ] Define schema in Rust
- [ ] Generate migration
- [ ] Run migration
- [ ] Introspect DB
- [ ] Verify matches Rust schema
- [ ] Modify Rust schema
- [ ] Generate new migration
- [ ] Run migration
- [ ] Verify again

## Test Utilities

Create helpers in `tests/common/mod.rs`:

```rust
pub async fn setup_db() -> (Container<Postgres>, Client);
pub async fn run_sql(client: &Client, sql: &str);
pub async fn introspect(client: &Client) -> Schema;
pub fn assert_schema_eq(a: &Schema, b: &Schema);
```

## CI Considerations

- Tests need Docker
- May be slow (container startup)
- Consider caching Postgres image
- Run in parallel where possible (separate containers)
