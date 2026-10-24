//! Native package composition shared by runtime consumers without compiler dependencies.

#![forbid(unsafe_code)]

#[path = "native/packages.rs"]
mod packages;
#[path = "db/native_operations.rs"]
pub mod postgres;

pub use packages::{
    context_operation_arity, resource_operation, value_binding, value_operation_requires_worker,
    RESOURCE_OPERATIONS, VALUE_BINDINGS, WORKER_VALUE_OPERATIONS,
};

/// Cryptographic operations shared with host package tooling.
pub use terlan_crypto_native as crypto;
