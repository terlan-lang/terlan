//! Atomic transfer of one actor's managed heap and mailbox roots.

use std::fmt;

use super::{ActorHeap, ActorId, ManagedExecutionRuntime, ManagedMailboxFragment};

/// Complete movable managed state for one actor generation.
#[derive(Debug)]
pub(crate) struct ManagedActorTransfer {
    owner: ActorId,
    heap: Option<ActorHeap>,
    mailbox_fragments: Vec<(u32, ManagedMailboxFragment)>,
}

impl ManagedActorTransfer {
    /// Returns the actor identity shared by the heap and all precise roots.
    pub(crate) fn owner_id(&self) -> u64 {
        self.owner.get()
    }

    /// Returns the transferred heap usage without exposing allocator storage.
    #[cfg(test)]
    pub(crate) fn heap_usage(&self) -> Option<(usize, usize)> {
        self.heap
            .as_ref()
            .map(|heap| (heap.allocated_bytes(), heap.object_count()))
    }

    /// Returns the number of precise mailbox roots transferred with the heap.
    #[cfg(test)]
    pub(crate) fn mailbox_fragment_count(&self) -> usize {
        self.mailbox_fragments.len()
    }
}

/// Failed destination admission that returns the complete transfer unchanged.
#[derive(Debug)]
pub(crate) struct ManagedActorImportFailure {
    reason: String,
    transfer: Box<ManagedActorTransfer>,
}

impl ManagedActorImportFailure {
    /// Returns the stable destination-admission reason.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns actor-state ownership so the source can roll back safely.
    pub(crate) fn into_transfer(self) -> ManagedActorTransfer {
        *self.transfer
    }
}

impl fmt::Display for ManagedActorImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for ManagedActorImportFailure {}

impl ManagedExecutionRuntime {
    /// Detaches one actor heap and all roots that point into it.
    pub(crate) fn detach_actor(&mut self, owner_id: u64) -> Result<ManagedActorTransfer, String> {
        let owner = ActorId::new(owner_id)
            .map_err(|error| format!("error[managed_execution.transfer_owner]: {error}"))?;
        let fragment_ids = self
            .mailbox_fragments
            .iter()
            .filter_map(|(identity, fragment)| (fragment.receiver() == owner).then_some(*identity))
            .collect::<Vec<_>>();
        if !fragment_ids.is_empty() && !self.heaps.contains_key(&owner_id) {
            return Err(
                "error[managed_execution.transfer_roots]: mailbox roots require their actor heap"
                    .to_string(),
            );
        }
        for identity in &fragment_ids {
            let fragment = self
                .mailbox_fragments
                .get(identity)
                .expect("inventoried actor mailbox fragment remains present");
            if *identity == 0 || *identity != fragment.fragment_id() {
                return Err(
                    "error[managed_execution.transfer_fragment]: fragment identity changed"
                        .to_string(),
                );
            }
        }
        let heap = self.heaps.remove(&owner_id);
        let mailbox_fragments = fragment_ids
            .into_iter()
            .map(|identity| {
                let fragment = self
                    .mailbox_fragments
                    .remove(&identity)
                    .expect("inventoried actor mailbox fragment remains present");
                (identity, fragment)
            })
            .collect();
        Ok(ManagedActorTransfer {
            owner,
            heap,
            mailbox_fragments,
        })
    }

    /// Verifies destination ownership without consuming or mutating either side.
    pub(crate) fn validate_actor_import(
        &self,
        transfer: &ManagedActorTransfer,
    ) -> Result<(), String> {
        let owner_id = transfer.owner_id();
        if transfer
            .heap
            .as_ref()
            .is_some_and(|heap| heap.owner() != transfer.owner)
        {
            return Err(
                "error[managed_execution.transfer_heap_owner]: heap owner changed".to_string(),
            );
        }
        if transfer.heap.is_some() && self.heaps.contains_key(&owner_id) {
            return Err(format!(
                "error[managed_execution.transfer_collision]: actor {owner_id} already has a heap"
            ));
        }
        if !transfer.mailbox_fragments.is_empty() && transfer.heap.is_none() {
            return Err(
                "error[managed_execution.transfer_roots]: mailbox roots require their actor heap"
                    .to_string(),
            );
        }
        for (identity, fragment) in &transfer.mailbox_fragments {
            if *identity == 0 || *identity != fragment.fragment_id() {
                return Err(
                    "error[managed_execution.transfer_fragment]: fragment identity changed"
                        .to_string(),
                );
            }
            if fragment.receiver() != transfer.owner {
                return Err(
                    "error[managed_execution.transfer_fragment_owner]: cross-actor mailbox root"
                        .to_string(),
                );
            }
            if self.mailbox_fragments.contains_key(identity) {
                return Err(format!(
                    "error[managed_execution.transfer_fragment_collision]: fragment {identity} already exists"
                ));
            }
        }
        Ok(())
    }

    /// Imports one preflighted actor state or returns it intact for rollback.
    pub(crate) fn import_actor(
        &mut self,
        mut transfer: ManagedActorTransfer,
    ) -> Result<(), ManagedActorImportFailure> {
        if let Err(reason) = self.validate_actor_import(&transfer) {
            return Err(ManagedActorImportFailure {
                reason,
                transfer: Box::new(transfer),
            });
        }
        let owner_id = transfer.owner_id();
        if let Some(heap) = transfer.heap.take() {
            self.heaps.insert(owner_id, heap);
        }
        for (identity, fragment) in transfer.mailbox_fragments.drain(..) {
            self.next_mailbox_fragment_id = self.next_mailbox_fragment_id.max(identity);
            self.mailbox_fragments.insert(identity, fragment);
        }
        Ok(())
    }
}
