//! Package-owned cryptography using maintained native providers.

#![forbid(unsafe_code)]

pub mod ed25519;
pub mod hash;
mod hash_bindings;

pub use hash_bindings::{
    SHA256, SHA256_BYTES, SHA256_DOMAIN_FRAMED, SHA256_FRAMED, SHA256_NUL_SEPARATED,
};

#[cfg(test)]
mod hash_test;

use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeBinding, NativeValue};

/// Exact copied-value signature verification contract.
pub const VERIFY_ED25519: NativeBinding = NativeBinding {
    operation: "std.crypto.ed25519.verify",
    arity: 3,
    invoke: |args| {
        let [NativeValue::String(public_key), NativeValue::String(payload), NativeValue::String(signature)] =
            args
        else {
            return Err(BoundaryError::message(
                ErrorDomain::NativeBoundary,
                "signature verification arguments",
                "error[dispatch.type]: std.crypto.ed25519.verify expects three strings",
            ));
        };
        Ok(NativeValue::Bool(ed25519::verify(
            public_key, payload, signature,
        )))
    },
};

#[cfg(test)]
#[path = "bindings_test.rs"]
mod tests;
