//! Thread-neutral state retained while direct native execution is parked.

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::{TvmBoundaryType, TvmContinuationDescriptor};
use crate::runtime::vm::actor::VmNativeTraceCall;

use super::NativeResultProjection;

/// Stable identities required to resume one exact native call.
#[derive(Debug)]
pub(super) struct NativeContinuationIdentity {
    pub(super) request_id: u64,
    pub(super) owner_id: u64,
    pub(super) continuation_id: u64,
}

/// Owned scheduler operation and words retained independently of native stack memory.
#[derive(Debug)]
pub(super) struct OwnedNativeTransition {
    pub(super) operation: TvmTransitionOperation,
    pub(super) arguments: Vec<i64>,
    pub(super) values: Vec<i64>,
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(super) capture_types: Vec<TvmBoundaryType>,
}

/// Immutable image metadata required to validate subsequent resume entries.
#[derive(Debug)]
pub(super) struct OwnedNativeResumeProgram {
    pub(super) result_type: TvmBoundaryType,
    pub(super) result_projection: NativeResultProjection,
    pub(super) continuations: Vec<TvmContinuationDescriptor>,
    pub(super) resume_count: usize,
}

/// Owned native continuation state retained while its VM actor is parked.
///
/// This type deliberately has no lifetime, pointer, scheduler handle, worker
/// connection, or cache parameter. Moving it between scheduler threads moves
/// only stable identities and owned VM values.
#[derive(Debug)]
pub(crate) struct PureNativeSuspension {
    identity: NativeContinuationIdentity,
    transition: OwnedNativeTransition,
    resume: OwnedNativeResumeProgram,
    trace_call: VmNativeTraceCall,
}

/// Consumed continuation state needed after its VM transition completes.
pub(super) struct NativeResumeState {
    pub(super) request_id: u64,
    pub(super) owner_id: u64,
    pub(super) values: Vec<i64>,
    pub(super) result_type: TvmBoundaryType,
    pub(super) result_projection: NativeResultProjection,
    pub(super) continuations: Vec<TvmContinuationDescriptor>,
    pub(super) resume_count: usize,
    pub(super) trace_call: VmNativeTraceCall,
}

impl PureNativeSuspension {
    /// Captures one validated transition into scheduler-independent owned state.
    pub(super) fn new(
        identity: NativeContinuationIdentity,
        transition: OwnedNativeTransition,
        resume: OwnedNativeResumeProgram,
        trace_call: VmNativeTraceCall,
    ) -> Self {
        Self {
            identity,
            transition,
            resume,
            trace_call,
        }
    }

    /// Returns the stable native request identity.
    pub(crate) fn request_id(&self) -> u64 {
        self.identity.request_id
    }

    /// Returns the stable VM actor identity.
    pub(super) fn owner_id(&self) -> u64 {
        self.identity.owner_id
    }

    /// Returns the stable generated continuation identity.
    pub(crate) fn continuation_id(&self) -> u64 {
        self.identity.continuation_id
    }

    /// Returns the scheduler operation that caused this suspension.
    pub(crate) fn operation(&self) -> TvmTransitionOperation {
        self.transition.operation.clone()
    }

    /// Returns operation arguments kept separate from continuation captures.
    pub(crate) fn arguments(&self) -> &[i64] {
        &self.transition.arguments
    }

    /// Returns pointer-free continuation captures for debugger inspection.
    ///
    /// Slots remain encoded native values because managed heap ownership stays
    /// with the execution shard; debugger rendering must not dereference them.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn debugger_capture_slots(&self) -> &[i64] {
        &self.transition.values
    }

    /// Returns descriptor-directed types in generated capture order.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn debugger_capture_types(&self) -> &[TvmBoundaryType] {
        &self.transition.capture_types
    }

    /// Consumes a completed transition into the data needed for native resume.
    pub(super) fn into_resume_state(self) -> NativeResumeState {
        NativeResumeState {
            request_id: self.identity.request_id,
            owner_id: self.identity.owner_id,
            values: self.transition.values,
            result_type: self.resume.result_type,
            result_projection: self.resume.result_projection,
            continuations: self.resume.continuations,
            resume_count: self.resume.resume_count,
            trace_call: self.trace_call,
        }
    }
}

#[cfg(test)]
#[path = "thread_neutral_test.rs"]
#[cfg(test)]
mod thread_neutral_test;
