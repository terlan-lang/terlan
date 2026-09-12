//! Canonical callable admission metadata shared by packaged and linked AOT images.

use crate::runtime::native_image::TvmCallableDescriptor;

use super::{is_materialized_continuation_module, NativeModule, NativeType};

/// Builds the sorted callable table, excluding synthetic continuation exports.
/// Capture-free callables remain admitted; duplicate IDs are retained for the
/// image validator to reject rather than silently discarding a conflicting entry.
pub(crate) fn native_callable_descriptors(modules: &[NativeModule]) -> Vec<TvmCallableDescriptor> {
    let mut callables = modules
        .iter()
        .filter(|module| !is_materialized_continuation_module(module))
        .flat_map(|module| &module.functions)
        .map(|function| TvmCallableDescriptor {
            id: function.export_id,
            parameters: function
                .params
                .iter()
                .skip(function.callable_captures.len())
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
            results: vec![function.return_type.boundary_type()],
            captures: function
                .callable_captures
                .iter()
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
        })
        .collect::<Vec<_>>();
    callables.sort_by_key(|callable| callable.id);
    callables
}
