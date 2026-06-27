#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::unwrap_used)]

//! Concrete Rust-native adapter implementations.
//!
//! This module owns native-backed behavior for standard-library adapters that
//! are implemented as Rust resources. SafeNative remains the bridge and safety
//! contract layer; this module owns concrete storage and target-native logic.

pub mod base64;
pub mod http;
pub mod json;
pub mod path;
pub mod postgres;
pub mod uri;
pub mod vector;
