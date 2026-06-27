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
pub mod syntax;
pub mod typeck;
