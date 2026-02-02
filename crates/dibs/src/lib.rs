//! Postgres toolkit for Rust, powered by facet reflection.
//!
//! This crate provides:
//! - Database migrations as Rust functions
//! - Schema introspection via facet reflection
//! - Query building (planned)
//!
//! # Naming Convention
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
//!
//! # Migrations
//!
//! Migrations are registered using the `#[dibs::migration]` attribute.
//! The version is automatically derived from the filename:
//!
//! ```ignore
//! // In file: src/migrations/m_2026_01_17_120000_create_user.rs
//! #[dibs::migration]
//! async fn migrate(ctx: &mut MigrationContext) -> MigrationResult<()> {
//!     ctx.execute("CREATE TABLE user (id SERIAL PRIMARY KEY, name TEXT NOT NULL)").await?;
//!     Ok(())
//! }
//! ```
//!
//! Use `MigrationResult` instead of `Result` to enable `#[track_caller]` - when an
//! error occurs, the exact source location (file:line:column) is captured.
//!
//! Run migrations with `MigrationRunner`:
//!
//! ```ignore
//! let runner = MigrationRunner::new(&client);
//! runner.migrate().await?;
//! ```

use std::future::Future;
use std::pin::Pin;

// TODO: clean up public interface
pub mod backoffice;
mod diff;
mod error;
mod introspect;
mod jsonb;
pub mod meta;
mod migrate;
mod plugin;
pub mod pool;
pub mod query;
pub mod schema;
pub mod service;
pub mod solver;
mod traced;

pub use backoffice::SquelServiceImpl;
pub use diff::{Change, SchemaDiff, TableDiff};
pub use error::{Error, MigrationError, SqlErrorContext};
pub use jsonb::Jsonb;
pub use meta::{create_meta_tables_sql, record_migration_sql, sync_tables_sql};
pub use migrate::{
    AppliedMigration, Migration, MigrationContext, MigrationRunner, MigrationStatus, RanMigration,
};
pub use pool::ConnectionProvider;
pub use service::{DibsServiceImpl, run_service};
pub use traced::{Connection, ConnectionExt, TracedConn, TracedObject, TracedPool};

// Re-export attr grammar
pub use dibs_db_schema::{__attr, __parse_attr, Attr};

// Re-export proto types for convenience
pub use dibs_proto::*;

// Re-export inventory for the proc macro
pub use inventory;

// Re-export the proc macro
pub use dibs_macros::migration;

/// Derive migration version from filename.
///
/// This is used internally by the `#[dibs::migration]` macro to derive the
/// version from the filename when no explicit version is provided.
///
/// Converts `m_2026_01_18_173711_create_users.rs` to `2026_01_18_173711-create_users`.
#[doc(hidden)]
pub const fn __derive_migration_version(filename: &str) -> &str {
    // Strip .rs extension
    let bytes = filename.as_bytes();
    let len = bytes.len();

    // Find where .rs starts (should be at len - 3)
    let without_ext_len =
        if len > 3 && bytes[len - 3] == b'.' && bytes[len - 2] == b'r' && bytes[len - 1] == b's' {
            len - 3
        } else {
            len
        };

    // Strip leading "m_" if present
    let (start, version_len) = if without_ext_len > 2 && bytes[0] == b'm' && bytes[1] == b'_' {
        (2, without_ext_len - 2)
    } else {
        (0, without_ext_len)
    };

    // SAFETY: we're slicing at valid UTF-8 boundaries (ASCII characters)
    unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            bytes.as_ptr().add(start),
            version_len,
        ))
    }
}

/// Result type for dibs operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Result type for migration functions, captures caller location on error.
pub type MigrationResult<T> = std::result::Result<T, MigrationError>;

/// Type alias for migration functions.
///
/// Migration functions are async functions that take a mutable reference to a
/// `MigrationContext` and return a `MigrationResult<()>`. Using `MigrationResult`
/// instead of `Result` enables `#[track_caller]` to capture the exact source
/// location where an error occurs (via the `?` operator).
pub type MigrationFn = for<'a> fn(
    &'a mut MigrationContext<'a>,
)
    -> Pin<Box<dyn Future<Output = MigrationResult<()>> + Send + 'a>>;

// Register Migration with inventory
inventory::collect!(Migration);

/// Generate query code from a `.styx` file.
///
/// This is the main entry point for build scripts that generate query code.
/// It collects the schema from inventory, parses the query file, generates
/// Rust code, and writes it to `OUT_DIR`.
///
/// # Example
///
/// ```ignore
/// // build.rs
/// fn main() {
///     // Force the linker to include the db crate's inventory submissions
///     my_db::ensure_linked();
///
///     dibs::build_queries(".dibs-queries/queries.styx");
/// }
/// ```
///
/// # Panics
///
/// Panics if the query file cannot be read or parsed, or if the output cannot be written.
pub fn build_queries(queries_path: impl AsRef<std::path::Path>) {
    let queries_path = queries_path.as_ref();

    println!("cargo::rerun-if-changed={}", queries_path.display());

    // Collect schema from registered tables via inventory
    let dibs_schema = Schema::collect();

    eprintln!(
        "cargo::warning=dibs: found {} tables in schema",
        dibs_schema.tables.len()
    );

    for table in &dibs_schema.tables {
        eprintln!(
            "cargo::warning=dibs: table '{}' with {} columns, {} FKs",
            table.name,
            table.columns.len(),
            table.foreign_keys.len()
        );
    }

    let (schema, planner_schema) = dibs_schema.to_query_schema();

    let source = std::fs::read_to_string(queries_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", queries_path.display(), e));

    let filename = queries_path.display().to_string();
    let file = parse_query_file(&filename, &source).unwrap();

    let generated = generate_rust_code_with_planner(&file, &schema, Some(&planner_schema));

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = std::path::Path::new(&out_dir).join("queries.rs");

    std::fs::write(&dest_path, &generated.code)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", dest_path.display(), e));

    println!("cargo::rustc-env=QUERIES_PATH={}", dest_path.display());
}
