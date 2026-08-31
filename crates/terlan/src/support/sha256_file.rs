use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{BoundaryError, ErrorDomain};

/// Computes a file SHA-256 without depending on a platform hash utility.
///
/// Inputs:
/// - `path`: existing file path to hash.
///
/// Output:
/// - `Ok(String)` with lowercase hex SHA-256.
/// - `Err(BoundaryError)` when the file cannot be opened or read.
///
/// Transformation:
/// - Streams the file through the linked SHA-256 implementation and renders
///   the digest as lowercase hexadecimal.
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
