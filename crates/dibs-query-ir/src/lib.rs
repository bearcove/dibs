#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Immutable resolved, typed, referenced, source-mapped, and identity-bearing Dibs query artifacts.

mod cardinality;
mod compiled;
mod hir;
mod id;
mod identity;
mod manifest;
mod reference;
mod source_map;
mod typed;

pub use cardinality::*;
pub use compiled::*;
pub use hir::*;
pub use id::*;
pub use identity::*;
pub use manifest::*;
pub use reference::*;
pub use source_map::*;
pub use typed::*;

pub use dibs_query_syntax::{SourceId, SourceSpan, Span};
