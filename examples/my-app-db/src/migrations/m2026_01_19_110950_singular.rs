//! Migration: singular
//! Created: 2026-01-19 11:09:50 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    // Table: category
    ctx.execute("ALTER TABLE categories RENAME TO category").await?;
    ctx.execute("ALTER TABLE category ADD CONSTRAINT category_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES category (id)").await?;
    ctx.execute("ALTER TABLE category DROP CONSTRAINT category_parent_id_fkey").await?;
    // Table: comment
    ctx.execute("ALTER TABLE comments RENAME TO comment").await?;
    ctx.execute("ALTER TABLE comment ADD CONSTRAINT comment_post_id_fkey FOREIGN KEY (post_id) REFERENCES post (id)").await?;
    ctx.execute("ALTER TABLE comment ADD CONSTRAINT comment_author_id_fkey FOREIGN KEY (author_id) REFERENCES user (id)").await?;
    ctx.execute("ALTER TABLE comment ADD CONSTRAINT comment_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES comment (id)").await?;
    ctx.execute("ALTER TABLE comment DROP CONSTRAINT comment_author_id_fkey").await?;
    ctx.execute("ALTER TABLE comment DROP CONSTRAINT comment_parent_id_fkey").await?;
    ctx.execute("ALTER TABLE comment DROP CONSTRAINT comment_post_id_fkey").await?;
    // Table: post
    ctx.execute("ALTER TABLE posts RENAME TO post").await?;
    ctx.execute("ALTER TABLE post ADD CONSTRAINT post_author_id_fkey FOREIGN KEY (author_id) REFERENCES user (id)").await?;
    ctx.execute("ALTER TABLE post ADD CONSTRAINT post_category_id_fkey FOREIGN KEY (category_id) REFERENCES category (id)").await?;
    ctx.execute("ALTER TABLE post DROP CONSTRAINT post_category_id_fkey").await?;
    ctx.execute("ALTER TABLE post DROP CONSTRAINT post_author_id_fkey").await?;
    // Table: post_like
    ctx.execute("ALTER TABLE post_likes RENAME TO post_like").await?;
    ctx.execute("ALTER TABLE post_like ADD CONSTRAINT post_like_user_id_fkey FOREIGN KEY (user_id) REFERENCES user (id)").await?;
    ctx.execute("ALTER TABLE post_like ADD CONSTRAINT post_like_post_id_fkey FOREIGN KEY (post_id) REFERENCES post (id)").await?;
    ctx.execute("ALTER TABLE post_like DROP CONSTRAINT post_like_post_id_fkey").await?;
    ctx.execute("ALTER TABLE post_like DROP CONSTRAINT post_like_user_id_fkey").await?;
    // Table: post_tag
    ctx.execute("ALTER TABLE post_tags RENAME TO post_tag").await?;
    ctx.execute("ALTER TABLE post_tag ADD CONSTRAINT post_tag_post_id_fkey FOREIGN KEY (post_id) REFERENCES post (id)").await?;
    ctx.execute("ALTER TABLE post_tag ADD CONSTRAINT post_tag_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES tag (id)").await?;
    ctx.execute("ALTER TABLE post_tag DROP CONSTRAINT post_tag_post_id_fkey").await?;
    ctx.execute("ALTER TABLE post_tag DROP CONSTRAINT post_tag_tag_id_fkey").await?;
    // Table: tag
    ctx.execute("ALTER TABLE tags RENAME TO tag").await?;
    // Table: user
    ctx.execute("ALTER TABLE users RENAME TO user").await?;
    // Table: user_follow
    ctx.execute("ALTER TABLE user_follows RENAME TO user_follow").await?;
    ctx.execute("ALTER TABLE user_follow ADD CONSTRAINT user_follow_follower_id_fkey FOREIGN KEY (follower_id) REFERENCES user (id)").await?;
    ctx.execute("ALTER TABLE user_follow ADD CONSTRAINT user_follow_following_id_fkey FOREIGN KEY (following_id) REFERENCES user (id)").await?;
    ctx.execute("ALTER TABLE user_follow DROP CONSTRAINT user_follow_following_id_fkey").await?;
    ctx.execute("ALTER TABLE user_follow DROP CONSTRAINT user_follow_follower_id_fkey").await?;

    Ok(())
}
