//! Direct and transitive accounting for actor-owned managed values.

use std::collections::{BTreeSet, VecDeque};

use bytes::Bytes;

use super::super::{ActorHeap, ManagedMemoryError, TvmRef};
use super::read_reference;

impl ActorHeap {
    /// Returns bytes owned directly by one live managed object.
    ///
    /// The result includes the object's fixed payload and any immutable
    /// out-of-line bytes owned by that object. Referenced child objects are not
    /// included.
    pub fn shallow_size<T>(&self, reference: TvmRef<T>) -> Result<usize, ManagedMemoryError> {
        let offset = self.resolve_offset(reference)?;
        let metadata = self
            .objects
            .get(&offset)
            .ok_or(ManagedMemoryError::UnknownReference)?;
        metadata
            .descriptor
            .size()
            .checked_add(self.external_strings.get(&offset).map_or(0, Bytes::len))
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)
    }

    /// Returns the distinct transitive bytes retained by one live managed object.
    ///
    /// Shared descendants are counted once, and traversal terminates for cyclic
    /// object graphs. Runtime object-table bookkeeping and spare heap capacity
    /// are deliberately excluded.
    pub fn retained_size<T>(&self, reference: TvmRef<T>) -> Result<usize, ManagedMemoryError> {
        let mut pending = VecDeque::from([self.resolve_offset(reference)?]);
        let mut visited = BTreeSet::new();
        let mut retained = 0_usize;
        while let Some(offset) = pending.pop_front() {
            if !visited.insert(offset) {
                continue;
            }
            let metadata = self
                .objects
                .get(&offset)
                .ok_or(ManagedMemoryError::UnknownReference)?;
            retained = retained
                .checked_add(metadata.descriptor.size())
                .and_then(|bytes| {
                    bytes.checked_add(self.external_strings.get(&offset).map_or(0, Bytes::len))
                })
                .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
            for reference_offset in metadata.descriptor.reference_offsets() {
                let encoded = read_reference(&self.space, offset + reference_offset)?;
                pending.push_back(
                    self.resolve_encoded(encoded)
                        .map_err(|_| ManagedMemoryError::CorruptedRelocationMetadata)?,
                );
            }
        }
        Ok(retained)
    }
}
