//! Migration: schema-update
//! Created: 2026-01-19 13:25:30 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    ctx.execute("ALTER TABLE \"categories\" RENAME TO \"category\"")
        .await?;
    ctx.execute("ALTER TABLE \"comments\" RENAME TO \"comment\"")
        .await?;
    ctx.execute("ALTER TABLE \"posts\" RENAME TO \"post\"")
        .await?;
    ctx.execute("ALTER TABLE \"post_likes\" RENAME TO \"post_like\"")
        .await?;
    ctx.execute("ALTER TABLE \"post_tags\" RENAME TO \"post_tag\"")
        .await?;
    ctx.execute("ALTER TABLE \"tags\" RENAME TO \"tag\"")
        .await?;
    ctx.execute("ALTER TABLE \"users\" RENAME TO \"user\"")
        .await?;
    ctx.execute("ALTER TABLE \"user_follows\" RENAME TO \"user_follow\"")
        .await?;

    Ok(())
}
