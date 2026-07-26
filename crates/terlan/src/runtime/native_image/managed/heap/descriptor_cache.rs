//! Owner-local immutable managed-type descriptor reuse.

use std::cell::RefCell;
use std::sync::Arc;

use super::{AllocationClass, ManagedTypeDescriptor, SemanticTypeId};

pub(super) const MAX_CACHED_SEQUENCE_DESCRIPTORS: usize = 64;
pub(super) const MAX_CACHED_SPECIALIZED_DESCRIPTORS: usize = 64;
pub(super) const MAX_CACHED_AGGREGATE_DESCRIPTORS: usize = 64;

thread_local! {
    pub(super) static OWNER_DESCRIPTOR_CACHE: RefCell<OwnerDescriptorCache> =
        RefCell::new(OwnerDescriptorCache::default());
}

/// Immutable sequence layout retained by one shard-owner thread.
#[derive(Clone, Debug)]
pub(super) struct SequenceDescriptorCacheEntry {
    pub(super) semantic: SemanticTypeId,
    pub(super) size: usize,
    pub(super) allocation_class: AllocationClass,
    pub(super) descriptor: Arc<ManagedTypeDescriptor>,
}

/// Immutable specialized layout retained by one shard-owner thread.
#[derive(Clone, Debug)]
pub(super) struct SpecializedDescriptorCacheEntry {
    pub(super) semantic: SemanticTypeId,
    pub(super) size: usize,
    pub(super) alignment: usize,
    pub(super) allocation_class: AllocationClass,
    pub(super) representation: Box<[u8]>,
    pub(super) descriptor: Arc<ManagedTypeDescriptor>,
}

#[derive(Default)]
pub(super) struct OwnerDescriptorCache {
    pub(super) sequence: Vec<SequenceDescriptorCacheEntry>,
    pub(super) next_sequence: usize,
    pub(super) specialized: Vec<SpecializedDescriptorCacheEntry>,
    pub(super) next_specialized: usize,
    pub(super) aggregate: Vec<Arc<ManagedTypeDescriptor>>,
}
