//! Deterministic typed-program generation contracts used by `terlc fuzz`.

mod cli;
mod generate;
mod model;

pub use cli::{parse, Action};
pub use generate::{generate, GenerationConfig};
pub use model::{
    CaseManifest, GeneratedCase, Oracle, RunOptions, TypedValue, CASE_SCHEMA, VALUE_SCHEMA,
};
