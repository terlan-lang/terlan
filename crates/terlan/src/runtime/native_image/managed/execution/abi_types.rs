//! Fixed callback and value types crossing the generated-code ABI.

use std::ffi::c_void;

use crate::runtime::native_image::TvmBoundaryType;

use super::super::{SemanticTypeId, TvmRef};

/// Generated aggregate allocation callback ABI.
pub(super) type ManagedAllocator =
    unsafe extern "C" fn(*mut c_void, *const u8, u64, *const i64, u64, *mut u64) -> i32;

/// Generated owned-closure resolution callback ABI.
pub(super) type ManagedClosureResolver = unsafe extern "C" fn(
    *mut c_void,
    i64,
    *const i64,
    u64,
    *const i64,
    *const i64,
    u64,
    *mut u64,
    *mut i64,
    u64,
    *mut u64,
) -> i32;

/// Returns the semantic identity for one actor-heap boundary value.
pub(super) fn managed_semantic_id(
    boundary_type: &TvmBoundaryType,
) -> Result<Option<SemanticTypeId>, String> {
    let canonical = match boundary_type {
        TvmBoundaryType::String => Some("std.core.String"),
        TvmBoundaryType::Bytes => Some("std.binary.Bytes"),
        TvmBoundaryType::Binary => Some("std.binary.Binary"),
        TvmBoundaryType::Managed(bytes) => return Ok(Some(SemanticTypeId::from_bytes(*bytes))),
        _ => None,
    };
    canonical
        .map(SemanticTypeId::from_canonical)
        .transpose()
        .map_err(|error| format!("error[managed_execution.capture_type]: {error}"))
}

/// Encodes one runtime-private reference as the fixed native word representation.
pub(super) fn reference_word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}
