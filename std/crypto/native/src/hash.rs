//! Package-owned SHA-256 framing over the maintained sha2 implementation.

use sha2::Digest;

/// Hashes bytes and returns lowercase hexadecimal SHA-256.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex_digest(&sha2::Sha256::digest(bytes))
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
    Some(hex_digest(&digest.finalize()))
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
    Some(hex_digest(&digest.finalize()))
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
    hex_digest(&digest.finalize())
}

/// Encodes digest bytes independently of the hash library's array wrapper.
fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                HEX[(byte >> 4) as usize] as char,
                HEX[(byte & 15) as usize] as char,
            ]
        })
        .collect()
}
