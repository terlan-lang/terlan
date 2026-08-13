//! Rust-native SHA-256 operations for `std.crypto.Hash`.

use sha2::Digest;

/// Hashes bytes and returns lowercase hexadecimal SHA-256.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hexadecimal(sha2::Sha256::digest(bytes))
}

/// Hashes length-prefixed UTF-8 fields in caller order.
///
/// Returns `None` only on a target whose address space can represent a field
/// larger than the portable unsigned 64-bit framing contract.
pub fn sha256_framed<S: AsRef<str>>(fields: &[S]) -> Option<String> {
    let mut digest = sha2::Sha256::new();
    for field in fields {
        let bytes = field.as_ref().as_bytes();
        let length = u64::try_from(bytes.len()).ok()?;
        digest.update(length.to_be_bytes());
        digest.update(bytes);
    }
    Some(hexadecimal(digest.finalize()))
}

/// Hashes one domain plus length-prefixed UTF-8 fields.
pub fn sha256_domain_framed<S: AsRef<str>>(domain: &str, fields: &[S]) -> Option<String> {
    let mut digest = sha2::Sha256::new();
    digest.update(domain.as_bytes());
    digest.update([0]);
    for field in fields {
        let bytes = field.as_ref().as_bytes();
        let length = u64::try_from(bytes.len()).ok()?;
        digest.update(length.to_be_bytes());
        digest.update(bytes);
    }
    Some(hexadecimal(digest.finalize()))
}

/// Hashes UTF-8 fields separated by one NUL byte.
pub fn sha256_nul_separated<S: AsRef<str>>(fields: &[S]) -> String {
    let mut digest = sha2::Sha256::new();
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            digest.update([0]);
        }
        digest.update(field.as_ref().as_bytes());
    }
    hexadecimal(digest.finalize())
}

fn hexadecimal(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
