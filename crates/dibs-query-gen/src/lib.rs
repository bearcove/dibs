//! Query DSL code generator for dibs.
//!
//! Parses `.styx` query files and generates Rust code + SQL.

// Error types
mod error;

// Happy types;
pub use dibs_query_schema::*;

// Rust code generation
mod rustgen;

// SQL code generation
mod sqlgen;

// Happy types
mod filter_spec;

// Query planner
mod planner;
