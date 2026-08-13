use crate::compiler::native_ir::{NativeModule, NativeType};
use crate::runtime::native_image::managed::{decode_aggregate_layout, decode_collection_layout};
use crate::runtime::native_image::{
    host_tvm_target, TvmBoundaryType, TvmCallableDescriptor, TvmContinuationDescriptor,
    TvmExecutableDescriptor, TvmExportDescriptor, TvmImageIdentity, TvmImageIntegrity,
    TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor,
};

use super::super::BuildOneError;
use super::native_image::{DIRECT_AOT_BACKEND, DIRECT_AOT_CODEGEN_REVISION};
use crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_ABI_VERSION;

/// Builds the canonical descriptor embedded into one native application image.
pub(super) fn native_application_image_descriptor(
    application_identity: &str,
    package: &str,
    natives: &[NativeModule],
    input_sha256: &str,
) -> Result<TvmExecutableDescriptor, BuildOneError> {
    let mut exports = natives
        .iter()
        .flat_map(|native| {
            native
                .functions
                .iter()
                .filter(|function| function.public)
                .map(move |function| TvmExportDescriptor {
                    id: function.export_id,
                    name: format!("{}.{}/{}", native.name, function.name, function.arity),
                    parameters: function
                        .params
                        .iter()
                        .copied()
                        .map(native_boundary_type)
                        .collect(),
                    results: vec![native_boundary_type(function.return_type)],
                })
        })
        .collect::<Vec<_>>();
    exports.sort_by_key(|export| export.id);
    let mut continuations = natives
        .iter()
        .flat_map(|native| {
            native
                .continuations
                .iter()
                .map(|continuation| TvmContinuationDescriptor {
                    id: continuation.id,
                    parameters: continuation
                        .params
                        .iter()
                        .copied()
                        .map(native_boundary_type)
                        .collect(),
                    results: vec![native_boundary_type(continuation.return_type)],
                })
        })
        .collect::<Vec<_>>();
    continuations.sort_by_key(|continuation| continuation.id);
    let mut callables = natives
        .iter()
        .flat_map(|native| {
            native
                .functions
                .iter()
                .filter(|_| {
                    !crate::compiler::native_ir::is_materialized_continuation_module(native)
                })
                .map(|function| TvmCallableDescriptor {
                    id: function.export_id,
                    parameters: function
                        .params
                        .iter()
                        .skip(function.callable_captures.len())
                        .copied()
                        .map(native_boundary_type)
                        .collect(),
                    results: vec![native_boundary_type(function.return_type)],
                    captures: function
                        .callable_captures
                        .iter()
                        .copied()
                        .map(native_boundary_type)
                        .collect(),
                })
        })
        .collect::<Vec<_>>();
    callables.sort_by_key(|callable| callable.id);
    let mut managed_layouts = natives
        .iter()
        .flat_map(|native| native.managed_layouts.iter())
        .map(|encoded_layout| {
            let descriptor = decode_aggregate_layout(encoded_layout).map_err(|error| {
                BuildOneError::Message(format!(
                    "error[native_ir.managed_layout]: invalid checked aggregate layout: {error}"
                ))
            })?;
            Ok(TvmManagedLayoutDescriptor {
                semantic_id: descriptor.managed().semantic_id().bytes(),
                encoded_layout: encoded_layout.to_vec(),
            })
        })
        .collect::<Result<Vec<_>, BuildOneError>>()?;
    managed_layouts.sort_by(|left, right| {
        left.semantic_id
            .cmp(&right.semantic_id)
            .then_with(|| left.encoded_layout.cmp(&right.encoded_layout))
    });
    managed_layouts.dedup();
    let mut managed_collections = natives
        .iter()
        .flat_map(|native| native.managed_collections.iter())
        .map(|encoded_layout| {
            let descriptor = decode_collection_layout(encoded_layout).map_err(|error| {
                BuildOneError::Message(format!(
                    "error[native_ir.managed_collection]: invalid checked collection schema: {error}"
                ))
            })?;
            Ok(TvmManagedCollectionDescriptor {
                semantic_id: descriptor.semantic_id().bytes(),
                encoded_layout: encoded_layout.to_vec(),
            })
        })
        .collect::<Result<Vec<_>, BuildOneError>>()?;
    managed_collections.sort_by(|left, right| {
        left.semantic_id
            .cmp(&right.semantic_id)
            .then_with(|| left.encoded_layout.cmp(&right.encoded_layout))
    });
    managed_collections.dedup();
    Ok(TvmExecutableDescriptor {
        runtime_abi_min: 3,
        runtime_abi_max: 3,
        native_boundary_min: PUBLIC_ADAPTER_ABI_VERSION,
        native_boundary_max: PUBLIC_ADAPTER_ABI_VERSION,
        target: host_tvm_target().map_err(|error| BuildOneError::Message(error.into()))?,
        identity: TvmImageIdentity {
            compiler: format!(
                "terlc-{}-{DIRECT_AOT_BACKEND}-codegen-{DIRECT_AOT_CODEGEN_REVISION}",
                env!("CARGO_PKG_VERSION")
            ),
            build: format!("sha256:{input_sha256}"),
            package: package.to_string(),
            module: application_identity.to_string(),
        },
        exports,
        capabilities: Vec::new(),
        resources: Vec::new(),
        dependencies: Vec::new(),
        continuations,
        callables,
        managed_layouts,
        managed_collections,
        atoms: natives
            .iter()
            .flat_map(|native| native.atoms.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        integrity: TvmImageIntegrity {
            code_digest: [0; 32],
            immutable_data_digest: [0; 32],
        },
        signature: None,
    })
}

fn native_boundary_type(ty: NativeType) -> TvmBoundaryType {
    let boundary = ty.boundary_type();
    debug_assert_eq!(
        ty.is_managed_reference(),
        matches!(
            boundary,
            TvmBoundaryType::String
                | TvmBoundaryType::Bytes
                | TvmBoundaryType::Binary
                | TvmBoundaryType::Managed(_)
        )
    );
    boundary
}
