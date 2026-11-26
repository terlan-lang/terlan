//! Deterministic typed-program generation contracts used by `terlc fuzz`.

mod checked;
mod cli;
mod executor;
mod generate;
mod model;

pub use cli::{apply_output_directory, parse, Action};
pub use executor::execute;
pub use generate::{generate, GenerationConfig};
pub use model::{
    CaseManifest, GeneratedCase, GeneratedEnvironment, GeneratedFile, GeneratedFixture,
    GeneratedSource, Oracle, RunOptions, TypedValue, CASE_SCHEMA, VALUE_SCHEMA,
};
