//! Shared checked physical-layout operations for managed collection slots.

use super::{ManagedFieldType, ManagedMemoryError};

/// Aligns one managed collection layout cursor.
pub(super) fn align_up(value: usize, alignment: usize) -> Result<usize, ManagedMemoryError> {
    value
        .checked_add(alignment - 1)
        .map(|aligned| aligned & !(alignment - 1))
        .ok_or(ManagedMemoryError::CollectionTooLarge)
}

/// Computes the checked storage size and stride for homogeneous packed slots.
pub(super) fn packed_slot_layout(
    field_type: ManagedFieldType,
    base: usize,
    count: usize,
) -> Result<(usize, usize), ManagedMemoryError> {
    let (element_size, alignment) = field_type.layout();
    let start = align_up(base, alignment)?;
    let stride = align_up(element_size, alignment)?;
    let size = start
        .checked_add(
            stride
                .checked_mul(count)
                .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        )
        .ok_or(ManagedMemoryError::CollectionTooLarge)?
        .max(base);
    Ok((size, stride))
}

/// Computes one checked homogeneous packed-slot byte offset.
pub(super) fn packed_slot_offset(
    field_type: ManagedFieldType,
    base: usize,
    index: usize,
) -> Result<usize, ManagedMemoryError> {
    let (_, alignment) = field_type.layout();
    let (_, stride) = packed_slot_layout(field_type, base, 0)?;
    align_up(base, alignment)?
        .checked_add(
            stride
                .checked_mul(index)
                .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        )
        .ok_or(ManagedMemoryError::CollectionTooLarge)
}
