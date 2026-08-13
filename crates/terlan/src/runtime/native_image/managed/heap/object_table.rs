use std::sync::Arc;

use super::ManagedTypeDescriptor;

#[derive(Clone, Debug)]
pub(super) struct ObjectMetadata {
    pub(super) descriptor: Arc<ManagedTypeDescriptor>,
}

/// Offset-ordered metadata for append-only semispace objects.
///
/// Managed allocation is monotonic until collection or rollback, so a tree
/// node per object only adds allocator traffic to the actor hot path. A compact
/// vector preserves ordered lookup and precise traversal without per-object
/// host allocations.
#[derive(Clone, Debug, Default)]
pub(super) struct ObjectTable {
    pub(super) entries: Vec<(usize, ObjectMetadata)>,
}

impl ObjectTable {
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn get(&self, offset: &usize) -> Option<&ObjectMetadata> {
        if let Some((last_offset, metadata)) = self.entries.last() {
            if last_offset == offset {
                return Some(metadata);
            }
        }
        self.entries
            .binary_search_by_key(offset, |(object_offset, _)| *object_offset)
            .ok()
            .map(|index| &self.entries[index].1)
    }

    pub(super) fn contains_key(&self, offset: &usize) -> bool {
        self.get(offset).is_some()
    }

    pub(super) fn insert(&mut self, offset: usize, metadata: ObjectMetadata) {
        debug_assert!(
            self.entries.last().is_none_or(|(prior, _)| *prior < offset),
            "managed objects must be appended in offset order"
        );
        self.entries.push((offset, metadata));
    }

    pub(super) fn truncate_from(&mut self, offset: usize) {
        let retained = self
            .entries
            .partition_point(|(object_offset, _)| *object_offset < offset);
        self.entries.truncate(retained);
    }
}
