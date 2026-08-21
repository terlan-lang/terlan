//! VM-owned immutable AOT dispatch-table lookup.

use std::ffi::c_void;

/// Bytes in one sparse dispatch-index entry.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) const TVM_DISPATCH_INDEX_ENTRY_BYTES: usize = 4;
/// Bytes in one dense dispatch record.
pub(crate) const TVM_DISPATCH_RECORD_ENTRY_BYTES: usize = 24;
/// Byte offset of the export identity inside one dense record.
pub(crate) const TVM_DISPATCH_RECORD_EXPORT_ID_OFFSET: usize = 0;
/// Byte offset of the native function pointer inside one dense record.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) const TVM_DISPATCH_RECORD_FUNCTION_POINTER_OFFSET: usize = 8;
/// Byte offset of the transition count inside one dense record.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) const TVM_DISPATCH_RECORD_TRANSITION_COUNT_OFFSET: usize = 16;
/// Byte offset of the compact ABI shape inside one dense record.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) const TVM_DISPATCH_RECORD_SHAPE_OFFSET: usize = 20;

/// Runtime-ABI-3 callback used by every admitted AOT image for entry lookup.
pub(crate) type TvmDispatchLookup =
    unsafe extern "C" fn(*const u32, *const u8, u64, u64) -> *const c_void;

/// Resolves one export identity through an image's compact immutable tables.
///
/// The probing algorithm is VM code, not generated application code, so all
/// simultaneously loaded image generations share one instruction body. The
/// image owns only immutable index and record data.
///
/// # Safety
///
/// `index` must address `table_mask + 1` native-endian `u32` entries and
/// `records` must address every dense record named by that index. Admitted AOT
/// images construct both buffers and retain them for the whole callback.
pub(crate) unsafe extern "C" fn tvm_dispatch_lookup_v1(
    index: *const u32,
    records: *const u8,
    table_mask: u64,
    export_id: u64,
) -> *const c_void {
    if index.is_null() || records.is_null() || table_mask == u64::MAX {
        return std::ptr::null();
    }
    let mut slot = export_id & table_mask;
    for _ in 0..=table_mask {
        // SAFETY: The caller contract guarantees the complete index buffer.
        let record_tag = unsafe { index.add(slot as usize).read() };
        if record_tag == 0 {
            return std::ptr::null();
        }
        let record_index = usize::try_from(record_tag - 1).unwrap_or(usize::MAX);
        let Some(record_offset) = record_index.checked_mul(TVM_DISPATCH_RECORD_ENTRY_BYTES) else {
            return std::ptr::null();
        };
        // SAFETY: The caller contract guarantees every indexed dense record.
        let record = unsafe { records.add(record_offset) };
        // SAFETY: Dense records are 8-byte aligned and contain a native u64 ID.
        let candidate = unsafe {
            record
                .add(TVM_DISPATCH_RECORD_EXPORT_ID_OFFSET)
                .cast::<u64>()
                .read()
        };
        if candidate == export_id {
            return record.cast();
        }
        slot = slot.wrapping_add(1) & table_mask;
    }
    std::ptr::null()
}

#[cfg(test)]
#[path = "dispatch_lookup_test.rs"]
mod tests;
