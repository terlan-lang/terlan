//! Shard-owned mutable state for direct native actor execution.

use std::collections::BTreeMap;

use crate::runtime::native_image::managed::{
    CollectionStats, ManagedExecutionRuntime, PendingManagedCaptures,
};
use crate::runtime::native_image::TvmBoundaryType;

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
    pub(crate) fn into_resume_state(
        self,
    ) -> (Option<TvmBoundaryType>, Option<PendingManagedCaptures>) {
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
    /// Next nonzero request identity allocated inside this shard.
    next_request_id: u64,
}

impl PureNativeExecutionRuntime {
    /// Creates empty execution state around admitted managed-image metadata.
    pub(crate) fn from_managed(managed: ManagedExecutionRuntime) -> Self {
        Self {
            managed,
            continuations: BTreeMap::new(),
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

    /// Allocates one nonzero request identity local to this execution shard.
    pub(crate) fn allocate_request_id(&mut self) -> Result<u64, String> {
        self.next_request_id = self.next_request_id.checked_add(1).ok_or_else(|| {
            "error[execution_shard.request_identity]: request identities exhausted".to_string()
        })?;
        Ok(self.next_request_id)
    }

    /// Parks one generated continuation under its exact actor owner.
    pub(crate) fn park_continuation(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        injected_type: Option<TvmBoundaryType>,
        managed: Option<PendingManagedCaptures>,
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
            },
        );
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
        })
    }

    /// Releases one actor's continuation, managed heap, and mailbox roots.
    pub(crate) fn release_owner(&mut self, owner_id: u64) {
        self.continuations.remove(&owner_id);
        self.managed.release_owner(owner_id);
    }

    /// Clears request-local state without removing a live service actor's heap.
    pub(crate) fn reset_owner(&mut self, owner_id: u64) {
        self.continuations.remove(&owner_id);
        self.managed.reset_owner(owner_id);
    }

    /// Compacts one live native actor around its precise parked roots.
    #[allow(dead_code)] // Explicit owner-loop hook; not every embedding exposes hibernation.
    pub(crate) fn hibernate_owner(
        &mut self,
        owner_id: u64,
    ) -> Result<Option<CollectionStats>, String> {
        let pending = self
            .continuations
            .get_mut(&owner_id)
            .and_then(|continuation| continuation.managed.as_mut());
        self.managed.hibernate_owner(owner_id, pending)
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
mod execution_runtime_access_test;
#[cfg(test)]
#[path = "execution_runtime_actor_transfer_test.rs"]
mod execution_runtime_actor_transfer_test;
