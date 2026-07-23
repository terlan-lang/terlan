//! Pointer encoding and relocation helpers for actor-local heaps.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::atomic::Ordering;

use super::{
    ActorHeap, ManagedMemoryError, ObjectTable, TvmRef, NEXT_HEAP_TOKEN, OFFSET_MASK,
    TOKEN_RESERVATION_SIZE, TOKEN_SHIFT,
};

impl ActorHeap {
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
