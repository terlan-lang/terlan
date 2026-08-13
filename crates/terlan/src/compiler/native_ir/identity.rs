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

/// Retained only for regression fixtures covering the removed wrapper ABI.
#[cfg(test)]
pub(super) fn stable_composed_continuation_id(
    module: &str,
    function: &str,
    arity: usize,
    call_ordinal: usize,
    dynamic_target: Option<u64>,
    callee_continuation_id: u64,
) -> u64 {
    let mut digest = stable_site_digest(
        b"terlan-tvm-composed-continuation-v1\0",
        module,
        function,
        arity,
        call_ordinal,
    );
    match dynamic_target {
        Some(target) => {
            digest.update([1]);
            digest.update(target.to_le_bytes());
        }
        None => digest.update([0]),
    }
    digest.update(callee_continuation_id.to_le_bytes());
    digest_prefix_id(digest)
}

/// Derives the synchronous completion identity for one composed call site.
pub(super) fn stable_composed_completion_id(
    module: &str,
    function: &str,
    arity: usize,
    call_ordinal: usize,
) -> u64 {
    digest_prefix_id(stable_site_digest(
        b"terlan-tvm-composed-completion-v1\0",
        module,
        function,
        arity,
        call_ordinal,
    ))
}

/// Derives the stable VM reduction-yield resume identity for one function.
pub(super) fn stable_reduction_continuation_id(module: &str, function: &str, arity: usize) -> u64 {
    stable_id(
        b"terlan-tvm-reduction-continuation-v1\0",
        module,
        function,
        arity,
        None,
    )
}

/// Derives the private synchronous-completion node used when a caller wraps a
/// recursive reduction continuation.
pub(super) fn stable_reduction_completion_id(module: &str, function: &str, arity: usize) -> u64 {
    stable_id(
        b"terlan-tvm-reduction-completion-v1\0",
        module,
        function,
        arity,
        None,
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
    digest_prefix_bytes(&bytes)
}

fn stable_site_digest(
    domain: &[u8],
    module: &str,
    function: &str,
    arity: usize,
    ordinal: usize,
) -> Sha256 {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(module.as_bytes());
    digest.update(b"\0");
    digest.update(function.as_bytes());
    digest.update(b"\0");
    digest.update(arity.to_le_bytes());
    digest.update(ordinal.to_le_bytes());
    digest
}

fn digest_prefix_id(digest: Sha256) -> u64 {
    let bytes = digest.finalize();
    digest_prefix_bytes(&bytes)
}

fn digest_prefix_bytes(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(
        bytes[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
    .max(1)
}
