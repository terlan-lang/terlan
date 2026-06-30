//! Runtime adapters and bridge implementations owned by `terlc`.
//!
//! Inputs:
//! - Backend-emitted runtime requests and native adapter operations.
//!
//! Outputs:
//! - SafeNative bridge responses and concrete Rust-native adapter behavior.
//!
//! Transformation:
//! - Keeps safety contracts separate from concrete native implementations
//!   while both remain inside the single shipped compiler crate.

pub mod native;
pub mod safenative;
pub mod vm;
