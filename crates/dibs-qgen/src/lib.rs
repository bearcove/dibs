//! Query DSL code generator for dibs.
//!
//! Parses `.styx` query files and generates Rust code + SQL.

// Error types
mod error;
pub use error::{QError, QErrorKind, QSource};

// Happy types;
pub use dibs_query_schema::*;

// Parse
mod parse;
pub use parse::parse_query_file;

// Query planner
mod planner;
pub(crate) use planner::{QueryPlan, QueryPlanner};

// SQL code generation
mod sqlgen;

// Rust code generation
mod rustgen;

// Internal stuff
mod filter_spec;
