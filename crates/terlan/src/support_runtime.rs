//! Runtime-only support surface for the compiler-free serve binary.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[path = "support/boundary_error.rs"]
pub mod boundary_error;

pub use boundary_error::{BoundaryError, ErrorDomain};

#[path = "support/sha256_file.rs"]
mod sha256_file;
pub(crate) use sha256_file::sha256sum_file;

/// Computes a deterministic process-independent content fingerprint.
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
