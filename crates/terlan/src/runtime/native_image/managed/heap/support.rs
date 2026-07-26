//! Pointer encoding and relocation helpers for actor-local heaps.

use super::{
    ActorHeap, ManagedMemoryError, ManagedTypeDescriptor, ObjectMetadata, ObjectTable, TvmRef,
    INITIAL_HEAP_BYTES, NEXT_HEAP_TOKEN, OFFSET_MASK, TOKEN_RESERVATION_SIZE, TOKEN_SHIFT,
};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use bytes::Bytes;

/// Soft and hard actor-local managed-heap limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapLimits {
    pub soft_bytes: usize,
    pub hard_bytes: usize,
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

impl ActorHeap {
    /// Allocates one String ABI placeholder backed by immutable external bytes.
    pub(crate) fn allocate_external_string_storage<T>(
        &mut self,
        descriptor: Arc<ManagedTypeDescriptor>,
        length: &[u8],
        value: Bytes,
    ) -> Result<TvmRef<T>, ManagedMemoryError> {
        let retained =
            self.external_strings
                .values()
                .try_fold(self.space.len(), |bytes, value| {
                    bytes
                        .checked_add(value.len())
                        .ok_or(ManagedMemoryError::AllocationLimitExceeded)
                })?;
        if retained
            .checked_add(value.len())
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?
            > self.limits.hard_bytes
        {
            return Err(ManagedMemoryError::AllocationLimitExceeded);
        }
        let reference = self.allocate_reference_free_parts(descriptor, &[length])?;
        self.remember_external_string(reference.erase(), value)?;
        Ok(reference)
    }

    /// Returns immutable out-of-line String bytes for one exact live reference.
    pub(crate) fn external_string_bytes<T>(
        &self,
        reference: TvmRef<T>,
    ) -> Result<Option<&Bytes>, ManagedMemoryError> {
        let offset = self.resolve_offset(reference)?;
        Ok(self.external_strings.get(&offset))
    }

    /// Associates cloned immutable storage with an already allocated placeholder.
    pub(crate) fn remember_external_string(
        &mut self,
        reference: TvmRef<()>,
        value: Bytes,
    ) -> Result<(), ManagedMemoryError> {
        let offset = self.resolve_offset(reference)?;
        self.external_strings.insert(offset, value);
        Ok(())
    }

    /// Draws a fresh generation from an owner-local reservation.
    ///
    /// The global allocator is touched only once per block, avoiding cache-line
    /// contention when independent shards reset request heaps concurrently.
    pub(super) fn next_reuse_token(&mut self) -> u32 {
        loop {
            if self.reserved_tokens_remaining == 0 {
                self.next_reserved_token =
                    NEXT_HEAP_TOKEN.fetch_add(TOKEN_RESERVATION_SIZE, Ordering::Relaxed);
                self.reserved_tokens_remaining = TOKEN_RESERVATION_SIZE;
            }
            let token = self.next_reserved_token;
            self.next_reserved_token = self.next_reserved_token.wrapping_add(1);
            self.reserved_tokens_remaining -= 1;
            if token != 0 {
                return token;
            }
        }
    }

    /// Releases actor-specific cache high-water storage before cross-actor pooling.
    pub(crate) fn reclaim_for_pool(&mut self) {
        self.reclaim_for_reuse();
        self.space
            .shrink_to(INITIAL_HEAP_BYTES.min(self.limits.soft_bytes));
        self.objects.entries.shrink_to(32);
        self.reuse_underutilized_count = 0;
    }

    /// Returns host allocation capacity retained by this reusable heap.
    pub(crate) fn retained_capacity_bytes(&self) -> usize {
        self.space.capacity().saturating_add(
            self.objects
                .entries
                .capacity()
                .saturating_mul(std::mem::size_of::<(usize, ObjectMetadata)>()),
        )
    }
}

/// Returns a fresh nonzero heap-generation token.
pub(super) fn next_token() -> u32 {
    loop {
        let token = NEXT_HEAP_TOKEN.fetch_add(1, Ordering::Relaxed);
        if token != 0 {
            return token;
        }
    }
}

/// Aligns one semispace cursor without overflow.
pub(super) fn align_up(value: usize, alignment: usize) -> Result<usize, ManagedMemoryError> {
    value
        .checked_add(alignment - 1)
        .map(|aligned| aligned & !(alignment - 1))
        .ok_or(ManagedMemoryError::AllocationLimitExceeded)
}

/// Encodes a heap-generation token and semispace offset into one reference.
pub(super) fn reference_with_token<T>(
    token: u32,
    offset: usize,
) -> Result<TvmRef<T>, ManagedMemoryError> {
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
pub(super) fn read_reference(space: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    let bytes: [u8; std::mem::size_of::<usize>()] = space
        .get(offset..offset + std::mem::size_of::<usize>())
        .ok_or(ManagedMemoryError::CorruptedRelocationMetadata)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedRelocationMetadata)?;
    Ok(usize::from_le_bytes(bytes))
}

/// Writes one pointer-width reference field into an object payload.
pub(super) fn write_reference(
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
pub(super) fn relocate_object_fields(
    space: &mut [u8],
    objects: &ObjectTable,
    old_token: u32,
    new_token: u32,
    relocation: &BTreeMap<usize, usize>,
) -> Result<(), ManagedMemoryError> {
    for (object_offset, metadata) in &objects.entries {
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
