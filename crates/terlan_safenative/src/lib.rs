#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::unwrap_used)]

//! SafeNative support logic that is small enough for proof-track validation.
//!
//! This crate is deliberately separate from generated adapter stubs. It holds
//! pure, low-level state-transition helpers that can be tested by Rust and
//! mirrored by Lean/Aeneas proof artifacts without pulling in async runtimes,
//! FFI, NIFs, or backend-specific worker code.

pub mod base64;
pub mod credit;
pub mod dispatch;
pub mod error;
pub mod handle;
pub mod json;
pub mod path;
pub mod request;
pub mod resource;
pub mod runtime;
pub mod term;
pub mod uri;
pub mod worker;
