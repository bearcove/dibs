//! Migration: more-fkeys
//! Created: 2026-01-18 20:36:59 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    // Table: categories
    ctx.execute("ALTER TABLE categories ADD CONSTRAINT categories_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES categories (id)").await?;
    // Table: comments
    ctx.execute("ALTER TABLE comments ADD CONSTRAINT comments_post_id_fkey FOREIGN KEY (post_id) REFERENCES posts (id)").await?;
    ctx.execute("ALTER TABLE comments ADD CONSTRAINT comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES users (id)").await?;
    ctx.execute("ALTER TABLE comments ADD CONSTRAINT comments_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES comments (id)").await?;
    // Table: post_likes
    ctx.execute("ALTER TABLE post_likes ADD CONSTRAINT post_likes_user_id_fkey FOREIGN KEY (user_id) REFERENCES users (id)").await?;
    ctx.execute("ALTER TABLE post_likes ADD CONSTRAINT post_likes_post_id_fkey FOREIGN KEY (post_id) REFERENCES posts (id)").await?;
    // Table: post_tags
    ctx.execute("ALTER TABLE post_tags ADD CONSTRAINT post_tags_post_id_fkey FOREIGN KEY (post_id) REFERENCES posts (id)").await?;
    ctx.execute("ALTER TABLE post_tags ADD CONSTRAINT post_tags_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES tags (id)").await?;
    // Table: posts
    ctx.execute("ALTER TABLE posts ADD CONSTRAINT posts_slug_key UNIQUE (slug)").await?;
    // Table: user_follows
    ctx.execute("ALTER TABLE user_follows ADD CONSTRAINT user_follows_follower_id_fkey FOREIGN KEY (follower_id) REFERENCES users (id)").await?;
    ctx.execute("ALTER TABLE user_follows ADD CONSTRAINT user_follows_following_id_fkey FOREIGN KEY (following_id) REFERENCES users (id)").await?;

    Ok(())
}
