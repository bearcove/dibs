# dibs

> Call dibs on your database schema.

A Postgres toolkit for Rust, powered by [facet](https://github.com/facet-rs/facet) reflection.

## Vision

**dibs** is a batteries-included Postgres library that gives you:

- **Schema definition** via Rust structs with facet attributes
- **Migrations** as Rust functions, auto-discovered at runtime
- **Schema diffing** - detect drift between your code and your database
- **Query building** - type-safe queries without a full ORM
- **Data mapping** - seamless row-to-struct conversion

All built on facet's reflection system - no code generation, no build.rs, no macros that hide what's happening.

## Example

```rust
use dibs::prelude::*;
use facet::Facet;

#[derive(Facet)]
#[facet(dibs::table = "users")]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,
    
    #[facet(dibs::unique)]
    pub email: String,
    
    pub name: String,
    
    #[facet(dibs::fkey = tenants::id)]
    pub tenant_id: i64,
    
    pub created_at: jiff::Timestamp,
}

// Queries
let user = db.find::<User>(42).await?;
let users = db.query::<User>()
    .filter(User::tenant_id.eq(5))
    .order_by(User::created_at.desc())
    .all()
    .await?;

// Mutations
db.insert(&new_user).await?;
db.update(&user).await?;
db.delete::<User>(42).await?;
```

## Migrations

Migrations are Rust functions, not SQL files. This lets you do complex data migrations with real logic:

```rust
use dibs::prelude::*;

#[dibs::migration("2026-01-17-normalize-emails")]
async fn normalize_emails(ctx: &mut MigrationContext) -> Result<()> {
    // Add column
    ctx.execute("ALTER TABLE users ADD COLUMN email_normalized TEXT").await?;
    
    // Backfill in batches (don't lock the table)
    ctx.backfill(|tx| async move {
        tx.execute(
            "UPDATE users SET email_normalized = LOWER(TRIM(email)) 
             WHERE email_normalized IS NULL 
             LIMIT 1000",
            &[]
        ).await
    }).await?;
    
    // Add constraint
    ctx.execute(
        "ALTER TABLE users ADD CONSTRAINT users_email_normalized_unique 
         UNIQUE (email_normalized)"
    ).await?;
    
    Ok(())
}
```

Run migrations:

```
$ dibs migrate
Applied 2026-01-17-normalize-emails (32ms)
```

## Schema Diffing

Compare your Rust types against the live database:

```
$ dibs diff
Changes detected:

  users:
    + email_normalized: TEXT (nullable)
    ~ name: VARCHAR(100) -> TEXT
    
  tenants:
    (no changes)
    
Run `dibs generate` to create a migration.
```

Generate a migration skeleton:

```
$ dibs generate add-email-normalized
Created: migrations/2026-01-17-add-email-normalized.rs
```

## CLI

```
dibs migrate      Run pending migrations
dibs rollback     Rollback the last migration  
dibs status       Show migration status
dibs diff         Compare schema to database
dibs generate     Generate a migration skeleton
dibs schema       Dump the current schema
```

## Non-Goals

- **Database agnosticism** - This is for Postgres. Use sqlx if you need portability.
- **Full ORM** - No lazy loading, no relation mapping magic. Just queries.
- **Hiding SQL** - You should know what queries are running.

## License

MIT OR Apache-2.0
