//! Thread-neutral state retained while direct native execution is parked.

use std::collections::HashSet;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::{TvmBoundaryType, TvmContinuationDescriptor};
use crate::runtime::vm::actor::VmNativeTraceCall;

/// Stable identities required to resume one exact native call.
#[derive(Debug)]
struct NativeContinuationIdentity {
    request_id: u64,
    owner_id: u64,
    continuation_id: u64,
}

/// Owned scheduler operation and words retained independently of native stack memory.
#[derive(Debug)]
struct OwnedNativeTransition {
    operation: TvmTransitionOperation,
    arguments: Vec<i64>,
    values: Vec<i64>,
}

/// Immutable image metadata required to validate subsequent resume entries.
#[derive(Debug)]
struct OwnedNativeResumeProgram {
    result_type: TvmBoundaryType,
    continuations: Vec<TvmContinuationDescriptor>,
    observed_continuations: HashSet<u64>,
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
    pub(super) continuations: Vec<TvmContinuationDescriptor>,
    pub(super) observed_continuations: HashSet<u64>,
    pub(super) trace_call: VmNativeTraceCall,
}

impl PureNativeSuspension {
    /// Captures one validated transition into scheduler-independent owned state.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        request_id: u64,
        owner_id: u64,
        continuation_id: u64,
        operation: TvmTransitionOperation,
        arguments: Vec<i64>,
        values: Vec<i64>,
        result_type: TvmBoundaryType,
        continuations: Vec<TvmContinuationDescriptor>,
        observed_continuations: HashSet<u64>,
        trace_call: VmNativeTraceCall,
    ) -> Self {
        Self {
            identity: NativeContinuationIdentity {
                request_id,
                owner_id,
                continuation_id,
            },
            transition: OwnedNativeTransition {
                operation,
                arguments,
                values,
            },
            resume: OwnedNativeResumeProgram {
                result_type,
                continuations,
                observed_continuations,
            },
            trace_call,
        }
    }

    /// Returns the stable native request identity.
    pub(super) fn request_id(&self) -> u64 {
        self.identity.request_id
    }

    /// Returns the stable VM actor identity.
    pub(super) fn owner_id(&self) -> u64 {
        self.identity.owner_id
    }

    /// Returns the stable generated continuation identity.
    pub(super) fn continuation_id(&self) -> u64 {
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

    /// Consumes a completed transition into the data needed for native resume.
    pub(super) fn into_resume_state(self) -> NativeResumeState {
        NativeResumeState {
            request_id: self.identity.request_id,
            owner_id: self.identity.owner_id,
            values: self.transition.values,
            result_type: self.resume.result_type,
            continuations: self.resume.continuations,
            observed_continuations: self.resume.observed_continuations,
            trace_call: self.trace_call,
        }
    }
}

#[cfg(test)]
#[path = "thread_neutral_test.rs"]
mod thread_neutral_test;
