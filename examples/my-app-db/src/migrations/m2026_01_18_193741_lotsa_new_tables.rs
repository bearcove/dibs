//! Migration: lotsa-new-tables
//! Created: 2026-01-18 19:37:41 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    // Table: categories
    ctx.execute(
        r#"
CREATE TABLE categories (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    parent_id BIGINT,
    sort_order INTEGER NOT NULL DEFAULT 0
)
"#,
    )
    .await?;
    // Table: comments
    ctx.execute(
        r#"
CREATE TABLE comments (
    id BIGINT PRIMARY KEY,
    post_id BIGINT NOT NULL,
    author_id BIGINT NOT NULL,
    parent_id BIGINT,
    body TEXT NOT NULL,
    is_approved BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ
)
"#,
    )
    .await?;
    // Table: post_likes
    ctx.execute(
        r#"
CREATE TABLE post_likes (
    user_id BIGINT NOT NULL,
    post_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, post_id)
)
"#,
    )
    .await?;
    // Table: post_tags
    ctx.execute(
        r#"
CREATE TABLE post_tags (
    post_id BIGINT NOT NULL,
    tag_id BIGINT NOT NULL,
    PRIMARY KEY (post_id, tag_id)
)
"#,
    )
    .await?;
    // Table: posts
    ctx.execute("ALTER TABLE posts ADD COLUMN category_id BIGINT")
        .await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN slug TEXT NOT NULL")
        .await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN excerpt TEXT")
        .await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN featured_image_url TEXT")
        .await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN published_at TIMESTAMPTZ")
        .await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN view_count BIGINT NOT NULL DEFAULT 0")
        .await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now()")
        .await?;
    ctx.execute("ALTER TABLE posts ADD CONSTRAINT posts_author_id_fkey FOREIGN KEY (author_id) REFERENCES users (id)").await?;
    ctx.execute("ALTER TABLE posts ADD CONSTRAINT posts_category_id_fkey FOREIGN KEY (category_id) REFERENCES categories (id)").await?;
    // Table: tags
    ctx.execute(
        r#"
CREATE TABLE tags (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    color TEXT
)
"#,
    )
    .await?;
    // Table: user_follows
    ctx.execute(
        r#"
CREATE TABLE user_follows (
    follower_id BIGINT NOT NULL,
    following_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (follower_id, following_id)
)
"#,
    )
    .await?;
    // Table: users
    ctx.execute("ALTER TABLE users ADD COLUMN avatar_url TEXT")
        .await?;
    ctx.execute("ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT false")
        .await?;
    ctx.execute("ALTER TABLE users ADD COLUMN last_login_at TIMESTAMPTZ")
        .await?;

    Ok(())
}
