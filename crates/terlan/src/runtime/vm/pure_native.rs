//! Execution-shard-owned execution of compiler-produced AOT artifacts.

mod direct_backend;
pub(super) mod execution;
mod execution_runtime;
mod execution_shard;
mod io_wakeup;
mod thread_neutral;

#[cfg(test)]
#[path = "pure_native/multicore_model_test.rs"]
#[cfg(test)]
mod multicore_model_test;

use std::path::Path;
use std::sync::Arc;

use crate::runtime::native_image::control::TvmControlFrame;
use crate::runtime::native_image::managed::ManagedExecutionRuntime;
#[cfg(test)]
use crate::runtime::native_image::TvmExecutableDescriptor;
use crate::runtime::native_image::{
    TvmBoundaryType, TvmContinuationDescriptor, TvmExportDescriptor,
};
use crate::runtime::vm::execution_shard_protocol::VmSealedShardImage;
use crate::runtime::vm::process::{VmManagedMailboxToken, VmMessage, VmProcessSource};
use crate::runtime::vm::ReplValue;
use crate::runtime::vm::VmAotHttpResponse;

pub(crate) use crate::runtime::vm::native_image_diagnostics::{
    VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
};
pub(crate) use direct_backend::DirectNativeBackend;
#[cfg(test)]
pub(crate) use execution::{dispatch_transition_operation, validate_transition_arguments};
pub(crate) use execution::{PureNativeCapabilityRequest, PureNativeExecution};
pub(crate) use execution_runtime::PendingNativeCompletionFrame;
pub(crate) use execution_runtime::{NativeContinuationClaim, PureNativeExecutionRuntime};
#[cfg(test)]
pub(crate) use execution_shard::PureNativeActorImportFailure;
pub(crate) use execution_shard::{
    PureNativeActorTransfer, PureNativeCapabilityWait, PureNativeExecutionImage,
    PureNativeExecutionShard, PureNativeTimerWait,
};
pub(crate) use io_wakeup::{PureNativeIoWait, PureNativeIoWake};
pub(crate) use thread_neutral::PureNativeSuspension;

/// Actor-scoped mutable state lent by one execution shard for a direct call.
pub(crate) struct PureNativeExecutionContext<'a> {
    /// Exact actor authorized to execute through this borrow.
    actor: crate::runtime::vm::process::VmProcessId,
    /// Complete mutable execution state owned by the actor's shard.
    runtime: &'a mut PureNativeExecutionRuntime,
}

impl<'a> PureNativeExecutionContext<'a> {
    /// Creates one actor-scoped borrow of shard-owned managed execution state.
    pub(crate) fn new(
        actor: crate::runtime::vm::process::VmProcessId,
        runtime: &'a mut PureNativeExecutionRuntime,
    ) -> Self {
        Self { actor, runtime }
    }

    /// Returns the actor authorized by this execution context.
    pub(crate) const fn actor(&self) -> crate::runtime::vm::process::VmProcessId {
        self.actor
    }

    /// Returns the native owner identity of the authorized actor.
    pub(crate) fn owner_id(&self) -> u64 {
        self.actor.as_u64()
    }

    /// Borrows shard-owned managed state for one direct operation.
    pub(crate) fn managed(&mut self) -> &mut ManagedExecutionRuntime {
        self.runtime.managed()
    }

    /// Reads shard-owned managed state without permitting mutation.
    pub(crate) fn managed_ref(&self) -> &ManagedExecutionRuntime {
        self.runtime.managed_ref()
    }

    /// Allocates one request identity from this context's execution shard.
    pub(crate) fn allocate_request_id(&mut self) -> Result<u64, String> {
        self.runtime.allocate_request_id()
    }

    /// Parks generated continuation state under this context's exact actor.
    pub(crate) fn park_continuation(
        &mut self,
        request_id: u64,
        continuation_id: u64,
        injected_type: Option<TvmBoundaryType>,
        managed: Option<crate::runtime::native_image::managed::PendingManagedCaptures>,
        completions: Vec<PendingNativeCompletionFrame>,
    ) -> Result<(), String> {
        let owner_id = self.owner_id();
        self.runtime.park_continuation_with_completions(
            owner_id,
            request_id,
            continuation_id,
            injected_type,
            managed,
            completions,
        )
    }

    /// Collects this actor after its transition arguments have been decoded.
    pub(crate) fn collect_parked_owner_at_safepoint(&mut self) -> Result<(), String> {
        let owner_id = self.owner_id();
        self.runtime.collect_parked_owner_at_safepoint(owner_id)
    }

    /// Claims generated continuation state only with exact owner authority.
    pub(crate) fn claim_continuation(
        &mut self,
        request_id: u64,
        continuation_id: u64,
    ) -> Result<NativeContinuationClaim, String> {
        let owner_id = self.owner_id();
        self.runtime
            .claim_continuation(owner_id, request_id, continuation_id)
    }

    /// Retains the scheduler program for one independently spawned actor.
    pub(crate) fn park_resident_suspension(
        &mut self,
        suspension: PureNativeSuspension,
    ) -> Result<(), String> {
        self.runtime.park_resident_suspension(suspension)
    }

    /// Claims one independently spawned actor program for execution.
    pub(crate) fn take_resident_suspension(
        &mut self,
        owner_id: u64,
    ) -> Option<PureNativeSuspension> {
        self.runtime.take_resident_suspension(owner_id)
    }

    /// Releases all mutable state owned by this context's actor.
    pub(crate) fn release_owner(&mut self) {
        let owner_id = self.owner_id();
        self.runtime.release_owner(owner_id);
    }

    /// Reclaims one completed request heap while retaining its live owner.
    pub(crate) fn reset_owner(&mut self) {
        let owner_id = self.owner_id();
        self.runtime.reset_owner(owner_id);
    }

    /// Reborrows the same shard state for a child actor executing synchronously.
    pub(crate) fn reborrow(
        &mut self,
        actor: crate::runtime::vm::process::VmProcessId,
    ) -> PureNativeExecutionContext<'_> {
        PureNativeExecutionContext::new(actor, self.runtime)
    }
}

/// Runtime-side driver for one admitted native image.
pub(crate) trait NativeImageBackend: std::fmt::Debug + Send + Sync {
    fn call_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        export_id: u64,
        args: &[ReplValue],
    ) -> Result<TvmControlFrame, String>;

    fn resume_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        values: Vec<i64>,
    ) -> Result<TvmControlFrame, String>;

    /// Materializes one validated native result through its owning runtime.
    fn decode_result(
        &self,
        context: &PureNativeExecutionContext<'_>,
        result_type: &TvmBoundaryType,
        value: i64,
        projection: NativeResultProjection,
    ) -> Result<NativeDecodedResult, String> {
        let _ = context;
        let _ = projection;
        decode_native_value(result_type, value).map(NativeDecodedResult::Value)
    }

    /// Materializes one typed transition payload from backend-owned storage.
    fn decode_transition_value(
        &self,
        context: &PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ReplValue, String> {
        let _ = context;
        decode_native_value(boundary_type, value)
    }

    /// Allocates one VM-owned mailbox value into backend-owned actor storage.
    fn encode_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: &ReplValue,
    ) -> Result<i64, String> {
        let _ = context;
        encode_transport_value(boundary_type, value)
    }

    /// Copies one managed transition word into receiver-owned mailbox storage.
    fn copy_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        recipient_id: u64,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<Option<VmManagedMailboxToken>, String> {
        let _ = (context, recipient_id, boundary_type, value);
        Ok(None)
    }

    /// Rolls back a managed graph when mailbox admission rejects publication.
    fn rollback_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        fragment: VmManagedMailboxToken,
    ) -> Result<(), String> {
        let _ = (context, fragment);
        Ok(())
    }

    /// Releases a precise managed root after its mailbox token is consumed.
    fn consume_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        fragment: VmManagedMailboxToken,
    ) -> Result<(), String> {
        let _ = (context, fragment);
        Ok(())
    }

    /// Produces one continuation word from an exact typed mailbox message.
    fn encode_transition_message(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        message: &VmMessage,
    ) -> Result<i64, String> {
        if message.managed_fragment.is_some() {
            return Err(
                "error[pure_native_managed_mailbox]: backend cannot consume managed mailbox fragments"
                    .to_string(),
            );
        }
        self.encode_transition_value(context, boundary_type, &message.payload)
    }

    fn shutdown(&mut self) -> Result<(), String>;

    /// Returns the complete executable digest when backed by a loaded image.
    fn whole_image_digest(&self) -> Option<[u8; 32]> {
        None
    }

    /// Releases one actor's backend-owned state without disturbing sibling actors.
    fn shutdown_owner(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        let _ = context;
        self.shutdown()
    }

    /// Reclaims request-local state for a live reusable service actor.
    fn reset_owner(&mut self, context: &mut PureNativeExecutionContext<'_>) -> Result<(), String> {
        self.shutdown_owner(context)
    }

    fn fork_box(&self) -> Result<Box<dyn NativeImageBackend>, String>;
}

/// VM-owned connection to one verified native execution backend.
#[derive(Debug)]
pub(crate) struct PureNativeBoundary {
    artifact: Option<ResolvedPureArtifact>,
    backend: Option<Box<dyn NativeImageBackend>>,
    call_cache: Option<NativeCallCache>,
}

/// Runtime-owned typed export description independent from artifact JSON.
#[derive(Clone, Debug)]
pub(crate) struct PureNativeExportSpec {
    pub(crate) id: u64,
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) parameters: Vec<TvmBoundaryType>,
    pub(crate) result: TvmBoundaryType,
}

#[derive(Clone, Debug)]
struct ResolvedPureArtifact {
    /// Stable descriptor identity used by shard admission and crash reports.
    image_identity: String,
    /// Verified descriptor digest admitted by the execution-shard supervisor.
    descriptor_digest: [u8; 32],
    exports: Vec<PureNativeExportSpec>,
    continuations: Vec<TvmContinuationDescriptor>,
}

struct PreparedNativeCall {
    request_id: u64,
    owner_id: u64,
    export_id: u64,
    result_type: TvmBoundaryType,
    /// Populated only after generated code actually returns a transition.
    continuations: Option<Vec<TvmContinuationDescriptor>>,
    trace_source: Option<VmProcessSource>,
    result_projection: NativeResultProjection,
}

/// Last resolved entry on one owner-local boundary.
#[derive(Clone, Debug)]
struct NativeCallCache {
    requested_function: String,
    arity: usize,
    export_index: usize,
    continuations: Arc<[TvmContinuationDescriptor]>,
}

/// Call-scoped result representation selected by the VM consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeResultProjection {
    PublicValue,
    HttpResponse,
}

/// Backend-decoded result before the execution driver selects its consumer.
#[derive(Debug)]
pub(crate) enum NativeDecodedResult {
    Value(ReplValue),
    HttpResponse(VmAotHttpResponse),
}

impl PureNativeBoundary {
    /// Loads a self-describing TVM image without transitional JSON metadata.
    pub(crate) fn load_image(path: &Path) -> Result<(Self, ManagedExecutionRuntime), String> {
        let (backend, managed) = DirectNativeBackend::load(path)?;
        let artifact = backend.resolved_artifact()?;
        Ok((
            Self {
                artifact: Some(artifact),
                backend: Some(Box::new(backend)),
                call_cache: None,
            },
            managed,
        ))
    }

    fn prepare_call(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
        trace_enabled: bool,
        result_projection: NativeResultProjection,
    ) -> Result<PreparedNativeCall, String> {
        let owner_id = context.owner_id();
        let artifact = self.artifact.as_ref().ok_or_else(|| {
            format!(
                "error[pure_native_artifact_missing]: native_pure function `{function}/{}` has no loaded AOT artifact",
                args.len()
            )
        })?;
        let cached_index = self.call_cache.as_ref().and_then(|cached| {
            (cached.arity == args.len() && cached.requested_function == function)
                .then_some(cached.export_index)
        });
        let export_index = match cached_index {
            Some(index) => index,
            None => {
                let mut matches = artifact.exports.iter().enumerate().filter(|(_, export)| {
                    export.arity == args.len() && export_matches(export, function)
                });
                let (index, _) = match matches.next() {
                    Some(found) => found,
                    None => {
                        return Err(format!(
                            "error[pure_native_export_missing]: AOT artifact does not export `{function}/{}`",
                            args.len()
                        ));
                    }
                };
                if matches.next().is_some() {
                    return Err(format!(
                        "error[pure_native_export_ambiguous]: native entry `{function}/{}` exists in multiple modules; use its qualified name",
                        args.len()
                    ));
                }
                self.call_cache = Some(NativeCallCache {
                    requested_function: function.to_owned(),
                    arity: args.len(),
                    export_index: index,
                    continuations: Arc::from(artifact.continuations.clone()),
                });
                index
            }
        };
        let artifact = self
            .artifact
            .as_ref()
            .expect("artifact remains admitted while resolving its cached export");
        let export = &artifact.exports[export_index];
        validate_arguments(export, args)?;
        if self.backend.is_none() {
            return Err(
                "error[execution_shard.backend_missing]: admitted AOT image has no in-shard backend"
                    .to_string(),
            );
        }
        Ok(PreparedNativeCall {
            request_id: context.allocate_request_id()?,
            owner_id,
            export_id: export.id,
            result_type: export.result.clone(),
            continuations: None,
            trace_source: trace_enabled.then(|| {
                VmProcessSource::new(export.module.clone(), export.function.clone(), export.arity)
            }),
            result_projection,
        })
    }

    /// Returns whether this boundary owns an exact typed export.
    pub(crate) fn has_export(&self, function: &str, arity: usize) -> bool {
        self.artifact.as_ref().is_some_and(|artifact| {
            artifact
                .exports
                .iter()
                .any(|export| export.arity == arity && export_matches(export, function))
        })
    }

    /// Returns sealed metadata for supervisor-owned admission of this image.
    fn sealed_image(&self) -> Result<VmSealedShardImage, String> {
        let artifact = self.artifact.as_ref().ok_or_else(|| {
            "error[execution_shard.admission]: native image metadata is unavailable".to_string()
        })?;
        VmSealedShardImage::new(artifact.image_identity.clone(), artifact.descriptor_digest)
            .map(|image| {
                image.with_continuations(
                    artifact
                        .continuations
                        .iter()
                        .map(|continuation| continuation.id)
                        .collect(),
                )
            })
            .map_err(|error| format!("error[execution_shard.admission]: {error:?}"))
    }

    /// Builds deterministic diagnostics for one admitted generation and lifetime proof.
    fn diagnostic_metadata(
        &self,
        generation_epoch: u64,
        references: &VmNativeGenerationReferenceSnapshot,
    ) -> Result<crate::runtime::vm::native_image_diagnostics::VmNativeImageDiagnosticMetadata, String>
    {
        let artifact = self.artifact.as_ref().ok_or_else(|| {
            "error[execution_shard.admission]: native image metadata is unavailable".to_string()
        })?;
        crate::runtime::vm::native_image_diagnostics::VmNativeImageDiagnosticMetadata::new(
            artifact.image_identity.clone(),
            artifact.descriptor_digest,
            artifact
                .continuations
                .iter()
                .map(|continuation| continuation.id)
                .collect(),
            generation_epoch,
            references,
        )
    }

    /// Returns the stable identity embedded in the admitted image descriptor.
    fn image_identity(&self) -> Result<&str, String> {
        self.artifact
            .as_ref()
            .map(|artifact| artifact.image_identity.as_str())
            .ok_or_else(|| {
                "error[execution_shard.admission]: native image identity is unavailable".to_string()
            })
    }

    /// Returns the complete digest of the exact executable mapping.
    fn whole_image_digest(&self) -> Result<[u8; 32], String> {
        self.backend
            .as_deref()
            .and_then(NativeImageBackend::whole_image_digest)
            .ok_or_else(|| {
                "error[execution_shard.admission]: whole-image digest is unavailable".to_string()
            })
    }

    /// Returns whether another boundary names the already admitted generation.
    fn is_same_generation(&self, candidate: &Self) -> Result<bool, String> {
        let current = self.artifact.as_ref().ok_or_else(|| {
            "error[execution_shard.admission]: current image metadata is unavailable".to_string()
        })?;
        let candidate = candidate.artifact.as_ref().ok_or_else(|| {
            "error[execution_shard.admission]: candidate image metadata is unavailable".to_string()
        })?;
        Ok(current.image_identity == candidate.image_identity
            && current.descriptor_digest == candidate.descriptor_digest)
    }

    /// Creates an empty boundary sharing only immutable admitted image code.
    fn fork_empty(&self) -> Result<Self, String> {
        let backend = self.backend.as_deref().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        Ok(Self {
            artifact: self.artifact.clone(),
            backend: Some(backend.fork_box()?),
            call_cache: None,
        })
    }

    /// Releases backend state owned by one completed shard-local actor.
    fn release_owner(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        self.backend
            .as_deref_mut()
            .ok_or_else(|| {
                "error[pure_native_backend_missing]: no active native execution backend".to_string()
            })?
            .shutdown_owner(context)
    }

    /// Resets backend request state without terminating its fixed owner.
    fn reset_owner(&mut self, context: &mut PureNativeExecutionContext<'_>) -> Result<(), String> {
        self.backend
            .as_deref_mut()
            .ok_or_else(|| {
                "error[pure_native_backend_missing]: no active native execution backend".to_string()
            })?
            .reset_owner(context)
    }

    /// Gracefully terminates the active native backend.
    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        let Some(mut backend) = self.backend.take() else {
            return Ok(());
        };
        backend.shutdown()
    }
}

/// Resolves the complete callable surface from one admitted native image.
fn exports_from_descriptor_parts(
    exports: &[TvmExportDescriptor],
) -> Result<Vec<PureNativeExportSpec>, String> {
    exports
        .iter()
        .map(|export| {
            let (qualified, arity) = export.name.rsplit_once('/').ok_or_else(|| {
                format!(
                    "error[pure_native_descriptor]: export `{}` has no canonical arity suffix",
                    export.name
                )
            })?;
            let arity = arity.parse::<usize>().map_err(|_| {
                format!(
                    "error[pure_native_descriptor]: export `{}` has an invalid arity suffix",
                    export.name
                )
            })?;
            let (module, function) = qualified.rsplit_once('.').ok_or_else(|| {
                format!(
                    "error[pure_native_descriptor]: export `{}` has no canonical module prefix",
                    export.name
                )
            })?;
            if module.is_empty() || function.is_empty() || arity != export.parameters.len() {
                return Err(format!(
                    "error[pure_native_descriptor]: export `{}` arity does not match its parameter table",
                    export.name
                ));
            }
            let [result] = export.results.as_slice() else {
                return Err(format!(
                    "error[pure_native_descriptor]: export `{}` must have exactly one result",
                    export.name
                ));
            };
            Ok(PureNativeExportSpec {
                id: export.id,
                module: module.to_string(),
                function: function.to_string(),
                arity,
                parameters: export.parameters.clone(),
                result: result.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
fn validate_continuation(
    continuation: &TvmContinuationDescriptor,
    result_type: &TvmBoundaryType,
    values: &[i64],
) -> Result<(), String> {
    validate_continuation_shape(continuation, Some(result_type), values)
}

/// Validates parked captures while allowing an intermediate continuation to
/// return its own descriptor-declared type before VM completion-frame unwind.
fn validate_continuation_captures(
    continuation: &TvmContinuationDescriptor,
    values: &[i64],
) -> Result<(), String> {
    validate_continuation_shape(continuation, None, values)
}

fn validate_continuation_shape(
    continuation: &TvmContinuationDescriptor,
    final_result: Option<&TvmBoundaryType>,
    values: &[i64],
) -> Result<(), String> {
    let transport_parameters = continuation
        .parameters
        .iter()
        .filter(|boundary_type| !boundary_type.is_managed_reference())
        .collect::<Vec<_>>();
    if transport_parameters.len() != values.len() {
        return Err(format!(
            "error[pure_native_continuation_type]: continuation {} expects {} transported values, received {}",
            continuation.id,
            transport_parameters.len(),
            values.len()
        ));
    }
    if !transport_parameters
        .iter()
        .all(|ty| is_transport_scalar_type(ty))
        || continuation.results.len() != 1
        || final_result.is_some_and(|result_type| {
            continuation.results.as_slice() != std::slice::from_ref(result_type)
        })
    {
        return Err(format!(
            "error[pure_native_continuation_type]: continuation {} does not match the declared resume signature: parameters={:?}, results={:?}, final_result={final_result:?}",
            continuation.id, continuation.parameters, continuation.results,
        ));
    }
    for (index, (boundary_type, value)) in transport_parameters.into_iter().zip(values).enumerate()
    {
        match boundary_type {
            TvmBoundaryType::Unit if *value != 0 => {
                return Err(format!(
                    "error[pure_native_continuation_type]: continuation {} Unit value {index} is {value}, expected 0",
                    continuation.id
                ));
            }
            TvmBoundaryType::Bool if !matches!(value, 0 | 1) => {
                return Err(format!(
                    "error[pure_native_continuation_type]: continuation {} Bool value {index} is {value}, expected 0 or 1",
                    continuation.id
                ));
            }
            TvmBoundaryType::Atom if u32::try_from(*value).is_err() => {
                return Err(format!(
                    "error[pure_native_continuation_type]: continuation {} Atom value {index} is {value}, expected an unsigned 32-bit atom index",
                    continuation.id
                ));
            }
            TvmBoundaryType::Float if !f64::from_bits(*value as u64).is_finite() => {
                return Err(format!(
                    "error[pure_native_continuation_type]: continuation {} Float value {index} is non-finite",
                    continuation.id
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reports whether one descriptor type is carried by the scalar protocol.
fn is_transport_scalar_type(boundary_type: &TvmBoundaryType) -> bool {
    matches!(
        boundary_type,
        TvmBoundaryType::Unit
            | TvmBoundaryType::Int
            | TvmBoundaryType::Float
            | TvmBoundaryType::Bool
            | TvmBoundaryType::Atom
    )
}

/// Projects the frozen descriptor export table into the exact call contract.
#[cfg(test)]
fn exports_from_descriptor(
    descriptor: &TvmExecutableDescriptor,
) -> Result<Vec<PureNativeExportSpec>, String> {
    exports_from_descriptor_parts(&descriptor.exports)
}

/// Matches an unqualified or exact module-qualified export without formatting.
fn export_matches(export: &PureNativeExportSpec, function: &str) -> bool {
    export.function == function
        || function
            .strip_prefix(&export.module)
            .and_then(|suffix| suffix.strip_prefix('.'))
            .is_some_and(|suffix| suffix == export.function)
}

fn validate_arguments(export: &PureNativeExportSpec, args: &[ReplValue]) -> Result<(), String> {
    if export.parameters.len() != args.len()
        || !export
            .parameters
            .iter()
            .zip(args)
            .all(|(parameter, argument)| match (parameter, argument) {
                (TvmBoundaryType::Unit, ReplValue::Unit)
                | (TvmBoundaryType::Int, ReplValue::Int(_))
                | (TvmBoundaryType::Bool, ReplValue::Bool(_))
                | (TvmBoundaryType::Atom, ReplValue::Atom(_))
                | (TvmBoundaryType::String, ReplValue::String(_))
                | (TvmBoundaryType::String, ReplValue::StringBytes(_))
                | (TvmBoundaryType::Bytes, ReplValue::Bytes(_))
                | (TvmBoundaryType::Binary, ReplValue::BitString(_)) => true,
                (TvmBoundaryType::Managed(_), ReplValue::Tuple(_))
                | (TvmBoundaryType::Managed(_), ReplValue::Record { .. })
                | (TvmBoundaryType::Managed(_), ReplValue::List(_))
                | (TvmBoundaryType::Managed(_), ReplValue::Map(_))
                | (TvmBoundaryType::Managed(_), ReplValue::Set(_)) => true,
                #[cfg(test)]
                (TvmBoundaryType::Managed(_), ReplValue::MapIndexed(_)) => true,
                (TvmBoundaryType::Float, ReplValue::Float(value)) => finite_float(value).is_ok(),
                _ => false,
            })
    {
        let actual = args
            .iter()
            .map(|argument| match argument {
                ReplValue::Unit => "Unit",
                ReplValue::Int(_) => "Int",
                ReplValue::Float(_) => "Float",
                ReplValue::Bool(_) => "Bool",
                ReplValue::Atom(_) => "Atom",
                ReplValue::String(_) | ReplValue::StringBytes(_) => "String",
                ReplValue::Bytes(_) => "Bytes",
                ReplValue::BitString(_) => "Binary",
                ReplValue::Tuple(_)
                | ReplValue::Record { .. }
                | ReplValue::List(_)
                | ReplValue::Map(_)
                | ReplValue::Set(_) => "Managed",
                #[cfg(test)]
                ReplValue::MapIndexed(_) => "Managed",
                #[cfg(test)]
                ReplValue::RandomGenerator(_) | ReplValue::Type(_) | ReplValue::Iterator { .. } => {
                    "Unsupported"
                }
            })
            .collect::<Vec<_>>();
        return Err(format!(
            "error[pure_native_type]: `{}/{}` does not match its declared native ABI: expected={:?}, actual={actual:?}",
            export.function, export.arity, export.parameters
        ));
    }
    Ok(())
}

fn decode_native_value(result_type: &TvmBoundaryType, value: i64) -> Result<ReplValue, String> {
    match (result_type, value) {
        (TvmBoundaryType::Unit, 0) => Ok(ReplValue::Unit),
        (TvmBoundaryType::Unit, _) => {
            Err("error[pure_native_reply]: Unit result is not 0".to_string())
        }
        (TvmBoundaryType::Int, value) => Ok(ReplValue::Int(value)),
        (TvmBoundaryType::Float, value) => {
            let value = f64::from_bits(value as u64);
            if value.is_finite() {
                Ok(ReplValue::Float(value.to_string()))
            } else {
                Err("error[pure_native_reply]: Float result is non-finite".to_string())
            }
        }
        (TvmBoundaryType::Bool, 0) => Ok(ReplValue::Bool(false)),
        (TvmBoundaryType::Bool, 1) => Ok(ReplValue::Bool(true)),
        (TvmBoundaryType::Bool, _) => {
            Err("error[pure_native_reply]: Bool result is not 0 or 1".to_string())
        }
        (other, _) => Err(format!(
            "error[pure_native_reply]: result type `{other:?}` requires its owning execution backend"
        )),
    }
}

/// Encodes one non-managed value for a backend-independent transition.
fn encode_transport_value(
    boundary_type: &TvmBoundaryType,
    value: &ReplValue,
) -> Result<i64, String> {
    match (boundary_type, value) {
        (TvmBoundaryType::Unit, ReplValue::Unit) => Ok(0),
        (TvmBoundaryType::Int, ReplValue::Int(value)) => Ok(*value),
        (TvmBoundaryType::Bool, ReplValue::Bool(value)) => Ok(i64::from(*value)),
        (TvmBoundaryType::Float, ReplValue::Float(value)) => finite_float(value)
            .map(|value| i64::from_ne_bytes(value.to_bits().to_ne_bytes())),
        (expected, actual) => Err(format!(
            "error[pure_native_transition_type]: value `{actual:?}` requires a backend encoder for `{expected:?}`"
        )),
    }
}

fn finite_float(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|error| format!("error[pure_native_type]: invalid Float `{value}`: {error}"))?;
    if parsed.is_finite() {
        Ok(parsed)
    } else {
        Err(format!(
            "error[pure_native_type]: invalid Float `{value}`: value must be finite"
        ))
    }
}

fn validate_request_id(actual: u64, expected: u64) -> Result<(), String> {
    if actual != expected {
        return Err(format!(
            "error[pure_native_correlation]: expected request `{expected}`, received `{actual}`"
        ));
    }
    Ok(())
}

fn validate_owner_id(actual: u64, expected: u64) -> Result<(), String> {
    if actual != expected {
        return Err(format!(
            "error[pure_native_owner]: expected owner `{expected}`, received `{actual}`"
        ));
    }
    Ok(())
}

fn native_status_error(status: i32) -> String {
    match status {
        1 => "error[native_export_missing]: unknown native export id".to_string(),
        2 => "error[native_export_arity]: native export arity mismatch".to_string(),
        3 => "error[arithmetic_overflow]: native integer operation overflowed".to_string(),
        4 => "error[division_by_zero]: native integer operation cannot divide by zero".to_string(),
        5 => "error[if_clause]: no native if condition matched".to_string(),
        7 => "error[native_transition_capacity]: native transition buffer is too small".to_string(),
        18 => "error[arithmetic_overflow]: native Float operation produced a non-finite value"
            .to_string(),
        19 => "error[division_by_zero]: native Float operation cannot divide by zero".to_string(),
        crate::runtime::native_image::managed::MANAGED_ALLOCATION_FAILED_STATUS => {
            "error[managed_allocation]: native managed allocation failed".to_string()
        }
        status => format!("error[native_status]: native export returned unknown status {status}"),
    }
}

#[cfg(test)]
#[path = "pure_native_transport_test.rs"]
#[cfg(test)]
mod pure_native_transport_test;
