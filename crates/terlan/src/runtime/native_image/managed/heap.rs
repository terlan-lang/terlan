//! Actor-local bump allocation and precise semispace collection.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use bytes::Bytes;

use super::{
    ActorId, AllocationClass, ManagedMailboxFragment, ManagedMemoryError, ManagedRoot,
    ManagedTypeDescriptor, RootLocation, SemanticTypeId, TvmRef,
};

#[path = "heap/descriptor_cache.rs"]
mod descriptor_cache;
#[path = "heap/support.rs"]
mod support;
use descriptor_cache::{
    SequenceDescriptorCacheEntry, SpecializedDescriptorCacheEntry,
    MAX_CACHED_AGGREGATE_DESCRIPTORS, MAX_CACHED_SEQUENCE_DESCRIPTORS,
    MAX_CACHED_SPECIALIZED_DESCRIPTORS, OWNER_DESCRIPTOR_CACHE,
};
use support::{
    align_up, next_token, read_reference, reference_with_token, relocate_object_fields,
    write_reference,
};
pub use support::{CollectionStats, HeapLimits};

const TOKEN_SHIFT: usize = 32;
const OFFSET_MASK: usize = u32::MAX as usize;
const METADATA_WORK_BYTES: usize = 32;
const INITIAL_HEAP_BYTES: usize = 8 * 1024;
const MAX_RETAINED_REUSE_BYTES: usize = 256 * 1024;
const REUSE_UNDERUTILIZED_BYTES: usize = 64 * 1024;
const REUSE_UNDERUTILIZED_LIMIT: u8 = 8;
const TOKEN_RESERVATION_SIZE: u32 = 1_024;
static NEXT_HEAP_TOKEN: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug)]
struct ObjectMetadata {
    descriptor: Arc<ManagedTypeDescriptor>,
}

/// Offset-ordered metadata for append-only semispace objects.
///
/// Managed allocation is monotonic until collection or rollback, so a tree
/// node per object only adds allocator traffic to the actor hot path. A compact
/// vector preserves ordered lookup and precise traversal without per-object
/// host allocations.
#[derive(Clone, Debug, Default)]
struct ObjectTable {
    entries: Vec<(usize, ObjectMetadata)>,
}

impl ObjectTable {
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn get(&self, offset: &usize) -> Option<&ObjectMetadata> {
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

    fn contains_key(&self, offset: &usize) -> bool {
        self.get(offset).is_some()
    }

    fn insert(&mut self, offset: usize, metadata: ObjectMetadata) {
        debug_assert!(
            self.entries.last().is_none_or(|(prior, _)| *prior < offset),
            "managed objects must be appended in offset order"
        );
        self.entries.push((offset, metadata));
    }

    fn truncate_from(&mut self, offset: usize) {
        let retained = self
            .entries
            .partition_point(|(object_offset, _)| *object_offset < offset);
        self.entries.truncate(retained);
    }
}

/// Independently collectible managed heap owned by exactly one actor.
#[derive(Debug)]
pub struct ActorHeap {
    owner: ActorId,
    token: u32,
    next_reserved_token: u32,
    reserved_tokens_remaining: u32,
    latest_retired_token: Option<u32>,
    retired_tokens: HashSet<u32>,
    limits: HeapLimits,
    space: Vec<u8>,
    objects: ObjectTable,
    external_strings: BTreeMap<usize, Bytes>,
    collections: u64,
    reuse_underutilized_count: u8,
}

impl ActorHeap {
    /// Creates an empty actor-local bump-allocation heap.
    pub fn new(owner: ActorId, limits: HeapLimits) -> Result<Self, ManagedMemoryError> {
        if usize::BITS != 64 {
            return Err(ManagedMemoryError::UnsupportedPointerWidth);
        }
        Ok(Self {
            owner,
            token: next_token(),
            next_reserved_token: 0,
            reserved_tokens_remaining: 0,
            latest_retired_token: None,
            retired_tokens: HashSet::new(),
            limits,
            space: Vec::with_capacity(limits.soft_bytes.min(INITIAL_HEAP_BYTES)),
            objects: ObjectTable::default(),
            external_strings: BTreeMap::new(),
            collections: 0,
            reuse_underutilized_count: 0,
        })
    }

    /// Returns the actor that exclusively owns this heap.
    pub fn owner(&self) -> ActorId {
        self.owner
    }

    /// Returns currently occupied semispace bytes, including alignment padding.
    pub fn allocated_bytes(&self) -> usize {
        self.space.len()
    }

    /// Returns the number of currently allocated objects.
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Returns the number of completed moving collections.
    pub fn collection_count(&self) -> u64 {
        self.collections
    }

    /// Reports whether allocation has crossed the actor's soft limit.
    pub fn should_collect(&self) -> bool {
        self.allocated_bytes() >= self.limits.soft_bytes
    }

    /// Rolls back append-only allocations when a compound allocation fails.
    pub(super) fn with_allocation_transaction<T, E>(
        &mut self,
        allocate: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let space_len = self.space.len();
        match allocate(self) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.space.truncate(space_len);
                self.objects.truncate_from(space_len);
                self.external_strings
                    .retain(|offset, _| *offset < space_len);
                Err(error)
            }
        }
    }

    /// Bump-allocates one immutable object with an exact precise reference map.
    pub fn allocate<T>(
        &mut self,
        descriptor: Arc<ManagedTypeDescriptor>,
        payload: &[u8],
        references: &[(usize, TvmRef<()>)],
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        if payload.len() != descriptor.size() {
            return Err(ManagedMemoryError::LayoutMismatch);
        }
        self.allocate_initialized(descriptor, references, |destination| {
            destination.copy_from_slice(payload);
            Ok(())
        })
    }

    /// Bump-allocates one immutable object directly into its final heap slot.
    pub(super) fn allocate_initialized<T>(
        &mut self,
        descriptor: Arc<ManagedTypeDescriptor>,
        references: &[(usize, TvmRef<()>)],
        initialize: impl FnOnce(&mut [u8]) -> Result<(), ManagedMemoryError>,
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        let expected_offsets = descriptor.reference_offsets();
        if references.len() != expected_offsets.len()
            || references
                .iter()
                .zip(expected_offsets)
                .any(|((supplied, _), expected)| supplied != expected)
        {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        for (_, reference) in references {
            self.resolve_offset(*reference)?;
        }
        let original_len = self.space.len();
        let offset = align_up(original_len, descriptor.alignment())?;
        let end = offset
            .checked_add(descriptor.size())
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        if end > self.limits.hard_bytes || end > OFFSET_MASK {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        self.space.resize(end, 0);
        if let Err(error) = initialize(&mut self.space[offset..end]) {
            self.space.truncate(original_len);
            return Err(error);
        }
        for (reference_offset, reference) in references {
            if let Err(error) = write_reference(
                &mut self.space,
                offset + reference_offset,
                reference.encoded().get(),
            ) {
                self.space.truncate(original_len);
                return Err(error);
            }
        }
        self.objects.insert(offset, ObjectMetadata { descriptor });
        self.reference_for_offset(offset)
    }

    /// Bump-allocates a reference-free object from borrowed byte parts.
    pub(super) fn allocate_reference_free_parts<T>(
        &mut self,
        descriptor: Arc<ManagedTypeDescriptor>,
        parts: &[&[u8]],
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        if !descriptor.reference_offsets().is_empty() {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        let payload_size = parts.iter().try_fold(0_usize, |size, part| {
            size.checked_add(part.len())
                .ok_or(ManagedMemoryError::AllocationLimitExceeded)
        })?;
        if payload_size != descriptor.size() {
            return Err(ManagedMemoryError::LayoutMismatch);
        }
        let offset = align_up(self.space.len(), descriptor.alignment())?;
        let end = offset
            .checked_add(payload_size)
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        if end > self.limits.hard_bytes || end > OFFSET_MASK {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        self.space.resize(offset, 0);
        for part in parts {
            self.space.extend_from_slice(part);
        }
        self.objects.insert(offset, ObjectMetadata { descriptor });
        self.reference_for_offset(offset)
    }

    /// Bump-allocates a reference-free object from fixed bytes and ranges
    /// already owned by this heap. Offset ranges remain valid when reserving
    /// the destination relocates the backing `Vec`.
    pub(super) fn allocate_reference_free_ranges<T>(
        &mut self,
        descriptor: Arc<ManagedTypeDescriptor>,
        prefix: &[u8],
        ranges: &[Range<usize>],
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        if !descriptor.reference_offsets().is_empty() {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        let original_len = self.space.len();
        let payload_size = ranges.iter().try_fold(prefix.len(), |size, range| {
            if range.start > range.end || range.end > original_len {
                return Err(ManagedMemoryError::UnknownReference);
            }
            size.checked_add(range.len())
                .ok_or(ManagedMemoryError::AllocationLimitExceeded)
        })?;
        if payload_size != descriptor.size() {
            return Err(ManagedMemoryError::LayoutMismatch);
        }
        let offset = align_up(original_len, descriptor.alignment())?;
        let end = offset
            .checked_add(payload_size)
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        if end > self.limits.hard_bytes || end > OFFSET_MASK {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        self.space.reserve(end.saturating_sub(original_len));
        self.space.resize(offset, 0);
        self.space.extend_from_slice(prefix);
        for range in ranges {
            self.space.extend_from_within(range.clone());
        }
        self.objects.insert(offset, ObjectMetadata { descriptor });
        self.reference_for_offset(offset)
    }

    /// Bump-allocates a reference-free object from fixed byte parts followed
    /// by ranges already owned by this heap.
    pub(super) fn allocate_reference_free_parts_ranges<T>(
        &mut self,
        descriptor: Arc<ManagedTypeDescriptor>,
        parts: &[&[u8]],
        ranges: &[Range<usize>],
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        if !descriptor.reference_offsets().is_empty() {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        let original_len = self.space.len();
        let fixed_size = parts.iter().try_fold(0_usize, |size, part| {
            size.checked_add(part.len())
                .ok_or(ManagedMemoryError::AllocationLimitExceeded)
        })?;
        let payload_size = ranges.iter().try_fold(fixed_size, |size, range| {
            if range.start > range.end || range.end > original_len {
                return Err(ManagedMemoryError::UnknownReference);
            }
            size.checked_add(range.len())
                .ok_or(ManagedMemoryError::AllocationLimitExceeded)
        })?;
        if payload_size != descriptor.size() {
            return Err(ManagedMemoryError::LayoutMismatch);
        }
        let offset = align_up(original_len, descriptor.alignment())?;
        let end = offset
            .checked_add(payload_size)
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        if end > self.limits.hard_bytes || end > OFFSET_MASK {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        self.space.reserve(end.saturating_sub(original_len));
        self.space.resize(offset, 0);
        for part in parts {
            self.space.extend_from_slice(part);
        }
        for range in ranges {
            self.space.extend_from_within(range.clone());
        }
        self.objects.insert(offset, ObjectMetadata { descriptor });
        self.reference_for_offset(offset)
    }

    /// Returns the absolute payload range for one validated heap reference.
    pub(super) fn payload_range<T>(
        &self,
        value: TvmRef<T>,
    ) -> Result<Range<usize>, ManagedMemoryError> {
        let offset = self.resolve_offset(value.erase())?;
        let size = self
            .objects
            .get(&offset)
            .ok_or(ManagedMemoryError::UnknownReference)?
            .descriptor
            .size();
        Ok(offset..offset + size)
    }

    /// Localizes an image-shared aggregate descriptor once per actor heap.
    pub(super) fn allocate_shared_aggregate<T>(
        &mut self,
        descriptor: &Arc<ManagedTypeDescriptor>,
        references: &[(usize, TvmRef<()>)],
        initialize: impl FnOnce(&mut [u8]) -> Result<(), ManagedMemoryError>,
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        let local = OWNER_DESCRIPTOR_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache
                .aggregate
                .iter()
                .find(|local| local.fingerprint() == descriptor.fingerprint())
                .cloned()
                .unwrap_or_else(|| {
                    let local = Arc::new(descriptor.as_ref().clone());
                    if cache.aggregate.len() < MAX_CACHED_AGGREGATE_DESCRIPTORS {
                        cache.aggregate.push(Arc::clone(&local));
                    }
                    local
                })
        });
        self.allocate_initialized(local, references, initialize)
    }

    /// Reuses immutable sequence layouts by semantic identity and byte size.
    pub(super) fn sequence_descriptor(
        &mut self,
        semantic: SemanticTypeId,
        size: usize,
        allocation_class: AllocationClass,
    ) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
        OWNER_DESCRIPTOR_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(entry) = cache.sequence.iter().find(|entry| {
                entry.semantic == semantic
                    && entry.size == size
                    && entry.allocation_class == allocation_class
            }) {
                return Ok(Arc::clone(&entry.descriptor));
            }
            let descriptor = Arc::new(ManagedTypeDescriptor::new(
                semantic,
                size,
                8,
                Vec::new(),
                allocation_class,
            )?);
            let entry = SequenceDescriptorCacheEntry {
                semantic,
                size,
                allocation_class,
                descriptor: Arc::clone(&descriptor),
            };
            if cache.sequence.len() < MAX_CACHED_SEQUENCE_DESCRIPTORS {
                cache.sequence.push(entry);
            } else {
                let index = cache.next_sequence % MAX_CACHED_SEQUENCE_DESCRIPTORS;
                cache.sequence[index] = entry;
                cache.next_sequence = (index + 1) % MAX_CACHED_SEQUENCE_DESCRIPTORS;
            }
            Ok(descriptor)
        })
    }

    /// Reuses immutable shape-specialized layouts without a global cache.
    pub(super) fn specialized_descriptor(
        &mut self,
        semantic: SemanticTypeId,
        size: usize,
        alignment: usize,
        reference_offsets: &[usize],
        allocation_class: AllocationClass,
        representation: &[u8],
    ) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
        OWNER_DESCRIPTOR_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(entry) = cache.specialized.iter().find(|entry| {
                entry.semantic == semantic
                    && entry.size == size
                    && entry.alignment == alignment
                    && entry.allocation_class == allocation_class
                    && entry.representation.as_ref() == representation
                    && entry.descriptor.reference_offsets() == reference_offsets
            }) {
                return Ok(Arc::clone(&entry.descriptor));
            }
            let descriptor = Arc::new(ManagedTypeDescriptor::new_specialized(
                semantic,
                size,
                alignment,
                reference_offsets.to_vec(),
                allocation_class,
                representation,
            )?);
            let entry = SpecializedDescriptorCacheEntry {
                semantic,
                size,
                alignment,
                allocation_class,
                representation: representation.into(),
                descriptor: Arc::clone(&descriptor),
            };
            if cache.specialized.len() < MAX_CACHED_SPECIALIZED_DESCRIPTORS {
                cache.specialized.push(entry);
            } else {
                let index = cache.next_specialized % MAX_CACHED_SPECIALIZED_DESCRIPTORS;
                cache.specialized[index] = entry;
                cache.next_specialized = (index + 1) % MAX_CACHED_SPECIALIZED_DESCRIPTORS;
            }
            Ok(descriptor)
        })
    }

    /// Reads one immutable object payload after validating owner and generation.
    pub fn read<T>(&self, reference: TvmRef<T>) -> Result<&[u8], ManagedMemoryError> {
        let offset = self.resolve_offset(reference.erase())?;
        let metadata = self
            .objects
            .get(&offset)
            .ok_or(ManagedMemoryError::UnknownReference)?;
        self.space
            .get(offset..offset + metadata.descriptor.size())
            .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)
    }

    /// Returns the canonical descriptor attached to one live object.
    pub fn descriptor<T>(
        &self,
        reference: TvmRef<T>,
    ) -> Result<&ManagedTypeDescriptor, ManagedMemoryError> {
        let offset = self.resolve_offset(reference.erase())?;
        self.objects
            .get(&offset)
            .map(|metadata| metadata.descriptor.as_ref())
            .ok_or(ManagedMemoryError::UnknownReference)
    }

    /// Decodes and validates one actor-local reference received from native code.
    pub(crate) fn validate_abi_reference(
        &self,
        encoded: u64,
        semantic_id: super::SemanticTypeId,
    ) -> Result<TvmRef<()>, ManagedMemoryError> {
        let encoded = usize::try_from(encoded)
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or(ManagedMemoryError::UnknownReference)?;
        let reference = TvmRef::from_encoded(encoded);
        if self.descriptor(reference)?.semantic_id() != semantic_id {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        Ok(reference)
    }

    /// Reads and validates one managed-reference field from a live object.
    pub fn reference_field<T>(
        &self,
        object: TvmRef<T>,
        reference_offset: usize,
    ) -> Result<TvmRef<()>, ManagedMemoryError> {
        let object_offset = self.resolve_offset(object)?;
        let metadata = self
            .objects
            .get(&object_offset)
            .ok_or(ManagedMemoryError::UnknownReference)?;
        if metadata
            .descriptor
            .reference_offsets()
            .binary_search(&reference_offset)
            .is_err()
        {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        let encoded = read_reference(&self.space, object_offset + reference_offset)?;
        self.resolve_encoded(encoded)?;
        NonZeroUsize::new(encoded)
            .map(TvmRef::from_encoded)
            .ok_or(ManagedMemoryError::UnknownReference)
    }

    /// Copies one immutable graph atomically into a receiver-owned mailbox fragment.
    pub(crate) fn copy_message_graph_to(
        &self,
        root: TvmRef<()>,
        expected_type: SemanticTypeId,
        receiver: &mut ActorHeap,
        fragment_id: u32,
        work_budget_bytes: usize,
    ) -> Result<ManagedMailboxFragment, ManagedMemoryError> {
        if self.owner == receiver.owner || fragment_id == 0 || work_budget_bytes == 0 {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        let root_offset = self.resolve_offset(root)?;
        if self
            .objects
            .get(&root_offset)
            .ok_or(ManagedMemoryError::UnknownReference)?
            .descriptor
            .semantic_id()
            != expected_type
        {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        let (copy_order, copied_payload_bytes) =
            self.message_copy_order(root_offset, work_budget_bytes)?;
        let receiver_bytes_before = receiver.space.len();
        let mut staged = ActorHeap {
            owner: receiver.owner,
            token: receiver.token,
            next_reserved_token: receiver.next_reserved_token,
            reserved_tokens_remaining: receiver.reserved_tokens_remaining,
            latest_retired_token: receiver.latest_retired_token,
            retired_tokens: receiver.retired_tokens.clone(),
            limits: receiver.limits,
            space: receiver.space.clone(),
            objects: receiver.objects.clone(),
            external_strings: receiver.external_strings.clone(),
            collections: receiver.collections,
            reuse_underutilized_count: receiver.reuse_underutilized_count,
        };
        let mut relocated = BTreeMap::<usize, TvmRef<()>>::new();
        for source_offset in &copy_order {
            let metadata = self
                .objects
                .get(source_offset)
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            let payload = self
                .space
                .get(*source_offset..*source_offset + metadata.descriptor.size())
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?
                .to_vec();
            let references = metadata
                .descriptor
                .reference_offsets()
                .iter()
                .map(|reference_offset| {
                    let encoded = read_reference(&self.space, source_offset + reference_offset)?;
                    let child_offset = self.resolve_encoded(encoded)?;
                    relocated
                        .get(&child_offset)
                        .copied()
                        .map(|reference| (*reference_offset, reference))
                        .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let copied =
                staged.allocate::<()>(metadata.descriptor.clone(), &payload, &references)?;
            if let Some(external) = self.external_strings.get(source_offset) {
                staged.remember_external_string(copied, external.clone())?;
            }
            relocated.insert(*source_offset, copied);
        }
        let copied_root = relocated
            .get(&root_offset)
            .copied()
            .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
        let receiver_heap_bytes = staged
            .space
            .len()
            .checked_sub(receiver_bytes_before)
            .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
        let root = ManagedRoot::new(
            staged.owner,
            RootLocation::Mailbox {
                fragment: fragment_id,
                slot: 0,
            },
            copied_root,
        );
        let fragment = ManagedMailboxFragment::new(
            self.owner,
            staged.owner,
            fragment_id,
            root,
            copy_order.len(),
            copied_payload_bytes,
            receiver_heap_bytes,
        );
        *receiver = staged;
        Ok(fragment)
    }

    /// Retains one immutable same-owner graph as a precise mailbox root.
    pub(crate) fn retain_message_graph(
        &self,
        root: TvmRef<()>,
        expected_type: SemanticTypeId,
        fragment_id: u32,
    ) -> Result<ManagedMailboxFragment, ManagedMemoryError> {
        if fragment_id == 0 {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        let root_offset = self.resolve_offset(root)?;
        if self
            .objects
            .get(&root_offset)
            .ok_or(ManagedMemoryError::UnknownReference)?
            .descriptor
            .semantic_id()
            != expected_type
        {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        Ok(ManagedMailboxFragment::new(
            self.owner,
            self.owner,
            fragment_id,
            ManagedRoot::new(
                self.owner,
                RootLocation::Mailbox {
                    fragment: fragment_id,
                    slot: 0,
                },
                root,
            ),
            0,
            0,
            0,
        ))
    }

    /// Rolls back the most recently copied cross-owner mailbox graph.
    pub(crate) fn rollback_message_graph(
        &mut self,
        fragment: &ManagedMailboxFragment,
    ) -> Result<(), ManagedMemoryError> {
        if fragment.receiver() != self.owner {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        let copied_bytes = fragment.receiver_heap_bytes();
        if copied_bytes == 0 {
            return Ok(());
        }
        let start = self
            .space
            .len()
            .checked_sub(copied_bytes)
            .ok_or(ManagedMemoryError::InvalidMailboxTransfer)?;
        let root_offset = self.resolve_offset(fragment.root_reference())?;
        if root_offset < start {
            return Err(ManagedMemoryError::InvalidMailboxTransfer);
        }
        self.space.truncate(start);
        self.objects.truncate_from(start);
        self.external_strings.retain(|offset, _| *offset < start);
        Ok(())
    }

    /// Performs one bounded precise copying collection and relocates every root.
    pub fn collect(
        &mut self,
        roots: &mut [ManagedRoot],
        work_budget_bytes: usize,
    ) -> Result<CollectionStats, ManagedMemoryError> {
        let live_offsets = self.trace_live_offsets(roots, work_budget_bytes)?;
        let bytes_before = self.space.len();
        let objects_before = self.objects.len();
        let new_token = next_token();
        let mut new_space = Vec::with_capacity(bytes_before.min(self.limits.hard_bytes));
        let mut new_objects = ObjectTable::default();
        let mut new_external_strings = BTreeMap::new();
        let mut relocation = BTreeMap::new();

        for old_offset in &live_offsets {
            let metadata = self
                .objects
                .get(old_offset)
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            let new_offset = align_up(new_space.len(), metadata.descriptor.alignment())?;
            new_space.resize(new_offset, 0);
            let source = self
                .space
                .get(*old_offset..*old_offset + metadata.descriptor.size())
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            new_space.extend_from_slice(source);
            relocation.insert(*old_offset, new_offset);
            new_objects.insert(new_offset, metadata.clone());
            if let Some(external) = self.external_strings.get(old_offset) {
                new_external_strings.insert(new_offset, external.clone());
            }
        }

        relocate_object_fields(
            &mut new_space,
            &new_objects,
            self.token,
            new_token,
            &relocation,
        )?;
        for root in roots.iter_mut() {
            if root.owner() != self.owner {
                return Err(ManagedMemoryError::CrossActorReference);
            }
            let old_offset = self.resolve_offset(root.reference())?;
            let new_offset = relocation
                .get(&old_offset)
                .copied()
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            root.relocate(reference_with_token(new_token, new_offset)?);
        }

        self.retire_token(self.token);
        self.token = new_token;
        self.space = new_space;
        self.objects = new_objects;
        self.external_strings = new_external_strings;
        self.collections = self.collections.saturating_add(1);
        Ok(CollectionStats {
            bytes_before,
            bytes_after: self.space.len(),
            objects_before,
            objects_after: self.objects.len(),
            work_bytes: live_offsets
                .iter()
                .map(|offset| {
                    self.objects
                        .get(relocation.get(offset).expect("live object was relocated"))
                        .map_or(0, |metadata| {
                            metadata.descriptor.size() + METADATA_WORK_BYTES
                        })
                })
                .sum(),
        })
    }

    /// Reclaims the complete actor heap immediately at actor exit.
    pub fn reclaim_all(&mut self) {
        self.space.clear();
        self.objects.clear();
        self.external_strings.clear();
        self.reuse_underutilized_count = 0;
        self.retire_token(self.token);
        self.token = self.next_reuse_token();
    }

    /// Reclaims a completed actor heap for bounded shard-local reuse.
    pub(crate) fn reclaim_for_reuse(&mut self) {
        if self.space.capacity() > MAX_RETAINED_REUSE_BYTES
            && self.space.len() <= REUSE_UNDERUTILIZED_BYTES
        {
            self.reuse_underutilized_count = self.reuse_underutilized_count.saturating_add(1);
        } else {
            self.reuse_underutilized_count = 0;
        }
        let retired = self.token;
        self.space.clear();
        self.objects.clear();
        self.external_strings.clear();
        if self.reuse_underutilized_count >= REUSE_UNDERUTILIZED_LIMIT {
            self.space
                .shrink_to(INITIAL_HEAP_BYTES.min(self.limits.soft_bytes));
            self.objects.entries.shrink_to(32);
            self.reuse_underutilized_count = 0;
        }
        self.retired_tokens.clear();
        self.latest_retired_token = Some(retired);
        self.token = self.next_reuse_token();
        self.collections = 0;
    }

    /// Retains complete stale-token classification while keeping the newest
    /// generation in a direct slot.
    fn retire_token(&mut self, token: u32) {
        if let Some(previous) = self.latest_retired_token.replace(token) {
            self.retired_tokens.insert(previous);
        }
    }

    /// Assigns an already-reclaimed heap to one new actor owner.
    pub(crate) fn assign_recycled_owner(&mut self, owner: ActorId) {
        debug_assert!(self.space.is_empty());
        debug_assert!(self.objects.is_empty());
        self.owner = owner;
    }

    /// Traces every object reachable from precise roots without mutating the heap.
    fn trace_live_offsets(
        &self,
        roots: &[ManagedRoot],
        work_budget_bytes: usize,
    ) -> Result<BTreeSet<usize>, ManagedMemoryError> {
        let mut queue = VecDeque::new();
        for root in roots {
            if root.owner() != self.owner {
                return Err(ManagedMemoryError::CrossActorReference);
            }
            queue.push_back(self.resolve_offset(root.reference())?);
        }
        let mut live = BTreeSet::new();
        let mut work = 0_usize;
        while let Some(offset) = queue.pop_front() {
            if !live.insert(offset) {
                continue;
            }
            let metadata = self
                .objects
                .get(&offset)
                .ok_or(ManagedMemoryError::UnknownReference)?;
            let external_bytes = self.external_strings.get(&offset).map_or(0, Bytes::len);
            work = work
                .checked_add(metadata.descriptor.size() + external_bytes + METADATA_WORK_BYTES)
                .ok_or(ManagedMemoryError::CollectionBudgetExceeded)?;
            if work > work_budget_bytes {
                return Err(ManagedMemoryError::CollectionBudgetExceeded);
            }
            for reference_offset in metadata.descriptor.reference_offsets() {
                let encoded = read_reference(&self.space, offset + reference_offset)?;
                queue.push_back(
                    self.resolve_encoded(encoded)
                        .map_err(|_| ManagedMemoryError::CorruptedRelocationMetadata)?,
                );
            }
        }
        Ok(live)
    }

    /// Resolves a typed reference into a validated active-space offset.
    fn resolve_offset<T>(&self, reference: TvmRef<T>) -> Result<usize, ManagedMemoryError> {
        self.resolve_encoded(reference.encoded().get())
    }

    /// Resolves an encoded reference while distinguishing foreign and stale tokens.
    fn resolve_encoded(&self, encoded: usize) -> Result<usize, ManagedMemoryError> {
        let token = (encoded >> TOKEN_SHIFT) as u32;
        if token != self.token {
            return if self.latest_retired_token == Some(token)
                || self.retired_tokens.contains(&token)
            {
                Err(ManagedMemoryError::StaleReference)
            } else {
                Err(ManagedMemoryError::CrossActorReference)
            };
        }
        let offset = (encoded & OFFSET_MASK)
            .checked_sub(1)
            .ok_or(ManagedMemoryError::UnknownReference)?;
        if !self.objects.contains_key(&offset) {
            return Err(ManagedMemoryError::UnknownReference);
        }
        Ok(offset)
    }

    /// Produces a child-first distinct-object copy order under a hard work budget.
    fn message_copy_order(
        &self,
        root_offset: usize,
        work_budget_bytes: usize,
    ) -> Result<(Vec<usize>, usize), ManagedMemoryError> {
        let mut stack = vec![(root_offset, false)];
        let mut visiting = BTreeSet::new();
        let mut completed = BTreeSet::new();
        let mut order = Vec::new();
        let mut work_bytes = 0_usize;
        let mut payload_bytes = 0_usize;
        while let Some((offset, expanded)) = stack.pop() {
            if expanded {
                if !visiting.remove(&offset) || !completed.insert(offset) {
                    continue;
                }
                let metadata = self
                    .objects
                    .get(&offset)
                    .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
                let external_bytes = self.external_strings.get(&offset).map_or(0, Bytes::len);
                payload_bytes = payload_bytes
                    .checked_add(metadata.descriptor.size() + external_bytes)
                    .ok_or(ManagedMemoryError::MessageTransferBudgetExceeded)?;
                work_bytes = work_bytes
                    .checked_add(metadata.descriptor.size() + external_bytes + METADATA_WORK_BYTES)
                    .ok_or(ManagedMemoryError::MessageTransferBudgetExceeded)?;
                if work_bytes > work_budget_bytes {
                    return Err(ManagedMemoryError::MessageTransferBudgetExceeded);
                }
                order.push(offset);
                continue;
            }
            if completed.contains(&offset) {
                continue;
            }
            if !visiting.insert(offset) {
                return Err(ManagedMemoryError::InvalidMailboxTransfer);
            }
            let metadata = self
                .objects
                .get(&offset)
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            stack.push((offset, true));
            for reference_offset in metadata.descriptor.reference_offsets().iter().rev() {
                let encoded = read_reference(&self.space, offset + reference_offset)?;
                let child = self.resolve_encoded(encoded)?;
                if visiting.contains(&child) {
                    return Err(ManagedMemoryError::InvalidMailboxTransfer);
                }
                if !completed.contains(&child) {
                    stack.push((child, false));
                }
            }
        }
        Ok((order, payload_bytes))
    }

    /// Creates a typed reference to an existing active-space object.
    fn reference_for_offset<T>(&self, offset: usize) -> Result<TvmRef<T>, ManagedMemoryError> {
        reference_with_token(self.token, offset)
    }
}

#[cfg(test)]
#[path = "heap_support_test.rs"]
mod heap_test_support;
