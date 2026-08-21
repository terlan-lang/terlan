//! Shard-owned mutable state for direct native actor execution.

use std::collections::BTreeMap;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::managed::{ManagedExecutionRuntime, PendingManagedCaptures};
use crate::runtime::native_image::TvmBoundaryType;

use super::PureNativeSuspension;

#[path = "execution_runtime/actor_transfer.rs"]
mod actor_transfer;

pub(crate) use actor_transfer::{
    PureNativeActorExecutionImportFailure, PureNativeActorExecutionTransfer,
};

/// One actor's backend continuation metadata retained outside immutable code.
#[derive(Debug)]
struct PendingNativeContinuation {
    /// Correlation identity of the native call that parked.
    request_id: u64,
    /// Stable generated continuation entry identity.
    continuation_id: u64,
    /// Exact runtime-injected value type, when the operation produces one.
    injected_type: Option<TvmBoundaryType>,
    /// Precise managed roots retained outside scalar transition values.
    managed: Option<PendingManagedCaptures>,
    /// Caller completion frames ordered from innermost to outermost.
    completions: Vec<PendingNativeCompletionFrame>,
}

/// One VM-owned caller frame retained independently of generated code.
#[derive(Debug)]
pub(crate) struct PendingNativeCompletionFrame {
    pub(crate) continuation_id: u64,
    pub(crate) scalar_captures: Vec<i64>,
    pub(crate) managed: Option<PendingManagedCaptures>,
}

/// Linear authority to resume one exact parked native continuation.
///
/// A claim exists only after its pending record has been removed from the
/// shard-owned table. Moving this value into restoration therefore makes a
/// second resume impossible without parking a new continuation.
#[derive(Debug)]
pub(crate) struct NativeContinuationClaim {
    /// Actor that exclusively owns the continuation.
    owner_id: u64,
    /// Correlation identity of the call that parked.
    request_id: u64,
    /// Stable generated continuation entry identity.
    continuation_id: u64,
    /// Exact runtime-injected value type, when present.
    injected_type: Option<TvmBoundaryType>,
    /// Precise managed roots retained while the actor was parked.
    managed: Option<PendingManagedCaptures>,
    completions: Vec<PendingNativeCompletionFrame>,
}

impl NativeContinuationClaim {
    /// Returns the actor authorized to consume this claim.
    pub(crate) const fn owner_id(&self) -> u64 {
        self.owner_id
    }

    /// Returns the call identity authorized by this claim.
    pub(crate) const fn request_id(&self) -> u64 {
        self.request_id
    }

    /// Returns the generated continuation identity authorized by this claim.
    pub(crate) const fn continuation_id(&self) -> u64 {
        self.continuation_id
    }

    /// Consumes the claim into its injected type and precise managed roots.
    pub(crate) fn into_resume_state_with_completions(
        self,
    ) -> (
        Option<TvmBoundaryType>,
        Option<PendingManagedCaptures>,
        Vec<PendingNativeCompletionFrame>,
    ) {
        (self.injected_type, self.managed, self.completions)
    }

    #[cfg(test)]
    pub(crate) fn into_resume_state(
        self,
    ) -> (Option<TvmBoundaryType>, Option<PendingManagedCaptures>) {
        debug_assert!(self.completions.is_empty());
        (self.injected_type, self.managed)
    }
}

/// Mutable actor heaps and continuations exclusively owned by one execution shard.
#[derive(Debug)]
pub(crate) struct PureNativeExecutionRuntime {
    /// Actor-local managed heaps and mailbox roots.
    managed: ManagedExecutionRuntime,
    /// At most one parked generated continuation per actor identity.
    continuations: BTreeMap<u64, PendingNativeContinuation>,
    /// Scheduler-owned suspension programs for independently spawned actors.
    resident_suspensions: BTreeMap<u64, PureNativeSuspension>,
    /// Next nonzero request identity allocated inside this shard.
    next_request_id: u64,
}

impl PureNativeExecutionRuntime {
    /// Creates empty execution state around admitted managed-image metadata.
    pub(crate) fn from_managed(managed: ManagedExecutionRuntime) -> Self {
        Self {
            managed,
            continuations: BTreeMap::new(),
            resident_suspensions: BTreeMap::new(),
            next_request_id: 0,
        }
    }

    /// Creates an empty shard state sharing only immutable admitted layouts.
    pub(crate) fn fork_empty(&self) -> Self {
        Self::from_managed(self.managed.fork_empty())
    }

    /// Borrows actor-local managed heaps and mailbox roots mutably.
    pub(crate) fn managed(&mut self) -> &mut ManagedExecutionRuntime {
        &mut self.managed
    }

    /// Borrows actor-local managed heaps and mailbox roots immutably.
    pub(crate) fn managed_ref(&self) -> &ManagedExecutionRuntime {
        &self.managed
    }

    /// Returns the number of independently parked actor continuations.
    pub(crate) fn pending_continuation_count(&self) -> usize {
        self.continuations.len()
    }

    /// Retains one independently spawned actor at its scheduler-visible park point.
    pub(crate) fn park_resident_suspension(
        &mut self,
        suspension: PureNativeSuspension,
    ) -> Result<(), String> {
        let owner_id = suspension.owner_id();
        if self
            .resident_suspensions
            .insert(owner_id, suspension)
            .is_some()
        {
            return Err(format!(
                "error[execution_shard.resident_pending]: actor {owner_id} already owns a resident suspension"
            ));
        }
        Ok(())
    }

    /// Claims a spawned actor suspension for one owner-loop execution slice.
    pub(crate) fn take_resident_suspension(
        &mut self,
        owner_id: u64,
    ) -> Option<PureNativeSuspension> {
        self.resident_suspensions.remove(&owner_id)
    }

    /// Claims the lowest-owner resident capability suspension for external service.
    pub(crate) fn take_resident_capability_suspension(&mut self) -> Option<PureNativeSuspension> {
        let owner_id = self
            .resident_suspensions
            .iter()
            .find_map(|(owner_id, suspension)| {
                (suspension.operation() == TvmTransitionOperation::Capability).then_some(*owner_id)
            })?;
        self.resident_suspensions.remove(&owner_id)
    }

    /// Reconstructs parked capture words without consuming resume authority.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn debugger_continuation_capture_words(
        &self,
        owner_id: u64,
        continuation_id: u64,
        types: &[TvmBoundaryType],
        transported: &[i64],
    ) -> Result<Vec<i64>, String> {
        let pending = self.continuations.get(&owner_id).ok_or_else(|| {
            format!(
                "error[vm.debugger.continuation_missing]: continuation {continuation_id} is not parked for actor {owner_id}"
            )
        })?;
        if pending.continuation_id != continuation_id {
            return Err(format!(
                "error[vm.debugger.continuation_identity]: actor {owner_id} owns continuation {}, not {continuation_id}",
                pending.continuation_id
            ));
        }
        self.managed.snapshot_continuation_captures(
            owner_id,
            continuation_id,
            types,
            transported,
            pending.managed.as_ref(),
        )
    }

    /// Allocates one nonzero request identity local to this execution shard.
    pub(crate) fn allocate_request_id(&mut self) -> Result<u64, String> {
        self.next_request_id = self.next_request_id.checked_add(1).ok_or_else(|| {
            "error[execution_shard.request_identity]: request identities exhausted".to_string()
        })?;
        Ok(self.next_request_id)
    }

    /// Parks one generated continuation under its exact actor owner.
    #[cfg(test)]
    pub(crate) fn park_continuation(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        injected_type: Option<TvmBoundaryType>,
        managed: Option<PendingManagedCaptures>,
    ) -> Result<(), String> {
        self.park_continuation_with_completions(
            owner_id,
            request_id,
            continuation_id,
            injected_type,
            managed,
            Vec::new(),
        )
    }

    /// Parks a continuation together with its VM-owned completion stack.
    pub(crate) fn park_continuation_with_completions(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        injected_type: Option<TvmBoundaryType>,
        managed: Option<PendingManagedCaptures>,
        completions: Vec<PendingNativeCompletionFrame>,
    ) -> Result<(), String> {
        if self.continuations.contains_key(&owner_id) {
            return Err(format!(
                "error[execution_shard.continuation_pending]: actor {owner_id} yielded while another continuation was parked"
            ));
        }
        self.continuations.insert(
            owner_id,
            PendingNativeContinuation {
                request_id,
                continuation_id,
                injected_type,
                managed,
                completions,
            },
        );
        Ok(())
    }

    /// Collects an actor after transition arguments have been decoded and only
    /// precise continuation/mailbox roots remain live.
    pub(crate) fn collect_parked_owner_at_safepoint(
        &mut self,
        owner_id: u64,
    ) -> Result<(), String> {
        let pending = self.continuations.get_mut(&owner_id).ok_or_else(|| {
            format!(
                "error[execution_shard.continuation_stale]: actor {owner_id} has no parked continuation to collect"
            )
        })?;
        let mut roots = pending
            .managed
            .iter_mut()
            .chain(
                pending
                    .completions
                    .iter_mut()
                    .filter_map(|frame| frame.managed.as_mut()),
            )
            .collect::<Vec<_>>();
        self.managed
            .collect_owner_with_continuation_stack(owner_id, &mut roots)?;
        Ok(())
    }

    /// Claims one continuation only when actor, request, and entry identities match.
    pub(crate) fn claim_continuation(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
    ) -> Result<NativeContinuationClaim, String> {
        let pending = self.continuations.get(&owner_id).ok_or_else(|| {
            format!(
                "error[execution_shard.continuation_stale]: continuation {continuation_id} is not parked for actor {owner_id}"
            )
        })?;
        if (pending.request_id, pending.continuation_id) != (request_id, continuation_id) {
            return Err(format!(
                "error[execution_shard.continuation_identity]: resume ({request_id}, {owner_id}, {continuation_id}) does not own the parked continuation"
            ));
        }
        let pending = self
            .continuations
            .remove(&owner_id)
            .expect("validated continuation remains present until removal");
        Ok(NativeContinuationClaim {
            owner_id,
            request_id: pending.request_id,
            continuation_id: pending.continuation_id,
            injected_type: pending.injected_type,
            managed: pending.managed,
            completions: pending.completions,
        })
    }

    /// Releases one actor's continuation, managed heap, and mailbox roots.
    pub(crate) fn release_owner(&mut self, owner_id: u64) {
        self.continuations.remove(&owner_id);
        self.resident_suspensions.remove(&owner_id);
        self.managed.release_owner(owner_id);
    }

    /// Clears request-local state without removing a live service actor's heap.
    pub(crate) fn reset_owner(&mut self, owner_id: u64) {
        self.continuations.remove(&owner_id);
        self.resident_suspensions.remove(&owner_id);
        self.managed.reset_owner(owner_id);
    }

    /// Rejects graceful shard shutdown while any actor remains parked.
    #[cfg(test)]
    pub(crate) fn ensure_idle(&self) -> Result<(), String> {
        if self.continuations.is_empty() {
            return Ok(());
        }
        Err(format!(
            "error[execution_shard.continuation_pending]: cannot shut down with {} parked continuation(s)",
            self.continuations.len()
        ))
    }
}

#[cfg(test)]
#[path = "execution_runtime_access_test.rs"]
#[cfg(test)]
mod execution_runtime_access_test;
#[cfg(test)]
#[path = "execution_runtime_actor_transfer_test.rs"]
#[cfg(test)]
mod execution_runtime_actor_transfer_test;
