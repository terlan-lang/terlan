use sha2::{Digest, Sha256};

/// Derives a package-composable format-1 export identity from its full name.
pub(super) fn stable_export_id(module: &str, function: &str, arity: usize) -> u64 {
    stable_id(b"terlan-tvm-export-v1\0", module, function, arity, None)
}

/// Derives a stable resume-entry identity within one function.
pub(super) fn stable_continuation_id(
    module: &str,
    function: &str,
    arity: usize,
    ordinal: usize,
) -> u64 {
    stable_id(
        b"terlan-tvm-continuation-v1\0",
        module,
        function,
        arity,
        Some(ordinal),
    )
}

fn stable_id(
    domain: &[u8],
    module: &str,
    function: &str,
    arity: usize,
    ordinal: Option<usize>,
) -> u64 {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(module.as_bytes());
    digest.update(b"\0");
    digest.update(function.as_bytes());
    digest.update(b"\0");
    digest.update(arity.to_le_bytes());
    if let Some(ordinal) = ordinal {
        digest.update(ordinal.to_le_bytes());
    }
    let bytes = digest.finalize();
    u64::from_le_bytes(
        bytes[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
    .max(1)
}
