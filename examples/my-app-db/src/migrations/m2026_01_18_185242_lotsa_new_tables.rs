//! Migration: lotsa-new-tables
//! Created: 2026-01-18 18:52:42 CET

use dibs::{MigrationContext, Result};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> Result<()> {
    // Table: categories
    ctx.execute(r#"
CREATE TABLE categories (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    parent_id BIGINT,
    sort_order INTEGER NOT NULL DEFAULT 0
)
"#).await?;
    // Table: comments
    ctx.execute(r#"
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
"#).await?;
    // Table: post_likes
    ctx.execute(r#"
CREATE TABLE post_likes (
    user_id BIGINT PRIMARY KEY,
    post_id BIGINT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
)
"#).await?;
    // Table: post_tags
    ctx.execute(r#"
CREATE TABLE post_tags (
    post_id BIGINT PRIMARY KEY,
    tag_id BIGINT PRIMARY KEY
)
"#).await?;
    // Table: posts
    ctx.execute("ALTER TABLE posts ADD COLUMN category_id BIGINT").await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN slug TEXT NOT NULL").await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN excerpt TEXT").await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN featured_image_url TEXT").await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN published_at TIMESTAMPTZ").await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN view_count BIGINT NOT NULL DEFAULT 0").await?;
    ctx.execute("ALTER TABLE posts ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now()").await?;
    ctx.execute("ALTER TABLE posts ADD CONSTRAINT posts_author_id_fkey FOREIGN KEY (author_id) REFERENCES users (id)").await?;
    ctx.execute("ALTER TABLE posts ADD CONSTRAINT posts_category_id_fkey FOREIGN KEY (category_id) REFERENCES categories (id)").await?;
    // Table: tags
    ctx.execute(r#"
CREATE TABLE tags (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    color TEXT
)
"#).await?;
    // Table: user_follows
    ctx.execute(r#"
CREATE TABLE user_follows (
    follower_id BIGINT PRIMARY KEY,
    following_id BIGINT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
)
"#).await?;
    // Table: users
    ctx.execute("ALTER TABLE users ADD COLUMN avatar_url TEXT").await?;
    ctx.execute("ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT false").await?;
    ctx.execute("ALTER TABLE users ADD COLUMN last_login_at TIMESTAMPTZ").await?;

    Ok(())
}
