//! Legacy MD5 integrity adapter for `std.encoding.Md5`.
//!
//! MD5 is retained only for compatibility with existing integrity formats. It
//! is not suitable for passwords, signatures, authentication, or any other
//! security decision. The implementation delegates to the maintained
//! RustCrypto `md-5` crate.

use md5::{Digest, Md5};

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Returns the lowercase MD5 digest of UTF-8 text.
///
/// Inputs:
/// - `text`: UTF-8 source text.
///
/// Output:
/// - A 32-character lowercase hexadecimal digest.
///
/// Transformation:
/// - Hashes the exact UTF-8 bytes with RustCrypto and renders each digest byte
///   as two lowercase hexadecimal characters.
pub fn digest(text: &str) -> String {
    digest_bytes(text.as_bytes())
}

/// Returns a lowercase MD5 digest for an arbitrary byte slice.
fn digest_bytes(bytes: &[u8]) -> String {
    let digest = Md5::digest(bytes);
    let mut output = String::with_capacity(32);
    for byte in digest {
        output.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
#[path = "md5_test.rs"]
mod md5_test;
