//! Backends consuming completed, checked query artifacts.

mod rust;
pub(crate) mod sql;

pub use rust::{GeneratedRust, RustGenerationError, generate_compiled_rust};
pub use sql::{RenderedSql, SqlRenderError, render_compiled_sql};
