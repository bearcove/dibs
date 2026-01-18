//! Schema definitions for my-app.
//!
//! This crate defines the database schema using facet reflection.
//! Demonstrates various relationship types: one-to-many, many-to-many,
//! self-referencing, and composite keys.

mod migrations;

use facet::Facet;

/// A user in the system.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "users")]
pub struct User {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// User's email address
    #[facet(dibs::unique)]
    pub email: String,

    /// Display name
    pub name: String,

    /// Optional bio
    pub bio: Option<String>,

    /// URL to avatar image
    pub avatar_url: Option<String>,

    /// Whether the user is an admin
    #[facet(dibs::default = "false")]
    pub is_admin: bool,

    /// When the user was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When the user last logged in
    pub last_login_at: Option<jiff::Timestamp>,
}

/// Users following other users (self-referencing many-to-many).
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "user_follows")]
pub struct UserFollow {
    /// The user who is following
    #[facet(dibs::pk, dibs::fk = "users.id")]
    pub follower_id: i64,

    /// The user being followed
    #[facet(dibs::pk, dibs::fk = "users.id")]
    pub following_id: i64,

    /// When the follow happened
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,
}

/// Hierarchical categories for posts.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "categories")]
pub struct Category {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Category name
    #[facet(dibs::unique)]
    pub name: String,

    /// URL-friendly slug
    #[facet(dibs::unique)]
    pub slug: String,

    /// Category description
    pub description: Option<String>,

    /// Parent category (self-referencing FK for hierarchy)
    #[facet(dibs::fk = "categories.id")]
    pub parent_id: Option<i64>,

    /// Display order within parent
    #[facet(dibs::default = "0")]
    pub sort_order: i32,
}

/// A blog post.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "posts")]
pub struct Post {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Author of the post
    #[facet(dibs::fk = "users.id")]
    pub author_id: i64,

    /// Category for the post
    #[facet(dibs::fk = "categories.id")]
    pub category_id: Option<i64>,

    /// Post title
    pub title: String,

    /// URL-friendly slug
    #[facet(dibs::unique)]
    pub slug: String,

    /// Short summary/excerpt
    pub excerpt: Option<String>,

    /// Post content (markdown)
    pub body: String,

    /// Featured image URL
    pub featured_image_url: Option<String>,

    /// Whether the post is published
    #[facet(dibs::default = "false")]
    pub published: bool,

    /// When the post was published
    pub published_at: Option<jiff::Timestamp>,

    /// View count
    #[facet(dibs::default = "0")]
    pub view_count: i64,

    /// When the post was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When the post was last updated
    #[facet(dibs::default = "now()")]
    pub updated_at: jiff::Timestamp,
}

/// Tags for categorizing posts.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "tags")]
pub struct Tag {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Tag name
    #[facet(dibs::unique)]
    pub name: String,

    /// URL-friendly slug
    #[facet(dibs::unique)]
    pub slug: String,

    /// Tag color for UI (hex)
    pub color: Option<String>,
}

/// Junction table for posts and tags (many-to-many).
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "post_tags")]
pub struct PostTag {
    /// The post
    #[facet(dibs::pk, dibs::fk = "posts.id")]
    pub post_id: i64,

    /// The tag
    #[facet(dibs::pk, dibs::fk = "tags.id")]
    pub tag_id: i64,
}

/// Comments on posts.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "comments")]
pub struct Comment {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// The post being commented on
    #[facet(dibs::fk = "posts.id")]
    pub post_id: i64,

    /// The user who wrote the comment
    #[facet(dibs::fk = "users.id")]
    pub author_id: i64,

    /// Parent comment (for threaded replies)
    #[facet(dibs::fk = "comments.id")]
    pub parent_id: Option<i64>,

    /// Comment content
    pub body: String,

    /// Whether the comment is approved/visible
    #[facet(dibs::default = "true")]
    pub is_approved: bool,

    /// When the comment was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When the comment was last edited
    pub edited_at: Option<jiff::Timestamp>,
}

/// Likes on posts (user can like a post once).
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "post_likes")]
pub struct PostLike {
    /// The user who liked
    #[facet(dibs::pk, dibs::fk = "users.id")]
    pub user_id: i64,

    /// The post that was liked
    #[facet(dibs::pk, dibs::fk = "posts.id")]
    pub post_id: i64,

    /// When the like happened
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,
}
