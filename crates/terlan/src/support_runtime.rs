//! Runtime-only support surface for the compiler-free serve binary.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;

#[path = "support/boundary_error.rs"]
pub mod boundary_error;

pub use boundary_error::{BoundaryError, ErrorDomain};

/// Computes a deterministic process-independent content fingerprint.
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Computes the checksum used to admit persisted AOT images.
pub(crate) fn sha256sum_file(path: &Path) -> Result<String, BoundaryError> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|error| {
            BoundaryError::sourced(
                ErrorDomain::CommandExecution,
                "command.sha256.spawn",
                "sha256sum_file",
                format!("cannot run sha256sum for `{}`: {error}", path.display()),
                error,
            )
        })?;
    if !output.status.success() {
        return Err(BoundaryError::message(
            ErrorDomain::CommandExecution,
            "sha256sum_file",
            format!(
                "error[command.sha256.exit]: sha256sum failed for `{}`",
                path.display()
            ),
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        BoundaryError::sourced(
            ErrorDomain::CommandExecution,
            "command.sha256.utf8",
            "sha256sum_file",
            format!("sha256sum output was not UTF-8: {error}"),
            error,
        )
    })?;
    let hash = stdout.split_whitespace().next().ok_or_else(|| {
        BoundaryError::message(
            ErrorDomain::CommandExecution,
            "sha256sum_file",
            "error[command.sha256.empty]: sha256sum output was empty",
        )
    })?;
    if !is_valid_sha256_hex(hash) {
        return Err(BoundaryError::message(
            ErrorDomain::CommandExecution,
            "sha256sum_file",
            format!("error[command.sha256.format]: sha256sum output was not SHA-256 hex: `{hash}`"),
        ));
    }
    Ok(hash.to_string())
}
