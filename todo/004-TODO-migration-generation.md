# Phase 004: Migration Generation

Generate Rust migration files from schema diffs.

## Decided

From docs, migrations are Rust files:

```rust
#[dibs::migration("2026-01-17-normalize-emails")]
async fn normalize_emails(ctx: &mut MigrationContext) -> Result<()> {
    ctx.execute("ALTER TABLE users ADD COLUMN email_normalized TEXT").await?;
    
    ctx.backfill(|tx| async move {
        tx.execute(
            "UPDATE users SET email_normalized = LOWER(TRIM(email)) 
             WHERE email_normalized IS NULL LIMIT 1000", &[]
        ).await
    }).await?;
    
    Ok(())
}
```

Version format: `YYYY-MM-DD-slug`

## Tasks

### 1. MigrationContext API

```rust
pub struct MigrationContext<'a> {
    tx: &'a Transaction<'a>,
}

impl MigrationContext<'_> {
    pub async fn execute(&self, sql: &str) -> Result<(), Error>;
    pub async fn backfill<F, Fut>(&self, f: F) -> Result<(), Error>
    where
        F: Fn(&Transaction<'_>) -> Fut,
        Fut: Future<Output = Result<u64, Error>>;
}
```

### 2. SQL generation from diff

For each `Change`, generate the DDL:
- `AddTable` → `CREATE TABLE ...`
- `AddColumn` → `ALTER TABLE ... ADD COLUMN ...`
- `DropColumn` → `ALTER TABLE ... DROP COLUMN ...`
- etc.

### 3. Migration file template

```rust
//! {slug}

use dibs::prelude::*;

#[dibs::migration("{version}")]
async fn {fn_name}(ctx: &mut MigrationContext) -> Result<()> {
    {body}
    Ok(())
}
```

### 4. Wire up `dibs generate <slug>`

```
$ dibs generate add-email-normalized
Created: migrations/2026-01-17-add-email-normalized.rs
```

### 5. Auto-update migrations/mod.rs

Include all migration modules so they get compiled and registered.

## Acceptance criteria

- [ ] `dibs generate <slug>` creates a migration file
- [ ] Generated SQL is valid Postgres
- [ ] MigrationContext provides execute/backfill
- [ ] migrations/mod.rs auto-updated
