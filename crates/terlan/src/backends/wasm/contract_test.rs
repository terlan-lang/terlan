use super::*;

#[test]
fn wasm_abi_contract_checksum_tracks_the_source_namespace() {
    let contract_checksum = wasm_abi_contract_checksum();

    assert!(contract_checksum.starts_with("fnv1a64:"));
    assert_eq!(contract_checksum.len(), "fnv1a64:0000000000000000".len());
    assert_ne!(contract_checksum, wasm_checksum(b"pub type I32 = Int."));
}

#[test]
fn wasm_abi_signature_checksum_tracks_type_and_order_changes() {
    let baseline = vec![WasmAbiSignature {
        name: "identity".to_string(),
        params: vec!["i32".to_string()],
        result: "i32".to_string(),
    }];
    let changed = vec![WasmAbiSignature {
        name: "identity".to_string(),
        params: vec!["i64".to_string()],
        result: "i64".to_string(),
    }];

    assert_ne!(
        wasm_abi_signature_checksum(&baseline),
        wasm_abi_signature_checksum(&changed)
    );
}
