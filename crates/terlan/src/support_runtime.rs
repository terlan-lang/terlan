//! Runtime-only support surface for the compiler-free serve binary.

use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

#[path = "support/boundary_error.rs"]
pub mod boundary_error;

pub use boundary_error::{BoundaryError, ErrorDomain};

/// Computes a deterministic process-independent content fingerprint.
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Computes the checksum used to admit persisted AOT images.
pub(crate) fn sha256sum_file(path: &Path) -> Result<String, BoundaryError> {
    let mut file = fs::File::open(path).map_err(|error| {
        BoundaryError::sourced(
            ErrorDomain::CommandExecution,
            "command.sha256.open",
            "sha256sum_file",
            format!("cannot open `{}` for SHA-256: {error}", path.display()),
            error,
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            BoundaryError::sourced(
                ErrorDomain::CommandExecution,
                "command.sha256.read",
                "sha256sum_file",
                format!("cannot read `{}` for SHA-256: {error}", path.display()),
                error,
            )
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    let mut hash = String::with_capacity(64);
    for byte in digest {
        write!(&mut hash, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(hash)
}
