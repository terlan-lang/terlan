//! Shared, indexed metadata from an already admitted native image.

use std::sync::Arc;

use crate::runtime::native_image::TvmContinuationDescriptor;

/// Immutable signatures shared by image forks, call caches and parked actors.
#[derive(Clone, Debug)]
pub(super) struct NativeContinuationTable(Arc<[TvmContinuationDescriptor]>);

impl From<Vec<TvmContinuationDescriptor>> for NativeContinuationTable {
    /// Index already admitted unique IDs without changing the sealed descriptor.
    fn from(mut entries: Vec<TvmContinuationDescriptor>) -> Self {
        entries.sort_unstable_by_key(|entry| entry.id);
        Self(entries.into())
    }
}

impl NativeContinuationTable {
    /// Enumerates stable IDs for admission and generation diagnostics.
    pub(super) fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.0.iter().map(|entry| entry.id)
    }

    /// Resolves a stable ID without scanning or copying the image's signatures.
    pub(super) fn get(&self, id: u64) -> Option<&TvmContinuationDescriptor> {
        self.0
            .binary_search_by_key(&id, |entry| entry.id)
            .ok()
            .map(|index| &self.0[index])
    }
}

#[cfg(test)]
#[path = "continuation_table_test.rs"]
mod tests;
