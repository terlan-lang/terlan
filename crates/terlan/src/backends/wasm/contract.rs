const ABI_SOURCE: &str = include_str!("../../../../../std/wasm/Abi.terl");

/// Canonical scalar signature used by both Wasm artifact emission and runtime validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmAbiSignature {
    pub(crate) name: String,
    pub(crate) params: Vec<String>,
    pub(crate) result: String,
}

/// Returns the checksum of the compiler-owned `std.wasm.Abi` namespace.
pub(crate) fn wasm_abi_contract_checksum() -> String {
    wasm_checksum(ABI_SOURCE.as_bytes())
}

/// Returns a deterministic checksum for ordered Wasm export signatures.
pub(crate) fn wasm_abi_signature_checksum(signatures: &[WasmAbiSignature]) -> String {
    let mut canonical = String::new();
    for signature in signatures {
        canonical.push_str(&signature.name);
        canonical.push('(');
        canonical.push_str(&signature.params.join(","));
        canonical.push_str(")->");
        canonical.push_str(&signature.result);
        canonical.push('\n');
    }
    wasm_checksum(canonical.as_bytes())
}

/// Returns the canonical checksum spelling for Wasm artifacts and contracts.
pub(crate) fn wasm_checksum(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
#[path = "contract_test.rs"]
mod contract_test;
