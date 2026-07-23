//! Owner-local heap lookup optimized for the fixed service actor.

use std::collections::HashMap;

use super::ActorHeap;

/// Actor heaps with a direct nullable slot for the common single-owner shard.
///
/// Additional live actors retain keyed lookup without forcing the persistent
/// service actor through hashing on every managed allocation and response
/// materialization.
#[derive(Debug, Default)]
pub(super) struct ManagedOwnerHeaps {
    primary: Option<(u64, ActorHeap)>,
    overflow: HashMap<u64, ActorHeap>,
}

impl ManagedOwnerHeaps {
    pub(super) fn contains_key(&self, owner: &u64) -> bool {
        self.primary
            .as_ref()
            .is_some_and(|(primary, _)| primary == owner)
            || self.overflow.contains_key(owner)
    }

    pub(super) fn get(&self, owner: &u64) -> Option<&ActorHeap> {
        match self.primary.as_ref() {
            Some((primary, heap)) if primary == owner => Some(heap),
            _ => self.overflow.get(owner),
        }
    }

    pub(super) fn get_mut(&mut self, owner: &u64) -> Option<&mut ActorHeap> {
        match self.primary.as_mut() {
            Some((primary, heap)) if primary == owner => Some(heap),
            _ => self.overflow.get_mut(owner),
        }
    }

    pub(super) fn insert(&mut self, owner: u64, heap: ActorHeap) -> Option<ActorHeap> {
        match self.primary.as_mut() {
            Some((primary, existing)) if *primary == owner => {
                Some(std::mem::replace(existing, heap))
            }
            None => {
                self.primary = Some((owner, heap));
                None
            }
            Some(_) => self.overflow.insert(owner, heap),
        }
    }

    pub(super) fn remove(&mut self, owner: &u64) -> Option<ActorHeap> {
        if self
            .primary
            .as_ref()
            .is_some_and(|(primary, _)| primary == owner)
        {
            return self.primary.take().map(|(_, heap)| heap);
        }
        self.overflow.remove(owner)
    }

    pub(super) fn len(&self) -> usize {
        usize::from(self.primary.is_some()) + self.overflow.len()
    }
}
