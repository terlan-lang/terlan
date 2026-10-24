//! Bounded OS entropy acquisition, independent of token formats and consumers.

use terlan_runtime_abi::{native_binding, BoundaryError, ErrorDomain, NativeBinding, NativeValue};

/// Returns raw OS-random bytes; no seeded fallback, encoding, or retry policy.
pub const ENTROPY_BYTES: NativeBinding =
    native_binding!("std.crypto.entropy.bytes", |count: i64| {
        bytes_with(count, getrandom::fill).map(NativeValue::Bytes)
    });

fn bytes_with(
    count: i64,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<Vec<u8>, BoundaryError> {
    if !(1..=4096).contains(&count) {
        return Err(BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "OS entropy size",
            "error[crypto.entropy.size]: entropy size must be between 1 and 4096 bytes",
        ));
    }
    let mut bytes = vec![0; count as usize];
    fill(&mut bytes).map_err(entropy_failure)?;
    Ok(bytes)
}

fn entropy_failure(error: getrandom::Error) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "OS entropy acquisition",
        format!("error[crypto.entropy.unavailable]: {error}"),
    )
}

#[cfg(test)]
#[path = "entropy_test.rs"]
mod tests;
