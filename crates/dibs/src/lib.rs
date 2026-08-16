//! Postgres toolkit for Rust, powered by facet reflection.
//!
//! This crate provides:
//! - Database migrations as Rust functions
//! - Schema introspection via facet reflection
// `error::Error` is a large thiserror variant; query execution closures
// return `Result<_, Error>` and trip clippy's `result_large_err` under
// newer toolchains. Boxing the error would propagate through every
// call site for marginal gain; accept the size.
#![allow(clippy::result_large_err)]
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
//! let runner = MigrationRunner::new(&mut client);
//! runner.migrate().await?;
//! ```

use std::future::Future;
use std::pin::Pin;

// TODO: clean up public interface
pub mod backoffice;
pub mod diff;
mod error;
pub mod introspect;
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
pub use dibs_jsonb::Jsonb;
pub use dibs_pg_catalog;
pub use diff::{Change, SchemaDiff, TableDiff};
pub use error::{Error, MigrationError, SqlErrorContext};
pub use meta::{create_meta_tables_sql, record_migration_sql, sync_tables_sql};
pub use migrate::{
    AppliedMigration, Migration, MigrationContext, MigrationRunner, MigrationStatus, RanMigration,
};
pub use pool::ConnectionProvider;
pub use service::{DibsServiceImpl, serve, serve_listener};
pub use traced::{Connection, ConnectionExt, TracedConn, TracedObject, TracedPool};

// Re-export schema types from dibs_db_schema
pub use dibs_db_schema::{
    __attr, __parse_attr, Attr, Check, CheckConstraint, Column, CompositeIndex, CompositeUnique,
    ForeignKey, Index, IndexColumn, NullsOrder, PgType, Schema, SortOrder, SourceLocation, Table,
    TableDef, TriggerCheck, TriggerCheckConstraint,
};

// Re-export proto types for convenience
pub use dibs_proto::*;

// Re-export inventory for the proc macro
pub use inventory;

// Re-export the proc macro
pub use dibs_macros::migration;

// Re-export query DSL codegen types
pub use dibs_qgen::{GeneratedCode, QueryFile, generate_rust_code, parse_query_file};
pub use dibs_qgen::{compile_query_source, generate_compiled_rust};

/// Quote a PostgreSQL identifier.
///
/// Always quotes identifiers to avoid issues with reserved keywords like
/// `user`, `order`, `table`, `group`, etc. Doubles any embedded quotes.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Generate a standard index name for a table and columns.
///
/// Uses the convention `idx_{table}_{columns}` where columns are joined by underscore.
pub fn index_name(table: &str, columns: &[impl AsRef<str>]) -> String {
    let cols: Vec<&str> = columns.iter().map(|c| c.as_ref()).collect();
    format!("idx_{}_{}", table, cols.join("_"))
}

/// Generate a standard unique index name for a table and columns.
///
/// Uses the convention `uq_{table}_{columns}` where columns are joined by underscore.
pub fn unique_index_name(table: &str, columns: &[impl AsRef<str>]) -> String {
    let cols: Vec<&str> = columns.iter().map(|c| c.as_ref()).collect();
    format!("uq_{}_{}", table, cols.join("_"))
}

/// Generate a deterministic CHECK constraint name for a table and expression.
///
/// Constraint names must be unique within a schema, so we include the table name
/// and a stable hash of the expression (after whitespace normalization).
pub fn check_constraint_name(table: &str, expr: &str) -> String {
    let normalized = normalize_sql_expr_for_hash(expr);
    let hex = blake3::hash(normalized.as_bytes()).to_hex().to_string();
    let suffix = &hex[..16];

    const PG_IDENT_MAX: usize = 63;
    let prefix_overhead = "ck__".len(); // "ck_" + "_" between table and suffix
    let suffix_len = suffix.len();
    let max_table_len = PG_IDENT_MAX.saturating_sub(prefix_overhead + suffix_len);

    let table_part = if table.len() <= max_table_len {
        table
    } else {
        let mut len = max_table_len.min(table.len());
        while len > 0 && !table.is_char_boundary(len) {
            len -= 1;
        }
        &table[..len]
    };

    format!("ck_{}_{}", table_part, suffix)
}

/// Generate a deterministic trigger name for a trigger-enforced check.
///
/// Trigger names are scoped to a table in Postgres, but we still include the table name
/// and a stable hash of the expression for readability and determinism.
pub fn trigger_check_name(table: &str, expr: &str) -> String {
    let normalized = normalize_sql_expr_for_hash(expr);
    let hex = blake3::hash(normalized.as_bytes()).to_hex().to_string();
    let suffix = &hex[..16];

    const PG_IDENT_MAX: usize = 63;
    let prefix_overhead = "trgck__".len(); // "trgck_" + "_" between table and suffix
    let suffix_len = suffix.len();
    let max_table_len = PG_IDENT_MAX.saturating_sub(prefix_overhead + suffix_len);

    let table_part = if table.len() <= max_table_len {
        table
    } else {
        let mut len = max_table_len.min(table.len());
        while len > 0 && !table.is_char_boundary(len) {
            len -= 1;
        }
        &table[..len]
    };

    format!("trgck_{}_{}", table_part, suffix)
}

/// Derive the trigger function name for a trigger-enforced check.
///
/// The function name is derived from the trigger name (hashed) so we don't
/// accidentally exceed Postgres' identifier length limit.
pub fn trigger_check_function_name(trigger_name: &str) -> String {
    let hex = blake3::hash(trigger_name.as_bytes()).to_hex().to_string();
    format!("trgfn_{}", &hex[..20])
}

fn normalize_sql_expr_for_hash(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut pending_space = false;

    let mut in_single_quote = false;
    let mut in_double_quote = false;

    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_single_quote {
            out.push(ch);
            if ch == '\'' {
                if matches!(chars.peek(), Some('\'')) {
                    out.push(chars.next().expect("peeked"));
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }

        if in_double_quote {
            out.push(ch);
            if ch == '"' {
                if matches!(chars.peek(), Some('"')) {
                    out.push(chars.next().expect("peeked"));
                } else {
                    in_double_quote = false;
                }
            }
            continue;
        }

        match ch {
            '\'' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push('\'');
                in_single_quote = true;
            }
            '"' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push('"');
                in_double_quote = true;
            }
            c if c.is_whitespace() => {
                pending_space = true;
            }
            c => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(c);
            }
        }
    }

    out.trim().to_string()
}

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
    let dibs_schema = schema::collect_schema();

    eprintln!(
        "cargo::warning=dibs: found {} tables in schema",
        dibs_schema.tables.len()
    );

    for table in dibs_schema.tables.values() {
        eprintln!(
            "cargo::warning=dibs: table '{}' with {} columns, {} FKs",
            table.name,
            table.columns.len(),
            table.foreign_keys.len()
        );
    }

    let source = std::fs::read_to_string(queries_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", queries_path.display(), e));

    let filename = camino::Utf8Path::new(queries_path.to_str().expect("path must be UTF-8"));
    let (file, qsource) = parse_query_file(filename, &source).unwrap();

    let generated =
        generate_rust_code(&file, &dibs_schema, qsource).expect("query code generation failed");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = std::path::Path::new(&out_dir).join("queries.rs");

    write_if_changed(&dest_path, generated.code.as_bytes())
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", dest_path.display(), e));

    println!("cargo::rustc-env=QUERIES_PATH={}", dest_path.display());
}

/// Compile a `.dibs` source file and generate its Rust execution API.
///
/// The application schema is collected from linked Dibs table inventory and
/// converted to a PostgreSQL 18 catalog snapshot before compilation.
pub fn build_compiled_queries(queries_path: impl AsRef<std::path::Path>) {
    build_compiled_queries_with_catalog(queries_path, |_| Ok(()));
}

/// Compile a `.dibs` source file after applying application-owned catalog registrations.
///
/// The callback receives the PostgreSQL 18 catalog after linked Dibs tables have been added.
/// Applications use it for exact scalar or table-function signatures owned by their migrations.
#[derive(Clone, Copy)]
enum BuildPhase {
    SchemaInventory,
    Catalog,
    SourceRead,
    ParserAdmission,
    DeclarationSplit,
    QueryCompilation,
    RustGeneration,
    Combination,
    OutputWrite,
}

impl BuildPhase {
    fn name(self) -> &'static str {
        match self {
            Self::SchemaInventory => "schema_inventory",
            Self::Catalog => "catalog",
            Self::SourceRead => "source_read",
            Self::ParserAdmission => "parser_admission",
            Self::DeclarationSplit => "declaration_split",
            Self::QueryCompilation => "query_compilation",
            Self::RustGeneration => "rust_generation",
            Self::Combination => "combination",
            Self::OutputWrite => "output_write",
        }
    }
}

struct BuildTimings {
    enabled: bool,
}

impl BuildTimings {
    fn from_env() -> Self {
        Self {
            enabled: std::env::var_os("DIBS_BUILD_TIMINGS").is_some(),
        }
    }

    fn measure<T>(&self, phase: BuildPhase, operation: impl FnOnce() -> T) -> T {
        let started = self.enabled.then(std::time::Instant::now);
        let result = operation();
        if let Some(started) = started {
            self.report(phase, started.elapsed());
        }
        result
    }

    fn measure_into<T>(
        &self,
        elapsed: &mut std::time::Duration,
        operation: impl FnOnce() -> T,
    ) -> T {
        let started = self.enabled.then(std::time::Instant::now);
        let result = operation();
        if let Some(started) = started {
            *elapsed += started.elapsed();
        }
        result
    }

    fn report(&self, phase: BuildPhase, elapsed: std::time::Duration) {
        if self.enabled {
            println!(
                "cargo::warning=dibs-build-phase phase={} elapsed_us={}",
                phase.name(),
                elapsed.as_micros()
            );
        }
    }
}

pub fn build_compiled_queries_with_catalog(
    queries_path: impl AsRef<std::path::Path>,
    configure: impl FnOnce(
        &mut dibs_pg_catalog::CatalogSnapshot,
    ) -> std::result::Result<(), dibs_pg_catalog::CatalogError>,
) {
    let queries_path = queries_path.as_ref();
    println!("cargo::rerun-if-changed={}", queries_path.display());
    println!("cargo::rerun-if-env-changed=DIBS_BUILD_TIMINGS");
    let timings = BuildTimings::from_env();

    let schema = timings.measure(BuildPhase::SchemaInventory, schema::collect_schema);
    let catalog = timings.measure(BuildPhase::Catalog, || {
        let mut catalog = dibs_pg_catalog::CatalogSnapshot::from_schema_postgres_18(&schema)
            .expect("build PostgreSQL 18 query catalog");
        configure(&mut catalog).expect("configure application query catalog");
        catalog
    });
    let source = timings.measure(BuildPhase::SourceRead, || {
        std::fs::read_to_string(queries_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", queries_path.display()))
    });
    let parser = timings.measure(
        BuildPhase::ParserAdmission,
        dibs_query_syntax::DibsParser::new,
    );
    let declarations = timings.measure(BuildPhase::DeclarationSplit, || {
        dibs_query_syntax::declaration_sources(&source)
            .expect("split complete Dibs query declarations")
    });
    let mut compilation_elapsed = std::time::Duration::ZERO;
    let mut generation_elapsed = std::time::Duration::ZERO;
    let mut generated_queries = Vec::with_capacity(declarations.len());
    for (index, declaration) in declarations.into_iter().enumerate() {
        let mut compiled = timings.measure_into(&mut compilation_elapsed, || {
            dibs_qgen::compile_query_source(
                &parser,
                dibs_query_syntax::SourceId::new(index as u32 + 1),
                declaration,
                &catalog,
            )
            .unwrap_or_else(|diagnostics| panic!("query compilation failed: {diagnostics:#?}"))
        });
        let query = compiled
            .pop()
            .expect("one compiled query per declaration source");
        assert!(compiled.is_empty(), "one query per declaration source");
        let generated = timings.measure_into(&mut generation_elapsed, || {
            dibs_qgen::generate_compiled_rust(&query)
                .unwrap_or_else(|error| panic!("generate {}: {error}", query.query_name))
        });
        generated_queries.push((query.query_name, generated.source));
    }
    timings.report(BuildPhase::QueryCompilation, compilation_elapsed);
    timings.report(BuildPhase::RustGeneration, generation_elapsed);
    let generated = timings.measure(BuildPhase::Combination, || {
        combine_generated_queries(
            generated_queries
                .iter()
                .map(|(name, source)| (name.as_str(), source.as_str())),
        )
    });

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let destination = std::path::Path::new(&out_dir).join("queries.rs");
    let changed = timings.measure(BuildPhase::OutputWrite, || {
        write_if_changed(&destination, generated.as_bytes())
            .unwrap_or_else(|error| panic!("write {}: {error}", destination.display()))
    });
    if timings.enabled {
        println!(
            "cargo::warning=dibs-build-output changed={changed} bytes={}",
            generated.len()
        );
    }
    println!("cargo::rustc-env=QUERIES_PATH={}", destination.display());
}

fn write_if_changed(path: &std::path::Path, contents: &[u8]) -> std::io::Result<bool> {
    match std::fs::read(path) {
        Ok(existing) if existing == contents => Ok(false),
        Ok(_) => {
            std::fs::write(path, contents)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::write(path, contents)?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

fn combine_generated_queries<'a>(queries: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut combined = String::new();
    for (index, (query_name, source)) in queries.into_iter().enumerate() {
        let module_name = format!("query_{index}_{}", snake_case(query_name));
        combined.push_str("mod ");
        combined.push_str(&module_name);
        combined.push_str(" {\n");
        for line in source.lines() {
            combined.push_str("    ");
            combined.push_str(line);
            combined.push('\n');
        }
        combined.push_str("}\n");
        combined.push_str("pub use ");
        combined.push_str(&module_name);
        combined.push_str("::*;\n\n");
    }
    combined
}

fn snake_case(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod compiled_query_build_tests {
    #[test]
    fn generated_queries_are_isolated_before_root_reexport() {
        let generated = super::combine_generated_queries([
            (
                "Alpha",
                "use dibs_runtime::prelude::*;\npub async fn alpha() {}",
            ),
            (
                "Beta",
                "use dibs_runtime::prelude::*;\npub async fn beta() {}",
            ),
        ]);

        assert!(generated.contains("mod query_0_alpha {"));
        assert!(generated.contains("mod query_1_beta {"));
        assert!(generated.contains("pub use query_0_alpha::*;"));
        assert!(generated.contains("pub use query_1_beta::*;"));
    }

    #[test]
    fn generated_output_is_not_rewritten_when_bytes_match() {
        let path = temporary_output_path("unchanged");
        std::fs::write(&path, b"same").unwrap();

        assert!(!super::write_if_changed(&path, b"same").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"same");

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn generated_output_is_rewritten_when_bytes_change() {
        let path = temporary_output_path("changed");
        std::fs::write(&path, b"before").unwrap();

        assert!(super::write_if_changed(&path, b"after").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"after");

        std::fs::remove_file(path).unwrap();
    }

    fn temporary_output_path(case: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dibs-{case}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    #[test]
    fn build_phase_names_are_stable() {
        assert_eq!(
            super::BuildPhase::SchemaInventory.name(),
            "schema_inventory"
        );
        assert_eq!(super::BuildPhase::Catalog.name(), "catalog");
        assert_eq!(super::BuildPhase::SourceRead.name(), "source_read");
        assert_eq!(
            super::BuildPhase::ParserAdmission.name(),
            "parser_admission"
        );
        assert_eq!(
            super::BuildPhase::DeclarationSplit.name(),
            "declaration_split"
        );
        assert_eq!(
            super::BuildPhase::QueryCompilation.name(),
            "query_compilation"
        );
        assert_eq!(super::BuildPhase::RustGeneration.name(), "rust_generation");
        assert_eq!(super::BuildPhase::Combination.name(), "combination");
        assert_eq!(super::BuildPhase::OutputWrite.name(), "output_write");
    }
}
