//! Runtime adapters and bridge implementations owned by `terlc`.
//!
//! Inputs:
//! - Backend-emitted runtime requests and native adapter operations.
//!
//! Outputs:
//! - NativeBoundary bridge responses and concrete Rust-native adapter behavior.
//!
//! Transformation:
//! - Keeps safety contracts separate from concrete native implementations
//!   while both remain inside the single shipped compiler crate.

#[path = "vm/map_layout.rs"]
pub(crate) mod map_layout;
pub mod native;
pub mod native_boundary;
pub mod native_image;
pub mod vm;
