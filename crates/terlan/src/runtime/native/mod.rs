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
//! are implemented as Rust resources. NativeBoundary remains the bridge and safety
//! contract layer; this module owns concrete storage and target-native logic.

pub mod base64;
pub mod hash;
pub mod http;
pub mod json;
pub mod md5;
pub mod path;
pub mod platform;
pub mod postgres;
pub mod random;
pub mod regex;
pub mod toml;
pub mod uri;
pub mod vector;

#[cfg(test)]
#[path = "postgres_test.rs"]
#[cfg(test)]
mod postgres_test;

#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

#[cfg(test)]
#[path = "platform_test.rs"]
mod platform_test;
