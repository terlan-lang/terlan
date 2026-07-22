//! Actor-local bump allocation and precise semispace collection.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::{
    ActorId, ManagedMailboxFragment, ManagedMemoryError, ManagedRoot, ManagedTypeDescriptor,
    RootLocation, SemanticTypeId, TvmRef,
};

const TOKEN_SHIFT: usize = 32;
const OFFSET_MASK: usize = u32::MAX as usize;
const METADATA_WORK_BYTES: usize = 32;
static NEXT_HEAP_TOKEN: AtomicU32 = AtomicU32::new(1);

/// Soft and hard actor-local managed-heap limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapLimits {
    pub soft_bytes: usize,
    pub hard_bytes: usize,
}

impl HeapLimits {
    /// Validates nonzero ordered actor-local heap limits.
    pub fn new(soft_bytes: usize, hard_bytes: usize) -> Result<Self, ManagedMemoryError> {
        if soft_bytes == 0 || soft_bytes > hard_bytes || hard_bytes > OFFSET_MASK {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        Ok(Self {
            soft_bytes,
            hard_bytes,
        })
    }
}

/// Observable result of one completed actor-local collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollectionStats {
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub objects_before: usize,
    pub objects_after: usize,
    pub work_bytes: usize,
}

#[derive(Clone, Debug)]
struct ObjectMetadata {
    descriptor: Arc<ManagedTypeDescriptor>,
}

/// Independently collectible managed heap owned by exactly one actor.
#[derive(Debug)]
pub struct ActorHeap {
    owner: ActorId,
    token: u32,
    retired_tokens: BTreeSet<u32>,
    limits: HeapLimits,
    space: Vec<u8>,
    objects: BTreeMap<usize, ObjectMetadata>,
    collections: u64,
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
            retired_tokens: BTreeSet::new(),
            limits,
            space: Vec::with_capacity(limits.soft_bytes.min(64 * 1024)),
            objects: BTreeMap::new(),
            collections: 0,
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
                self.objects.split_off(&space_len);
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
        let supplied_offsets = references
            .iter()
            .map(|(offset, _)| *offset)
            .collect::<Vec<_>>();
        if supplied_offsets != descriptor.reference_offsets() {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        for (_, reference) in references {
            self.resolve_offset(*reference)?;
        }
        let offset = align_up(self.space.len(), descriptor.alignment())?;
        let end = offset
            .checked_add(payload.len())
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        if end > self.limits.hard_bytes || end > OFFSET_MASK {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        self.space.resize(offset, 0);
        self.space.extend_from_slice(payload);
        for (reference_offset, reference) in references {
            write_reference(
                &mut self.space,
                offset + reference_offset,
                reference.encoded().get(),
            )?;
        }
        self.objects.insert(offset, ObjectMetadata { descriptor });
        self.reference_for_offset(offset)
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
            retired_tokens: receiver.retired_tokens.clone(),
            limits: receiver.limits,
            space: receiver.space.clone(),
            objects: receiver.objects.clone(),
            collections: receiver.collections,
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
        self.objects.split_off(&start);
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
        let mut new_objects = BTreeMap::new();
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

        self.retired_tokens.insert(self.token);
        self.token = new_token;
        self.space = new_space;
        self.objects = new_objects;
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
        self.retired_tokens.insert(self.token);
        self.token = next_token();
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
            work = work
                .checked_add(metadata.descriptor.size() + METADATA_WORK_BYTES)
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
            return if self.retired_tokens.contains(&token) {
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
                payload_bytes = payload_bytes
                    .checked_add(metadata.descriptor.size())
                    .ok_or(ManagedMemoryError::MessageTransferBudgetExceeded)?;
                work_bytes = work_bytes
                    .checked_add(metadata.descriptor.size() + METADATA_WORK_BYTES)
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

/// Returns a fresh nonzero heap-generation token.
fn next_token() -> u32 {
    loop {
        let token = NEXT_HEAP_TOKEN.fetch_add(1, Ordering::Relaxed);
        if token != 0 {
            return token;
        }
    }
}

/// Aligns one semispace cursor without overflow.
fn align_up(value: usize, alignment: usize) -> Result<usize, ManagedMemoryError> {
    value
        .checked_add(alignment - 1)
        .map(|aligned| aligned & !(alignment - 1))
        .ok_or(ManagedMemoryError::AllocationLimitExceeded)
}

/// Encodes a heap-generation token and semispace offset into one pointer-width reference.
fn reference_with_token<T>(token: u32, offset: usize) -> Result<TvmRef<T>, ManagedMemoryError> {
    let low = offset
        .checked_add(1)
        .filter(|value| *value <= OFFSET_MASK)
        .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
    let encoded = ((token as usize) << TOKEN_SHIFT) | low;
    NonZeroUsize::new(encoded)
        .map(TvmRef::from_encoded)
        .ok_or(ManagedMemoryError::UnknownReference)
}

/// Reads one pointer-width reference field from an object payload.
fn read_reference(space: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    let bytes: [u8; std::mem::size_of::<usize>()] = space
        .get(offset..offset + std::mem::size_of::<usize>())
        .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedRelocationMetadata)?;
    Ok(usize::from_le_bytes(bytes))
}

/// Writes one pointer-width reference field into an object payload.
fn write_reference(
    space: &mut [u8],
    offset: usize,
    encoded: usize,
) -> Result<(), ManagedMemoryError> {
    let destination = space
        .get_mut(offset..offset + std::mem::size_of::<usize>())
        .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
    destination.copy_from_slice(&encoded.to_le_bytes());
    Ok(())
}

/// Rewrites every copied managed field using the completed relocation table.
fn relocate_object_fields(
    space: &mut [u8],
    objects: &BTreeMap<usize, ObjectMetadata>,
    old_token: u32,
    new_token: u32,
    relocation: &BTreeMap<usize, usize>,
) -> Result<(), ManagedMemoryError> {
    for (object_offset, metadata) in objects {
        for reference_offset in metadata.descriptor.reference_offsets() {
            let encoded = read_reference(space, object_offset + reference_offset)?;
            if (encoded >> TOKEN_SHIFT) as u32 != old_token {
                return Err(ManagedMemoryError::CorruptedRelocationMetadata);
            }
            let old_offset = (encoded & OFFSET_MASK)
                .checked_sub(1)
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            let new_offset = relocation
                .get(&old_offset)
                .copied()
                .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?;
            let relocated = reference_with_token::<()>(new_token, new_offset)?;
            write_reference(
                space,
                object_offset + reference_offset,
                relocated.encoded().get(),
            )?;
        }
    }
    Ok(())
}
