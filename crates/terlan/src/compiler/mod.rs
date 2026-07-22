//! Compiler front-end and type-analysis modules owned by `terlc`.
//!
//! Inputs:
//! - Terlan source text, syntax output, interfaces, and type contracts.
//!
//! Outputs:
//! - Parsed syntax trees, HIR interfaces, and typed CoreIR.
//!
//! Transformation:
//! - Groups compiler phases by responsibility inside one shipped crate.

pub mod api_contract;
pub mod hir;
pub(crate) mod native_ir;
pub(crate) mod purity;
#[cfg(test)]
#[path = "purity_test.rs"]
mod purity_test;
pub(crate) mod router;
pub mod syntax;
pub mod typeck;
pub(crate) mod value_lifecycle;
