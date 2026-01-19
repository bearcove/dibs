//! Migration: create-tables
//! Created: 2026-01-18 18:18:21 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    // Table: posts
    ctx.execute(
        r#"
CREATE TABLE posts (
    id BIGINT PRIMARY KEY,
    author_id BIGINT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
)
"#,
    )
    .await?;
    // Table: users
    ctx.execute(
        r#"
CREATE TABLE users (
    id BIGINT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    bio TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
)
"#,
    )
    .await?;

    Ok(())
}
