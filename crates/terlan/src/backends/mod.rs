//! Compiler backend implementations owned by the shipped `terlc` crate.
//!
//! Inputs:
//! - Core compiler lowering products and backend-specific metadata.
//!
//! Outputs:
//! - Target artifacts such as Vm source, JavaScript, WebAssembly, or
//!   runtime bridge metadata.
//!
//! Transformation:
//! - Keeps backend code grouped by target while the Cargo package remains a
//!   single shipped compiler crate.

pub(crate) mod wasm;
