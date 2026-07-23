//! Linear queued-actor claims for scheduler work stealing.

use std::sync::atomic::Ordering;

use super::{
    pack_state, unpack_state, VmActorDirectory, VmActorDirectoryError, VmActorHandle,
    VmActorLifecycle,
};
use crate::runtime::vm::process::VmProcessId;

/// Generation-qualified authority over one queued actor removed for stealing.
#[derive(Debug)]
#[must_use = "a steal claim must be transferred or explicitly rolled back"]
pub(crate) struct VmActorStealClaim {
    handle: VmActorHandle,
    owner_generation: u64,
}

impl VmActorStealClaim {
    /// Returns the exact actor process controlled by this claim.
    pub(crate) fn process_id(&self) -> VmProcessId {
        self.handle.pid()
    }
}

impl<T, P> VmActorDirectory<T, P> {
    /// Atomically removes one fully published queued actor from execution eligibility.
    pub(crate) fn claim_queued_for_steal(
        &self,
        pid: VmProcessId,
    ) -> Result<VmActorStealClaim, VmActorDirectoryError> {
        let cell = self.cell(pid)?;
        let pins = cell.lookup_pins.load(Ordering::Acquire);
        if pins != 0 {
            return Err(VmActorDirectoryError::LookupPinned(pins));
        }
        let pending = cell.mailbox.len();
        if pending != 0 {
            return Err(VmActorDirectoryError::TransferMailboxNotDrained { pending });
        }
        let before_word = cell.state.load(Ordering::Acquire);
        let before = unpack_state(before_word)?;
        if before.owner != 0 {
            return Err(VmActorDirectoryError::AlreadyOwned {
                owner: before.owner,
                owner_generation: before.owner_generation,
            });
        }
        if before.lifecycle != VmActorLifecycle::Queued {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: before.lifecycle,
                to: VmActorLifecycle::Migrating,
            });
        }
        let after_word = pack_state(
            VmActorLifecycle::Migrating,
            before.actor_generation,
            before.owner_generation,
            0,
        );
        cell.state
            .compare_exchange(before_word, after_word, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|observed| super::ownership_race_error(observed))?;
        self.record_transition(
            cell.handle,
            VmActorLifecycle::Queued,
            VmActorLifecycle::Migrating,
            0,
            before.owner_generation,
        );
        let claim = VmActorStealClaim {
            handle: cell.handle,
            owner_generation: before.owner_generation,
        };
        let pins = cell.lookup_pins.load(Ordering::Acquire);
        if pins != 0 {
            self.abort_steal_claim(claim)?;
            return Err(VmActorDirectoryError::LookupPinned(pins));
        }
        let pending = cell.mailbox.len();
        if pending != 0 {
            self.abort_steal_claim(claim)?;
            return Err(VmActorDirectoryError::TransferMailboxNotDrained { pending });
        }
        Ok(claim)
    }

    /// Restores one rejected steal to its exact queued actor generation.
    pub(crate) fn abort_steal_claim(
        &self,
        claim: VmActorStealClaim,
    ) -> Result<(), VmActorDirectoryError> {
        self.publish_steal_claim(&claim)
    }

    /// Publishes one accepted steal claim as queued on its destination owner.
    pub(crate) fn complete_steal_claim(
        &self,
        claim: VmActorStealClaim,
    ) -> Result<(), (VmActorDirectoryError, VmActorStealClaim)> {
        self.publish_steal_claim(&claim)
            .map_err(|error| (error, claim))
    }

    /// Consumes linear migration authority into one fully published queue state.
    fn publish_steal_claim(&self, claim: &VmActorStealClaim) -> Result<(), VmActorDirectoryError> {
        let cell = self.cell_for_handle(claim.handle)?;
        let before_word = cell.state.load(Ordering::Acquire);
        let before = unpack_state(before_word)?;
        if before.lifecycle != VmActorLifecycle::Migrating
            || before.owner != 0
            || before.owner_generation != claim.owner_generation
        {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: before.lifecycle,
                to: VmActorLifecycle::Queued,
            });
        }
        let after_word = pack_state(
            VmActorLifecycle::Queued,
            before.actor_generation,
            before.owner_generation,
            0,
        );
        cell.state
            .compare_exchange(before_word, after_word, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|observed| super::ownership_race_error(observed))?;
        self.record_transition(
            cell.handle,
            VmActorLifecycle::Migrating,
            VmActorLifecycle::Queued,
            0,
            before.owner_generation,
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "steal_test.rs"]
mod steal_test;
