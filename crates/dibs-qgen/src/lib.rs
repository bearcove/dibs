//! Query DSL code generator for dibs.
//!
//! Parses `.styx` query files and generates Rust code + SQL.

// Error types
mod error;
pub use error::{QueryGenError, QueryGenErrorKind};

// Happy types;
pub use dibs_query_schema::*;

// Query planner
mod planner;
pub use planner::QueryPlan;

// SQL code generation
mod sqlgen;

// Rust code generation
mod rustgen;

// Internal stuff
mod filter_spec;
