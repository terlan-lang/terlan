//! Direct execution-shard loader for admitted Terlan AOT images.

#[path = "direct_backend/managed_values.rs"]
mod managed_values;

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

use libloading::{Library, Symbol};

use crate::runtime::native_image::control::{TvmControlFrame, TvmTransitionOperation};
use crate::runtime::native_image::managed::{ManagedExecutionRuntime, SemanticTypeId};
use crate::runtime::native_image::{
    SealedTvmImage, TvmBoundaryType, TvmContinuationDescriptor, TvmExportDescriptor,
    TVM_DISPATCH_SYMBOL_V2, TVM_INDIRECT_TRANSITION_WORD_CAPACITY,
};
use crate::runtime::vm::bitstring::VmBitString;
use crate::runtime::vm::ReplValue;

use super::{decode_native_value, NativeImageBackend, PureNativeExecutionContext};
use managed_values::{allocate_public_managed, materialize_public_managed};

/// Runtime-ABI-2 native image dispatch ABI.
type NativeDispatch = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    *const c_void,
    u64,
    *const i64,
    u64,
    *mut i64,
    *mut i64,
    u64,
    *mut u64,
) -> i32;

struct LoadedDirectImage {
    /// Loaded library retained for at least as long as its dispatch pointer.
    _library: Library,
    /// Runtime-owned immutable file retained until after the library unloads.
    sealed: SealedTvmImage,
    /// Admitted native dispatch entry copied from the retained library.
    dispatch: NativeDispatch,
    /// Maximum bounded transition word count required by this image.
    transition_capacity: usize,
    /// Exact callable export signatures admitted with the image.
    exports: Vec<TvmExportDescriptor>,
    /// Exact generated continuation signatures admitted with the image.
    continuations: Vec<TvmContinuationDescriptor>,
    /// Stable descriptor identity used by lifecycle and diagnostic records.
    image_identity: String,
    /// Descriptor digest validated against the sealed executable mapping.
    descriptor_digest: [u8; 32],
}

/// Shard-owned direct native dispatch with no application-call IPC.
pub(crate) struct DirectNativeBackend {
    /// Immutable loaded image shared by independent actor-runtime forks.
    image: Arc<LoadedDirectImage>,
}

impl std::fmt::Debug for DirectNativeBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectNativeBackend")
            .field("transition_capacity", &self.image.transition_capacity)
            .finish_non_exhaustive()
    }
}

impl DirectNativeBackend {
    /// Admits and loads an image inside the execution-shard process.
    #[allow(unsafe_code)]
    pub(crate) fn load(path: &Path) -> Result<(Self, ManagedExecutionRuntime), String> {
        let sealed = SealedTvmImage::admit(path)?;
        let inspection = sealed.inspection().clone();
        let descriptor_digest = inspection.descriptor_digest;
        let descriptor = inspection.descriptor;
        let identity = &descriptor.identity;
        let image_identity = format!(
            "{}:{}:{}:{}",
            identity.compiler, identity.build, identity.package, identity.module
        );
        let transition_capacity = descriptor
            .continuations
            .iter()
            .map(|continuation| continuation.parameters.len())
            .max()
            .unwrap_or(0)
            .saturating_add(5)
            .max(
                (!descriptor.callables.is_empty())
                    .then_some(TVM_INDIRECT_TRANSITION_WORD_CAPACITY)
                    .unwrap_or(0),
            );

        // SAFETY: This process is the supervised execution shard. Admission
        // above validates the native image and fixed dispatch ABI before load.
        let library = unsafe { Library::new(sealed.path()) }.map_err(|error| {
            format!("error[execution_shard.load]: failed to load sealed image: {error}")
        })?;
        sealed.verify_unchanged()?;
        // SAFETY: format 1 fixes this symbol to `NativeDispatch`; the copied
        // function pointer cannot outlive `library`, retained in the same Arc.
        let dispatch: Symbol<'_, NativeDispatch> =
            unsafe { library.get(TVM_DISPATCH_SYMBOL_V2.as_bytes()) }.map_err(|error| {
                format!(
                "error[execution_shard.symbol]: failed to load `{TVM_DISPATCH_SYMBOL_V2}`: {error}"
            )
            })?;
        let dispatch = *dispatch;
        let managed = ManagedExecutionRuntime::with_executable_image_metadata(
            &descriptor.managed_layouts,
            &descriptor.managed_collections,
            &descriptor.atoms,
            descriptor_digest,
            &descriptor.callables,
        )?;
        Ok((
            Self {
                image: Arc::new(LoadedDirectImage {
                    _library: library,
                    sealed,
                    dispatch,
                    transition_capacity,
                    exports: descriptor.exports,
                    continuations: descriptor.continuations,
                    image_identity,
                    descriptor_digest,
                }),
            },
            managed,
        ))
    }

    /// Returns lifecycle metadata from the exact image loaded by the platform loader.
    pub(super) fn resolved_artifact(&self) -> Result<super::ResolvedPureArtifact, String> {
        Ok(super::ResolvedPureArtifact {
            image_identity: self.image.image_identity.clone(),
            descriptor_digest: self.image.descriptor_digest,
            exports: super::exports_from_descriptor_parts(&self.image.exports)?,
            continuations: self.image.continuations.clone(),
        })
    }

    #[allow(unsafe_code)]
    fn dispatch(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        entry_id: u64,
        arguments: &[i64],
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        let mut value = 0_i64;
        let mut transition_values = vec![0_i64; self.image.transition_capacity];
        let mut transition_len = 0_u64;
        let dispatch = self.image.dispatch;
        let transition_capacity = self.image.transition_capacity;
        let status =
            context
                .managed()
                .with_dispatch(owner_id, |context, allocator, closure_resolver| {
                    // SAFETY: The image was admitted with the fixed format-1 dispatch
                    // ABI. The managed runtime keeps this context and allocator valid
                    // only for the duration of this synchronous native call.
                    unsafe {
                        dispatch(
                            context,
                            allocator,
                            closure_resolver,
                            entry_id,
                            arguments.as_ptr(),
                            arguments.len() as u64,
                            &mut value,
                            transition_values.as_mut_ptr(),
                            transition_capacity as u64,
                            &mut transition_len,
                        )
                    }
                });
        if let Some(error) = context.managed().take_allocation_error() {
            return Err(error);
        }
        let transition_len = usize::try_from(transition_len).map_err(|_| {
            "error[execution_shard.transition_size]: transition length exceeds usize".to_string()
        })?;
        if transition_len > self.image.transition_capacity {
            return Err(format!(
                "error[execution_shard.transition_size]: native transition returned {transition_len} values with capacity {}",
                self.image.transition_capacity
            ));
        }
        transition_values.truncate(transition_len);
        let mut frame = frame_from_status(request_id, owner_id, status, value, transition_values)?;
        self.validate_result(context, entry_id, &frame)?;
        self.park_transition(context, &mut frame)?;
        Ok(frame)
    }

    /// Validates a successful managed result before it leaves native dispatch.
    fn validate_result(
        &self,
        context: &PureNativeExecutionContext<'_>,
        entry_id: u64,
        frame: &TvmControlFrame,
    ) -> Result<(), String> {
        let TvmControlFrame::Success { value, .. } = frame else {
            return Ok(());
        };
        let result = self.entry_result(entry_id)?;
        if result.is_managed_reference() {
            context.managed_ref().validate_boundary_reference(
                context.owner_id(),
                result,
                *value,
            )?;
        }
        Ok(())
    }

    /// Resolves the single result type attached to one generated entry.
    fn entry_result(&self, entry_id: u64) -> Result<&TvmBoundaryType, String> {
        let results = self
            .image
            .exports
            .iter()
            .find(|entry| entry.id == entry_id)
            .map(|entry| entry.results.as_slice())
            .or_else(|| {
                self.image
                    .continuations
                    .iter()
                    .find(|entry| entry.id == entry_id)
                    .map(|entry| entry.results.as_slice())
            })
            .ok_or_else(|| {
                format!("error[execution_shard.entry]: image has no entry {entry_id}")
            })?;
        let [result] = results else {
            return Err(format!(
                "error[execution_shard.result]: entry {entry_id} must declare one result"
            ));
        };
        Ok(result)
    }

    /// Resolves the exact argument types attached to one generated entry.
    fn entry_parameters(&self, entry_id: u64) -> Result<&[TvmBoundaryType], String> {
        self.image
            .exports
            .iter()
            .find(|entry| entry.id == entry_id)
            .map(|entry| entry.parameters.as_slice())
            .or_else(|| {
                self.image
                    .continuations
                    .iter()
                    .find(|entry| entry.id == entry_id)
                    .map(|entry| entry.parameters.as_slice())
            })
            .ok_or_else(|| format!("error[execution_shard.entry]: image has no entry {entry_id}"))
    }

    /// Converts one public runtime value into an owner-local native word.
    fn encode_argument(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: &ReplValue,
    ) -> Result<i64, String> {
        let owner_id = context.owner_id();
        encode_public_argument(context.managed(), owner_id, boundary_type, value)
    }

    /// Removes managed captures from a transition and retains precise roots.
    fn park_transition(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        frame: &mut TvmControlFrame,
    ) -> Result<(), String> {
        let TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id,
            operation,
            arguments,
            values,
            ..
        } = frame
        else {
            return Ok(());
        };
        let continuation = self.continuation(*continuation_id)?.clone();
        let injected_type = transition_injected_type(operation, arguments)?;
        let (_, captures) = split_continuation_types(injected_type.as_ref(), &continuation)?;
        let (transported, managed) = context.managed().park_continuation_captures(
            *owner_id,
            *continuation_id,
            captures,
            values,
        )?;
        *values = transported;
        context.park_continuation(*request_id, *continuation_id, injected_type, managed)
    }

    /// Restores scalar and managed captures in generated parameter order.
    fn restore_continuation(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        transported: &[i64],
    ) -> Result<Vec<i64>, String> {
        let claim = context.claim_continuation(request_id, continuation_id)?;
        let owner_id = claim.owner_id();
        debug_assert_eq!(claim.request_id(), request_id);
        debug_assert_eq!(claim.continuation_id(), continuation_id);
        let (injected_type, managed) = claim.into_resume_state();
        let continuation = self.continuation(continuation_id)?.clone();
        let (injected, captures) = split_continuation_types(injected_type.as_ref(), &continuation)?;
        if transported.len() < injected.len() {
            return Err(format!(
                "error[execution_shard.continuation_arity]: continuation {continuation_id} requires {} injected values",
                injected.len()
            ));
        }
        let (injected_values, scalar_captures) = transported.split_at(injected.len());
        let captures = context.managed().restore_continuation_captures(
            owner_id,
            continuation_id,
            captures,
            scalar_captures,
            managed,
        )?;
        let mut restored = Vec::with_capacity(injected_values.len() + captures.len());
        restored.extend_from_slice(injected_values);
        restored.extend(captures);
        Ok(restored)
    }

    /// Finds one admitted continuation descriptor by stable entry identity.
    fn continuation(&self, continuation_id: u64) -> Result<&TvmContinuationDescriptor, String> {
        self.image
            .continuations
            .iter()
            .find(|entry| entry.id == continuation_id)
            .ok_or_else(|| {
                format!(
                    "error[execution_shard.continuation_unknown]: image has no continuation {continuation_id}"
                )
            })
    }
}

impl NativeImageBackend for DirectNativeBackend {
    fn whole_image_digest(&self) -> Option<[u8; 32]> {
        Some(self.image.sealed.bytes_digest())
    }
    fn call_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        export_id: u64,
        args: &[ReplValue],
    ) -> Result<TvmControlFrame, String> {
        let parameters = self.entry_parameters(export_id)?.to_vec();
        if parameters.len() != args.len() {
            return Err(format!(
                "error[execution_shard.arity]: entry {export_id} expects {} arguments, received {}",
                parameters.len(),
                args.len()
            ));
        }
        let arguments = parameters
            .iter()
            .zip(args)
            .map(|(boundary_type, value)| self.encode_argument(context, boundary_type, value))
            .collect::<Result<Vec<_>, _>>()?;
        self.dispatch(context, request_id, export_id, &arguments)
    }

    fn resume_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        values: Vec<i64>,
    ) -> Result<TvmControlFrame, String> {
        let values = self.restore_continuation(context, request_id, continuation_id, &values)?;
        self.dispatch(context, request_id, continuation_id, &values)
    }

    fn decode_result(
        &self,
        context: &PureNativeExecutionContext<'_>,
        result_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ReplValue, String> {
        decode_public_result(
            context.managed_ref(),
            context.owner_id(),
            result_type,
            value,
        )
    }

    fn decode_transition_value(
        &self,
        context: &PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ReplValue, String> {
        decode_public_result(
            context.managed_ref(),
            context.owner_id(),
            boundary_type,
            value,
        )
    }

    fn encode_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: &ReplValue,
    ) -> Result<i64, String> {
        let owner_id = context.owner_id();
        encode_public_argument(context.managed(), owner_id, boundary_type, value)
    }

    fn copy_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        recipient_id: u64,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<Option<crate::runtime::vm::process::VmManagedMailboxToken>, String> {
        if !boundary_type.is_managed_reference() {
            return Ok(None);
        }
        let owner_id = context.owner_id();
        let fragment =
            context
                .managed()
                .copy_mailbox_value(owner_id, recipient_id, boundary_type, value)?;
        crate::runtime::vm::process::VmManagedMailboxToken::new(
            fragment.fragment_id(),
            fragment.sender().get(),
            fragment.receiver().get(),
            fragment.receiver_heap_bytes(),
        )
        .map(Some)
    }

    fn rollback_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        fragment: crate::runtime::vm::process::VmManagedMailboxToken,
    ) -> Result<(), String> {
        context
            .managed()
            .rollback_mailbox_value(fragment.fragment_id())
    }

    fn consume_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        fragment: crate::runtime::vm::process::VmManagedMailboxToken,
    ) -> Result<(), String> {
        context
            .managed()
            .consume_mailbox_value(fragment.fragment_id())
    }

    fn encode_transition_message(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        message: &crate::runtime::vm::process::VmMessage,
    ) -> Result<i64, String> {
        let owner_id = context.owner_id();
        if let Some(fragment) = message.managed_fragment {
            if fragment.receiver() != owner_id {
                return Err(
                    "error[execution_shard.mailbox_owner]: managed mailbox receiver mismatch"
                        .to_string(),
                );
            }
            return context.managed_ref().mailbox_value_word(
                fragment.fragment_id(),
                owner_id,
                boundary_type,
            );
        }
        encode_public_argument(context.managed(), owner_id, boundary_type, &message.payload)
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn shutdown_owner(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        context.release_owner();
        Ok(())
    }

    fn fork_box(&self) -> Result<Box<dyn NativeImageBackend>, String> {
        Ok(Box::new(Self {
            image: self.image.clone(),
        }))
    }
}

/// Converts one public runtime argument into its descriptor-directed native word.
fn encode_public_argument(
    managed: &mut ManagedExecutionRuntime,
    owner_id: u64,
    boundary_type: &TvmBoundaryType,
    value: &ReplValue,
) -> Result<i64, String> {
    match (boundary_type, value) {
        (TvmBoundaryType::Unit, ReplValue::Unit) => Ok(0),
        (TvmBoundaryType::Int, ReplValue::Int(value)) => Ok(*value),
        (TvmBoundaryType::Bool, ReplValue::Bool(value)) => Ok(i64::from(*value)),
        (TvmBoundaryType::Atom, ReplValue::Atom(value)) => managed.encode_atom_value(value),
        (TvmBoundaryType::Float, ReplValue::Float(value)) => {
            let value = value.parse::<f64>().map_err(|error| {
                format!("error[execution_shard.type]: invalid Float `{value}`: {error}")
            })?;
            if value.is_finite() {
                Ok(i64::from_ne_bytes(value.to_bits().to_ne_bytes()))
            } else {
                Err("error[execution_shard.type]: Float must be finite".to_string())
            }
        }
        (TvmBoundaryType::String, ReplValue::String(value)) => {
            managed.allocate_string_value(owner_id, value)
        }
        (TvmBoundaryType::Bytes, ReplValue::Bytes(value)) => {
            managed.allocate_bytes_value(owner_id, value)
        }
        (TvmBoundaryType::Binary, ReplValue::BitString(value)) => {
            managed.allocate_binary_value(owner_id, value.packed_bytes(), value.bit_len())
        }
        (TvmBoundaryType::Managed(identity), value) => {
            managed.with_public_allocation(owner_id, |heap, layouts| {
                allocate_public_managed(heap, layouts, SemanticTypeId::from_bytes(*identity), value)
            })
        }
        (expected, actual) => Err(format!(
            "error[execution_shard.type]: value `{actual:?}` does not match `{expected:?}`"
        )),
    }
}

/// Materializes one descriptor-directed native result into runtime-owned storage.
fn decode_public_result(
    managed: &ManagedExecutionRuntime,
    owner_id: u64,
    result_type: &TvmBoundaryType,
    value: i64,
) -> Result<ReplValue, String> {
    match result_type {
        TvmBoundaryType::Atom => managed.materialize_atom_value(value).map(ReplValue::Atom),
        TvmBoundaryType::String => managed
            .materialize_string_value(owner_id, value)
            .map(ReplValue::String),
        TvmBoundaryType::Bytes => managed
            .materialize_bytes_value(owner_id, value)
            .map(|value| ReplValue::Bytes(Arc::from(value))),
        TvmBoundaryType::Binary => {
            let (packed, bit_length) = managed.materialize_binary_value(owner_id, value)?;
            VmBitString::from_bytes(packed, bit_length)
                .map(ReplValue::BitString)
                .map_err(|error| format!("error[execution_shard.binary]: {error}"))
        }
        TvmBoundaryType::Managed(identity) => {
            managed.with_public_materialization(owner_id, |heap, layouts| {
                materialize_public_managed(
                    heap,
                    layouts,
                    SemanticTypeId::from_bytes(*identity),
                    value,
                )
            })
        }
        _ => decode_native_value(result_type, value),
    }
}

/// Splits runtime-injected operation results from continuation captures.
fn split_continuation_types<'a>(
    injected_type: Option<&TvmBoundaryType>,
    continuation: &'a TvmContinuationDescriptor,
) -> Result<(&'a [TvmBoundaryType], &'a [TvmBoundaryType]), String> {
    if let Some(injected_type) = injected_type {
        let Some((actual, captures)) = continuation.parameters.split_first() else {
            return Err(format!(
                "error[execution_shard.continuation_type]: continuation {} must accept a {injected_type:?} result first",
                continuation.id
            ));
        };
        if actual != injected_type {
            return Err(format!(
                "error[execution_shard.continuation_type]: continuation {} accepts {actual:?}, expected {injected_type:?}",
                continuation.id
            ));
        }
        return Ok((&continuation.parameters[..1], captures));
    }
    Ok((&[], &continuation.parameters))
}

/// Resolves the exact value injected by one native transition.
fn transition_injected_type(
    operation: &TvmTransitionOperation,
    arguments: &[i64],
) -> Result<Option<TvmBoundaryType>, String> {
    match operation {
        TvmTransitionOperation::Receive if arguments.is_empty() => Ok(Some(TvmBoundaryType::Int)),
        TvmTransitionOperation::Receive => {
            TvmBoundaryType::from_transition_words(arguments).map(Some)
        }
        TvmTransitionOperation::Spawn
        | TvmTransitionOperation::Monitor
        | TvmTransitionOperation::Resource => Ok(Some(TvmBoundaryType::Int)),
        TvmTransitionOperation::Capability => capability_result_type(arguments).map(Some),
        _ => Ok(None),
    }
}

fn frame_from_status(
    request_id: u64,
    owner_id: u64,
    status: i32,
    value: i64,
    mut transition_values: Vec<i64>,
) -> Result<TvmControlFrame, String> {
    if status == 0 {
        return Ok(TvmControlFrame::Success {
            request_id,
            owner_id,
            value,
        });
    }
    let (operation, argument_count) = match status {
        6 => (TvmTransitionOperation::Yield, 0),
        8 => (TvmTransitionOperation::Send, 2),
        9 => (TvmTransitionOperation::Receive, 0),
        10 => (TvmTransitionOperation::Spawn, 1),
        11 => (TvmTransitionOperation::Timer, 1),
        12 => (TvmTransitionOperation::Link, 1),
        13 => (TvmTransitionOperation::Monitor, 1),
        14 => (TvmTransitionOperation::Resource, 1),
        15 => (TvmTransitionOperation::Cancellation, 1),
        16 => (TvmTransitionOperation::Failure, 1),
        17 => (TvmTransitionOperation::Scheduling, 1),
        22 => (TvmTransitionOperation::Send, 5),
        23 => (TvmTransitionOperation::Receive, 3),
        24 => {
            let tag = transition_values.first().copied().ok_or_else(|| {
                "error[execution_shard.capability_arguments]: missing capability tag".to_string()
            })?;
            let count = match tag {
                1 | 2 | 3 | 6 => 5,
                4 | 5 => 6,
                _ => {
                    return Err(format!(
                        "error[execution_shard.capability_arguments]: unknown capability tag {tag}"
                    ));
                }
            };
            (TvmTransitionOperation::Capability, count)
        }
        _ => {
            return Ok(TvmControlFrame::Failure {
                request_id,
                owner_id,
                status,
            });
        }
    };
    if transition_values.len() < argument_count {
        return Err(format!(
            "error[execution_shard.transition_arguments]: {operation:?} returned {} values, expected at least {argument_count}",
            transition_values.len()
        ));
    }
    let values = transition_values.split_off(argument_count);
    Ok(TvmControlFrame::Transition {
        request_id,
        owner_id,
        continuation_id: value as u64,
        operation,
        arguments: transition_values,
        values,
    })
}

fn capability_result_type(arguments: &[i64]) -> Result<TvmBoundaryType, String> {
    if arguments.len() < 4 {
        return Err(
            "error[execution_shard.capability_arguments]: result type metadata is missing".into(),
        );
    }
    TvmBoundaryType::from_transition_words(&arguments[1..4])
}

#[cfg(test)]
#[path = "direct_backend_test.rs"]
mod direct_backend_test;
