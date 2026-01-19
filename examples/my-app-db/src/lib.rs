//! Schema definitions for my-app.
//!
//! This crate defines the database schema using facet reflection.
//! Demonstrates various relationship types: one-to-many, many-to-many,
//! self-referencing, and composite keys.
//!
//! ## Naming Convention
//!
//! **Table names use singular form** (e.g., `user`, `post`, `comment`).
//!
//! This convention treats each table as a definition of what a single record
//! represents, rather than a container of multiple records. It reads more
//! naturally in code: `User::find(id)` returns "a user", and foreign keys
//! like `author_id` reference "the user table".
//!
//! Junction tables for many-to-many relationships use singular forms joined
//! by underscore: `post_tag`, `post_like`, `user_follow`.

mod migrations;

use facet::Facet;

/// A user in the system.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "user")]
#[facet(dibs::icon = "user")]
pub struct User {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// User's email address
    #[facet(dibs::unique, dibs::subtype = "email")]
    pub email: String,

    /// Display name
    #[facet(dibs::label, dibs::icon = "user")]
    pub name: String,

    /// Optional bio
    #[facet(dibs::lang = "markdown")]
    pub bio: Option<String>,

    /// URL to avatar image
    #[facet(dibs::subtype = "avatar")]
    pub avatar_url: Option<String>,

    /// Whether the user is an admin
    #[facet(dibs::default = "false", dibs::icon = "shield")]
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
#[facet(dibs::table = "user_follow")]
#[facet(dibs::icon = "user-plus")]
pub struct UserFollow {
    /// The user who is following
    #[facet(dibs::pk, dibs::fk = "user.id")]
    pub follower_id: i64,

    /// The user being followed
    #[facet(dibs::pk, dibs::fk = "user.id")]
    pub following_id: i64,

    /// When the follow happened
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,
}

/// Hierarchical categories for posts.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "category")]
#[facet(dibs::icon = "folder-tree")]
pub struct Category {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Category name
    #[facet(dibs::unique, dibs::label, dibs::icon = "folder")]
    pub name: String,

    /// URL-friendly slug
    #[facet(dibs::unique, dibs::subtype = "slug")]
    pub slug: String,

    /// Category description
    #[facet(dibs::lang = "markdown")]
    pub description: Option<String>,

    /// Parent category (self-referencing FK for hierarchy)
    #[facet(dibs::fk = "category.id", dibs::icon = "corner-down-right")]
    pub parent_id: Option<i64>,

    /// Display order within parent
    #[facet(dibs::default = "0", dibs::icon = "arrow-up-down")]
    pub sort_order: i32,
}

/// A blog post.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "post")]
#[facet(dibs::icon = "newspaper")]
pub struct Post {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Author of the post
    #[facet(dibs::fk = "user.id", dibs::icon = "pen-tool")]
    pub author_id: i64,

    /// Category for the post
    #[facet(dibs::fk = "category.id")]
    pub category_id: Option<i64>,

    /// Post title
    #[facet(dibs::label, dibs::icon = "heading")]
    pub title: String,

    /// URL-friendly slug
    #[facet(dibs::unique, dibs::subtype = "slug")]
    pub slug: String,

    /// Short summary/excerpt
    #[facet(dibs::long, dibs::icon = "text")]
    pub excerpt: Option<String>,

    /// Post content (markdown)
    #[facet(dibs::lang = "markdown")]
    pub body: String,

    /// Featured image URL
    #[facet(dibs::subtype = "image")]
    pub featured_image_url: Option<String>,

    /// Whether the post is published
    #[facet(dibs::default = "false", dibs::icon = "globe")]
    pub published: bool,

    /// When the post was published
    #[facet(dibs::icon = "calendar-check")]
    pub published_at: Option<jiff::Timestamp>,

    /// View count
    #[facet(dibs::default = "0", dibs::icon = "eye")]
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
#[facet(dibs::table = "tag")]
#[facet(dibs::icon = "tag")]
pub struct Tag {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Tag name
    #[facet(dibs::unique, dibs::label, dibs::subtype = "tag")]
    pub name: String,

    /// URL-friendly slug
    #[facet(dibs::unique, dibs::subtype = "slug")]
    pub slug: String,

    /// Tag color for UI (hex)
    #[facet(dibs::subtype = "color")]
    pub color: Option<String>,
}

/// Junction table for posts and tags (many-to-many).
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "post_tag")]
#[facet(dibs::icon = "tags")]
pub struct PostTag {
    /// The post
    #[facet(dibs::pk, dibs::fk = "post.id")]
    pub post_id: i64,

    /// The tag
    #[facet(dibs::pk, dibs::fk = "tag.id")]
    pub tag_id: i64,
}

/// Comments on posts.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "comment")]
#[facet(dibs::icon = "message-circle")]
pub struct Comment {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// The post being commented on
    #[facet(dibs::fk = "post.id")]
    pub post_id: i64,

    /// The user who wrote the comment
    #[facet(dibs::fk = "user.id", dibs::icon = "user")]
    pub author_id: i64,

    /// Parent comment (for threaded replies)
    #[facet(dibs::fk = "comment.id", dibs::icon = "reply")]
    pub parent_id: Option<i64>,

    /// Comment content
    #[facet(dibs::lang = "markdown")]
    pub body: String,

    /// Whether the comment is approved/visible
    #[facet(dibs::default = "true", dibs::icon = "check-circle")]
    pub is_approved: bool,

    /// When the comment was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When the comment was last edited
    #[facet(dibs::icon = "pencil")]
    pub edited_at: Option<jiff::Timestamp>,
}

/// Likes on posts (user can like a post once).
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "post_like")]
#[facet(dibs::icon = "heart")]
pub struct PostLike {
    /// The user who liked
    #[facet(dibs::pk, dibs::fk = "user.id")]
    pub user_id: i64,

    /// The post that was liked
    #[facet(dibs::pk, dibs::fk = "post.id")]
    pub post_id: i64,

    /// When the like happened
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,
}
