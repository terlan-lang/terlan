//! Shared metadata helpers for managed constructor layouts.

use crate::terlan_typeck::CoreType;

use super::{NativeConstructorLayouts, NativeType};

/// Normalizes compiler-owned facade fields that have one public managed ABI.
pub(in crate::compiler::native_ir) fn canonical_structural_field_name(
    canonical: &str,
    variant: &str,
    index: usize,
    source_name: Option<&str>,
) -> Option<String> {
    if canonical == "Apply(Option;String)" && variant == "Some" && index == 0 {
        Some("value".to_string())
    } else {
        source_name.map(str::to_string)
    }
}

/// Recovers the checked semantic type carried by a managed native value.
///
/// Continuation lowering retains the compact native representation for local
/// variables. Collection literals assembled from those variables still need
/// the original semantic element type so their List schema remains identical
/// to the checked program rather than becoming an opaque managed collection.
pub(in crate::compiler::native_ir) fn result_core_type_for_native(
    result: NativeType,
    layouts: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let mut matches = layouts
        .values()
        .filter(|layout| layout.result == result)
        .filter_map(|layout| layout.result_core_type.clone());
    let first = matches.next()?;
    matches.all(|candidate| candidate == first).then_some(first)
}
