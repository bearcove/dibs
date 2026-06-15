# Migrations Vision

## Crate Structure

A typical app using dibs:

```
my-app/
  crates/
    my-app/           # main app, depends on my-app-db for schema types
    my-app-db/        # lib (schema) + bin (roam service)
  dibs.toml           # config pointing dibs CLI at my-app-db
```

### my-app-db crate

**As a library**: exports schema types

```rust
// my-app-db/src/lib.rs
use facet::Facet;

#[derive(Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "users")]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,
    pub email: String,
    pub name: String,
}

// Also exports migration definitions
pub mod migrations;
```

**As a binary**: roam service for schema operations

```rust
// my-app-db/src/main.rs
fn main() {
    my_app_db::run_service();
}
```

Small binary - just roam + dibs, no heavy app dependencies.

### my-app crate

```rust
// my-app/src/main.rs
use my_app_db::{User, Post};  // import schema types

// ... axum, business logic, otel, crash reporting, the works
```

Big binary with all the app stuff. Doesn't include migration machinery.

## How dibs CLI Works

The `dibs` CLI doesn't compile in any schema. It spawns the user's db crate as needed.

```bash
dibs diff --database-url postgres://...
```

1. `dibs` reads `dibs.toml` → finds `crate = "my-app-db"`
2. Spawns `cargo run -p my-app-db` (or the built binary)
3. `my-app-db` starts, opens roam socket (TCP)
4. `dibs` connects, sends request
5. `my-app-db` processes (has schema via inventory), returns result
6. `dibs` displays result, both exit

Not a daemon - just a short-lived service that responds to queries.

## Roam API

Requests `dibs` sends to `my-app-db`:

### schema.collect
Returns the full schema (tables, columns, indices, etc.)

### schema.diff
```
{ database_url: String }
```
Connects to DB, introspects, diffs against Rust schema, returns changes.

### migrate.status
```
{ database_url: String }
```
Returns list of migrations with applied/pending status.

### migrate.run
```
{ database_url: String, migration: Option<String> }
```
Runs pending migrations (or a specific one). Streams logs back via roam-tracing.

## Migration Definitions

Migrations live in `my-app-db`:

```rust
// my-app-db/src/migrations/m20260118_add_user_bio.rs
use dibs::migration;

#[migration("20260118-add-user-bio")]
async fn add_user_bio(ctx: &mut MigrationContext) -> Result<()> {
    ctx.execute("ALTER TABLE users ADD COLUMN bio TEXT").await?;
    Ok(())
}
```

Registered via inventory, available when the binary runs.

## Deployment

In production, `my-app-db` is deployed as a separate binary:

```bash
# Run migrations before starting the app
./my-app-db migrate --database-url $DATABASE_URL

# Then start the app
./my-app
```

Or as a k8s init container, etc.

The `my-app-db` binary can also run in "direct" mode without roam for production:
```bash
my-app-db migrate --database-url ...   # direct mode
my-app-db serve                         # roam mode (for dibs CLI)
```

## Config

```toml
# dibs.toml
[db]
crate = "my-app-db"

# Optional: pre-built binary path for faster iteration
# binary = "target/debug/my-app-db"
```

## Workflow

### Development

```bash
# See what changed
dibs diff

# Generate migration from diff
dibs generate "add-user-bio"
# Creates my-app-db/src/migrations/m20260118_add_user_bio.rs

# Run migrations
dibs migrate

# Check status
dibs status
```

### CI/CD

```bash
# Build the migration binary
cargo build -p my-app-db --release

# Run migrations
./target/release/my-app-db migrate --database-url $DATABASE_URL
```

## Log Streaming

When running migrations via `dibs migrate`, logs stream back in real-time via roam-tracing:

```
$ dibs migrate
Running 20260118-add-user-bio...
  ALTER TABLE users ADD COLUMN bio TEXT
  Backfilling 150,000 rows...
  [=========>          ] 45% (67,500 / 150,000)
  Done in 3.2s
Applied 1 migration.
```

Progress bars for backfills, streaming output, the works.
