use super::*;

#[test]
fn lookup_resolves_collisions_and_rejects_unknown_entries() {
    let index = [1_u32, 2, 0, 0];
    let mut records = [0_u8; TVM_DISPATCH_RECORD_ENTRY_BYTES * 2];
    records[..8].copy_from_slice(&4_u64.to_ne_bytes());
    records[TVM_DISPATCH_RECORD_ENTRY_BYTES..TVM_DISPATCH_RECORD_ENTRY_BYTES + 8]
        .copy_from_slice(&8_u64.to_ne_bytes());

    let resolved = unsafe { tvm_dispatch_lookup_v1(index.as_ptr(), records.as_ptr(), 3, 8) };
    assert_eq!(resolved.cast::<u8>(), unsafe {
        records.as_ptr().add(TVM_DISPATCH_RECORD_ENTRY_BYTES)
    });
    assert!(unsafe { tvm_dispatch_lookup_v1(index.as_ptr(), records.as_ptr(), 3, 12) }.is_null());
}
