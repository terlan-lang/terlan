//! Copied-value SHA-256 exports; no compiler or VM dispatch knowledge.

use terlan_runtime_abi::{BoundaryError, ErrorDomain, FromNativeValue, NativeBinding};

use crate::hash;

/// SHA-256 of UTF-8 text.
pub const SHA256: NativeBinding = NativeBinding {
    operation: "std.crypto.hash.sha256",
    arity: 1,
    invoke: |args| {
        SHA256.validate_arity(args.len())?;
        Ok(hash::sha256_bytes(<&str>::from_native(&args[0])?.as_bytes()).into())
    },
};

/// SHA-256 of raw bytes, without UTF-8 conversion.
pub const SHA256_BYTES: NativeBinding = NativeBinding {
    operation: "std.crypto.hash.sha256_bytes",
    arity: 1,
    invoke: |args| {
        SHA256_BYTES.validate_arity(args.len())?;
        Ok(hash::sha256_bytes(<&[u8]>::from_native(&args[0])?).into())
    },
};

/// Ordered, unsigned-64-bit length-prefixed UTF-8 fields.
pub const SHA256_FRAMED: NativeBinding = NativeBinding {
    operation: "std.crypto.hash.sha256_framed",
    arity: 1,
    invoke: |args| {
        SHA256_FRAMED.validate_arity(args.len())?;
        let fields = Vec::<&str>::from_native(&args[0])?;
        framed_digest(hash::sha256_framed(&fields), SHA256_FRAMED.operation).map(Into::into)
    },
};

/// Unframed domain, NUL separator, then ordered length-prefixed UTF-8 fields.
pub const SHA256_DOMAIN_FRAMED: NativeBinding = NativeBinding {
    operation: "std.crypto.hash.sha256_domain_framed",
    arity: 2,
    invoke: |args| {
        SHA256_DOMAIN_FRAMED.validate_arity(args.len())?;
        let domain = <&str>::from_native(&args[0])?;
        let fields = Vec::<&str>::from_native(&args[1])?;
        framed_digest(
            hash::sha256_domain_framed(domain, &fields),
            SHA256_DOMAIN_FRAMED.operation,
        )
        .map(Into::into)
    },
};

/// UTF-8 fields joined by exactly one NUL octet.
pub const SHA256_NUL_SEPARATED: NativeBinding = NativeBinding {
    operation: "std.crypto.hash.sha256_nul_separated",
    arity: 1,
    invoke: |args| {
        SHA256_NUL_SEPARATED.validate_arity(args.len())?;
        Ok(hash::sha256_nul_separated(&Vec::<&str>::from_native(&args[0])?).into())
    },
};

fn framed_digest(value: Option<String>, operation: &str) -> Result<String, BoundaryError> {
    value.ok_or_else(|| BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "SHA-256 framing",
        format!("error[dispatch.hash_field_too_large]: {operation} field length exceeds unsigned 64-bit framing"),
    ))
}

#[cfg(test)]
#[path = "hash_bindings_test.rs"]
mod tests;
