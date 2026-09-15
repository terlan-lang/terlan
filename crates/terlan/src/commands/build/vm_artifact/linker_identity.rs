//! Shared native-link environment identity for inner and bootstrap caches.

use std::collections::BTreeMap;

use super::native_cache;

const LINK_ENVIRONMENT: &[&str] = &[
    "TERLAN_NATIVE_LINKER",
    "PATH",
    "RUSTC",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "CARGO_HOME",
    "SDKROOT",
    "MACOSX_DEPLOYMENT_TARGET",
    "LIBRARY_PATH",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "LDEMULATION",
    "SOURCE_DATE_EPOCH",
];

/// Hashes declared link settings without putting their values into cache reports.
pub(super) fn environment_digest() -> String {
    let values = LINK_ENVIRONMENT
        .iter()
        .map(|name| {
            (
                *name,
                std::env::var_os(name).map(|value| value.as_encoded_bytes().to_vec()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    digest_values(&values)
}

fn digest_values(values: &BTreeMap<&str, Option<Vec<u8>>>) -> String {
    let mut bytes = Vec::new();
    for (name, value) in values {
        bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(u8::from(value.is_some()));
        if let Some(value) = value {
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend_from_slice(value);
        }
    }
    native_cache::sha256_hex(&bytes)
}

#[cfg(test)]
#[path = "linker_identity_test.rs"]
mod tests;
