//! Opaque cookie identities; actor storage and lifecycle are separate concerns.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use terlan_runtime_abi::NativeAdapterError;

const IDENTITY_BYTES: usize = 32;
const MAX_ATTEMPTS: usize = 16;

/// Issues an OS-random identity distinct from the supplied and occupied identities.
/// The caller must retain exclusive registry access until insertion.
pub fn issue(
    excluded: &str,
    mut occupied: impl FnMut(&str) -> bool,
) -> Result<String, NativeAdapterError> {
    issue_with(excluded, &mut occupied, |bytes| {
        getrandom::fill(bytes)
            .map_err(|error| NativeAdapterError::new("http.session.entropy", error.to_string(), 0))
    })
}

fn issue_with(
    excluded: &str,
    occupied: &mut impl FnMut(&str) -> bool,
    mut fill: impl FnMut(&mut [u8]) -> Result<(), NativeAdapterError>,
) -> Result<String, NativeAdapterError> {
    for _ in 0..MAX_ATTEMPTS {
        let mut bytes = [0; IDENTITY_BYTES];
        fill(&mut bytes)?;
        let identity = URL_SAFE_NO_PAD.encode(bytes);
        if identity != excluded && !occupied(&identity) {
            return Ok(identity);
        }
    }
    Err(NativeAdapterError::new(
        "http.session.identity_collision",
        "could not allocate a distinct session identity",
        0,
    ))
}

#[cfg(test)]
#[path = "session_identity_test.rs"]
mod tests;
