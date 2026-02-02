//! Query DSL code generator for dibs.
//!
//! Parses `.styx` query files and generates Rust code + SQL.

mod codegen;
mod error;
mod filter_spec;
mod parse;
mod planner;
mod schema;
mod sql;
