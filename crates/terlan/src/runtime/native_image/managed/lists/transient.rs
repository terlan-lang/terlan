//! Exclusive actor-local transient construction for persistent RRB lists.

use super::super::aggregates::validate_typed_value;
use super::*;

/// Bounded transient buffer that publishes one canonical persistent list.
///
/// The builder exclusively borrows its actor heap, preventing collection from
/// relocating managed references retained by the construction buffer. Dropping
/// it before `finish` publishes no managed list object.
#[derive(Debug)]
pub struct ManagedListBuilder<'heap> {
    heap: &'heap mut ActorHeap,
    descriptor: ManagedListDescriptor,
    elements: Vec<ManagedFieldValue>,
}

impl ActorHeap {
    /// Starts a bounded list construction transaction for this actor heap.
    pub fn list_builder(
        &mut self,
        descriptor: &ManagedListDescriptor,
        expected_length: usize,
    ) -> Result<ManagedListBuilder<'_>, ManagedMemoryError> {
        validate_element_count(expected_length)?;
        Ok(ManagedListBuilder {
            heap: self,
            descriptor: descriptor.clone(),
            elements: Vec::with_capacity(expected_length),
        })
    }
}

impl ManagedListBuilder<'_> {
    /// Returns the number of values accepted by this builder.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Reports whether this builder currently contains no values.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Appends one validated value without allocating a managed list node.
    pub fn push(&mut self, value: ManagedFieldValue) -> Result<(), ManagedMemoryError> {
        let next_length = self
            .elements
            .len()
            .checked_add(1)
            .ok_or(ManagedMemoryError::CollectionTooLarge)?;
        validate_element_count(next_length)?;
        validate_typed_value(self.heap, self.descriptor.element_type, value)?;
        self.elements.push(value);
        Ok(())
    }

    /// Atomically appends a validated value slice to the construction buffer.
    pub fn extend_from_slice(
        &mut self,
        values: &[ManagedFieldValue],
    ) -> Result<(), ManagedMemoryError> {
        let next_length = self
            .elements
            .len()
            .checked_add(values.len())
            .ok_or(ManagedMemoryError::CollectionTooLarge)?;
        validate_element_count(next_length)?;
        for value in values {
            validate_typed_value(self.heap, self.descriptor.element_type, *value)?;
        }
        self.elements.extend_from_slice(values);
        Ok(())
    }

    /// Publishes the buffered values as one canonical immutable RRB list.
    pub fn finish(self) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        self.heap
            .list_from_elements(&self.descriptor, &self.elements)
    }
}
