# Phase 005: Migration Execution

Run pending migrations and track what's been applied.

## From docs

```
$ dibs migrate
Applied 2026-01-17-add-email-normalized (32ms)
```

## Tasks

### 1. Migrations table

```sql
CREATE TABLE IF NOT EXISTS _dibs_migrations (
    version TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 2. Migration registration via inventory

```rust
pub struct MigrationDef {
    pub version: &'static str,
    pub run: fn(&mut MigrationContext) -> Pin<Box<dyn Future<Output = Result<()>>>>,
}

inventory::collect!(MigrationDef);
```

The `#[dibs::migration]` macro registers each migration.

### 3. Get pending migrations

```rust
pub async fn pending_migrations(client: &Client) -> Result<Vec<&'static MigrationDef>> {
    let applied: HashSet<String> = /* query _dibs_migrations */;
    
    inventory::iter::<MigrationDef>()
        .filter(|m| !applied.contains(m.version))
        .sorted_by_key(|m| m.version)
        .collect()
}
```

### 4. Run migrations in transaction

```rust
pub async fn migrate(client: &mut Client) -> Result<()> {
    for migration in pending_migrations(client).await? {
        let tx = client.transaction().await?;
        let mut ctx = MigrationContext::new(&tx);
        
        (migration.run)(&mut ctx).await?;
        
        tx.execute(
            "INSERT INTO _dibs_migrations (version) VALUES ($1)",
            &[&migration.version]
        ).await?;
        
        tx.commit().await?;
        println!("Applied {}", migration.version);
    }
    Ok(())
}
```

### 5. Wire up `dibs migrate`

Run all pending migrations.

### 6. Wire up `dibs status`

Show applied vs pending migrations.

## Acceptance criteria

- [ ] `_dibs_migrations` table created automatically
- [ ] Migrations registered via `#[dibs::migration]`
- [ ] `dibs migrate` runs pending migrations
- [ ] Each migration runs in a transaction
- [ ] `dibs status` shows migration status
