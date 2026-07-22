#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::unwrap_used)]

//! NativeBoundary support logic that is small enough for proof-track validation.
//!
//! This crate is deliberately separate from generated adapter stubs. It holds
//! pure, low-level state-transition helpers that can be tested by Rust and
//! mirrored by Lean/Aeneas proof artifacts without pulling in async runtimes,
//! FFI, NIFs, or backend-specific worker code.

pub mod adapter_abi;
pub mod cancellation;
vm_capability_component! {
    pub(crate) mod capability_sandbox;
    pub(crate) mod capability_wire;
}
pub mod credit;
pub mod dispatch;
pub mod error;
pub mod handle;
pub mod metadata;
mod proof_correlation;
pub mod request;
pub mod resource;
pub mod runtime;
mod runtime_events;
pub mod term;
pub mod worker;
mod worker_report;
