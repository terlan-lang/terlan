use super::{
    ManagedExecutionRuntime, ManagedRoot, PendingManagedCaptures, DEFAULT_HARD_HEAP_BYTES,
};
use crate::runtime::native_image::managed::CollectionStats;

const HIBERNATION_COLLECTION_WORK_BYTES: usize = DEFAULT_HARD_HEAP_BYTES * 2;

impl ManagedExecutionRuntime {
    /// Precisely compacts one live actor to parked continuation and mailbox roots.
    pub(crate) fn hibernate_owner(
        &mut self,
        owner_id: u64,
        pending: Option<&mut PendingManagedCaptures>,
    ) -> Result<Option<CollectionStats>, String> {
        if pending
            .as_ref()
            .is_some_and(|captures| captures.owner.get() != owner_id)
        {
            return Err(format!(
                "error[managed_execution.hibernate_owner]: continuation roots do not belong to actor {owner_id}"
            ));
        }
        let Some(mut heap) = self.heaps.remove(&owner_id) else {
            if pending.is_some() {
                return Err(format!(
                    "error[managed_execution.hibernate_heap]: actor {owner_id} has continuation roots but no managed heap"
                ));
            }
            return Ok(None);
        };

        let pending_root_count = pending
            .as_ref()
            .map_or(0, |captures| captures.continuation.captures().len());
        let mut fragment_ids = self
            .mailbox_fragments
            .iter()
            .filter_map(|(fragment_id, fragment)| {
                (fragment.receiver().get() == owner_id).then_some(*fragment_id)
            })
            .collect::<Vec<_>>();
        fragment_ids.sort_unstable();
        let mut roots = Vec::<ManagedRoot>::with_capacity(pending_root_count + fragment_ids.len());
        if let Some(captures) = pending.as_ref() {
            roots.extend_from_slice(captures.continuation.captures());
        }
        for fragment_id in &fragment_ids {
            roots.push(
                self.mailbox_fragments
                    .get(fragment_id)
                    .expect("collected fragment identity remains registered")
                    .root()
                    .clone(),
            );
        }

        let collection = match heap.collect(&mut roots, HIBERNATION_COLLECTION_WORK_BYTES) {
            Ok(collection) => collection,
            Err(error) => {
                self.heaps.insert(owner_id, heap);
                return Err(format!(
                    "error[managed_execution.hibernate_collection]: {error}"
                ));
            }
        };
        if let Some(captures) = pending {
            captures
                .continuation
                .captures_mut()
                .clone_from_slice(&roots[..pending_root_count]);
        }
        for (offset, fragment_id) in fragment_ids.iter().enumerate() {
            self.mailbox_fragments
                .get_mut(fragment_id)
                .expect("collected fragment identity remains registered")
                .roots_mut()[0] = roots[pending_root_count + offset].clone();
        }
        self.heaps.insert(owner_id, heap);
        Ok(Some(collection))
    }
}
