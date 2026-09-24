//! Materializes a checked error before its owning actor and heap are terminated.

use super::super::{NativeImageBackend, PureNativeExecutionContext, PureNativeSuspension};
use crate::runtime::native_image::managed::{managed_erased_value_semantic_id, ManagedErasedValue};
use crate::runtime::vm::{actor::VmActorRuntime, VmRuntimeResult};

/// Retains concrete failure data through the ordinary actor-exit propagation path.
pub(super) fn service(
    backend: &dyn NativeImageBackend,
    actors: &mut VmActorRuntime,
    context: &PureNativeExecutionContext<'_>,
    suspension: &PureNativeSuspension,
) -> VmRuntimeResult<()> {
    super::validate_transition_arguments(&suspension.operation(), suspension.arguments())?;
    let word = suspension.arguments()[0];
    let (ty, value) =
        context
            .managed_ref()
            .with_public_materialization(context.owner_id(), |heap, _| {
                let reference = heap
                    .validate_abi_reference(
                        u64::from_ne_bytes(word.to_ne_bytes()),
                        managed_erased_value_semantic_id().map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                heap.erased_value(reference.cast::<ManagedErasedValue>())
                    .map_err(|error| error.to_string())
            })?;
    let value = backend.decode_transition_value(context, &ty, value)?;
    actors.service_native_typed_failure(
        context.owner_id(),
        suspension.request_id(),
        suspension.continuation_id(),
        ty,
        value,
    )?;
    Ok(())
}
