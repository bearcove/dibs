# dibs

A Postgres toolkit for Rust, powered by [facet](https://github.com/facet-rs/facet) reflection.

## Components

### Schema Definition

Tables are defined as Rust structs with facet attributes:

```rust
#[derive(Facet)]
#[facet(dibs::table = "user")]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::unique)]
    pub email: String,

    pub name: String,

    #[facet(dibs::fk = "profile.user_id")]
    pub profile_id: Option<i64>,
}
```

Available attributes:
- `dibs::table = "name"` — marks struct as a table
- `dibs::pk` — primary key
- `dibs::unique` — unique constraint
- `dibs::fk = "table.column"` — foreign key reference
- `dibs::not_null` — explicit NOT NULL
- `dibs::default = "expr"` — default value expression
- `dibs::column = "name"` — override column name
- `dibs::index` — create index on column
- `dibs::auto` — auto-increment

### Migrations

Migrations are async Rust functions with the `#[dibs::migration]` attribute:

```rust
#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    ctx.execute("CREATE TABLE user (id BIGSERIAL PRIMARY KEY, email TEXT UNIQUE NOT NULL)").await?;
    Ok(())
}
```

Migrations are auto-discovered at runtime via the [inventory](https://crates.io/crates/inventory) crate.

### Schema Diffing

The `diff` command compares the Rust schema (collected via facet reflection) against the live database schema (introspected via `information_schema`). It reports:
- New tables/columns to add
- Removed tables/columns to drop
- Type or constraint changes

The `generate-from-diff` command produces a migration file from the diff.

### Query Codegen

Queries are written in [Styx](https://github.com/bearcove/styx) format in `.dibs-queries/queries.styx`:

```styx
@schema {id crate:dibs-queries@1, cli dibs}

ProductByHandle @select{
    params {handle @string}
    from product
    where {handle $handle, deleted_at @null}
    first true
    select {id, handle, status, active}
}

CreateProduct @insert{
    params {handle @string, status @string}
    into product
    values {handle $handle, status $status, created_at @now}
    returning {id, handle, status}
}
```

Supported query types:
- `@query` — SELECT with WHERE, ORDER BY, LIMIT/OFFSET, DISTINCT, COUNT
- `@insert` — INSERT with RETURNING
- `@update` — UPDATE with WHERE and RETURNING
- `@upsert` — INSERT ... ON CONFLICT ... UPDATE
- `@delete` — DELETE with RETURNING
- `sql` blocks — raw SQL for complex queries

Filter operators: `@null`, `@ilike`, `@gte`, `@lte`, `@in`, `@ne`

Functions: `@now`, `@coalesce`, `@lower`, `@concat`, `@default`

Relations via `@rel` blocks for JOINs.

### LSP Extension

Dibs includes an LSP extension for Styx query files, providing:
- Completions for table and column names
- Hover information
- Diagnostics for schema mismatches
- Definition jumping

Invoked by the Styx LSP server as `dibs lsp-extension`.

## CLI

Running `dibs` without arguments launches the TUI (if stdout is a terminal).

### Commands

```
dibs                      Interactive TUI
dibs migrate              Run pending migrations
dibs status               Show migration status
dibs diff                 Compare Rust schema to database
dibs generate NAME        Create empty migration skeleton
dibs generate-from-diff NAME  Generate migration from schema diff
dibs schema               Browse schema (TUI)
dibs schema --plain       Output schema as text
dibs schema --sql         Output schema as CREATE TABLE statements
dibs lsp-extension        Run as LSP extension
```

Requires `DATABASE_URL` environment variable.

### TUI

The TUI has two tabs:

**Rust tab** — Schema browser
- Left pane: table list with expand/collapse
- Right pane: columns, types, constraints, foreign keys, indices
- Enter on source location opens in editor
- Enter on foreign key jumps to target table

**Postgres tab** — Migration management
- Shows applied/pending migrations
- Generate new migrations from diff
- Apply migrations
- Auto-rebuilds on `.rs` file changes

Navigation: `j/k` up/down, `h/l` or Tab switch panes, `gg/G` top/bottom, `^D/^U` half-page scroll, `q` quit.

## Configuration

Configuration via `.config/dibs.styx`:

```styx
@schema {id crate:dibs@1, cli dibs}

db {
    crate my-app-db
}
```

The `db.crate` field specifies which crate contains the schema and migrations.

## Crates

| Crate | Description |
|-------|-------------|
| `dibs` | Core: schema, migrations, diffing, introspection |
| `dibs-cli` | CLI binary with TUI and commands |
| `dibs-macros` | `#[dibs::migration]` proc macro |
| `dibs-qgen` | Styx query parser and Rust codegen |
| `dibs-query-schema` | Query DSL schema types |
| `dibs-proto` | RPC protocol definitions |
| `dibs-config` | Configuration types |
| `dibs-sql` | SQL generation |
| `dibs-codegen` | Internal codegen utilities |
| `dibs-runtime` | Runtime utilities |
| `facet-tokio-postgres` | Facet integration with tokio-postgres |
| `dockside` | Minimal Docker CLI for testing |

## License

MIT OR Apache-2.0
