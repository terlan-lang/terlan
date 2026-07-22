//! Actor-local immutable sets backed by the canonical managed map representation.

use super::{
    ActorHeap, ManagedFieldType, ManagedFieldValue, ManagedKeySemantics, ManagedMapDescriptor,
    ManagedMapProfile, ManagedMemoryError, SemanticTypeId, TvmRef,
};

/// Compile-time marker for one actor-local immutable set root.
#[derive(Debug)]
pub struct ManagedSet;

/// Canonical typed descriptor for `Set[T]` using unit-valued map entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedSetDescriptor {
    map: ManagedMapDescriptor,
}

impl ManagedSetDescriptor {
    /// Creates a set descriptor from its canonical checked element type.
    pub fn new(
        canonical_type: &str,
        element_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        Ok(Self {
            map: ManagedMapDescriptor::new(canonical_type, element_type, ManagedFieldType::Unit)?,
        })
    }

    /// Returns the canonical set semantic identity.
    pub fn semantic_id(&self) -> SemanticTypeId {
        self.map.semantic_id()
    }

    /// Returns the statically selected element slot category.
    pub fn element_type(&self) -> ManagedFieldType {
        self.map.key_type()
    }
}

impl ActorHeap {
    /// Materializes an insertion-ordered set and removes structural duplicates.
    pub fn set_from_elements<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedSetDescriptor,
        elements: &[ManagedFieldValue],
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
        let entries = elements
            .iter()
            .copied()
            .map(|element| (element, ManagedFieldValue::Unit))
            .collect::<Vec<_>>();
        self.map_from_entries(&descriptor.map, &entries, semantics)
            .map(TvmRef::cast)
    }

    /// Allocates the canonical empty representation for one set type.
    pub fn set_empty(
        &mut self,
        descriptor: &ManagedSetDescriptor,
    ) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
        self.map_empty(&descriptor.map).map(TvmRef::cast)
    }

    /// Returns the number of unique elements in a set.
    pub fn set_length(
        &self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
    ) -> Result<usize, ManagedMemoryError> {
        self.map_length(&descriptor.map, set.cast())
    }

    /// Reports whether a set contains no elements.
    pub fn set_is_empty(
        &self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
    ) -> Result<bool, ManagedMemoryError> {
        self.map_is_empty(&descriptor.map, set.cast())
    }

    /// Returns the physical profile shared with managed maps.
    pub fn set_profile(
        &self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
    ) -> Result<ManagedMapProfile, ManagedMemoryError> {
        self.map_profile(&descriptor.map, set.cast())
    }

    /// Decodes set elements in stable insertion order.
    pub fn set_elements(
        &self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
    ) -> Result<Vec<ManagedFieldValue>, ManagedMemoryError> {
        self.map_entries(&descriptor.map, set.cast())
            .map(|entries| entries.into_iter().map(|(element, _)| element).collect())
    }

    /// Reports whether a structurally equal element is present.
    pub fn set_contains<S: ManagedKeySemantics>(
        &self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
        element: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<bool, ManagedMemoryError> {
        self.map_contains_key(&descriptor.map, set.cast(), element, semantics)
    }

    /// Returns a persistent set containing one element.
    pub fn set_add<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
        element: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
        self.map_put(
            &descriptor.map,
            set.cast(),
            element,
            ManagedFieldValue::Unit,
            semantics,
        )
        .map(TvmRef::cast)
    }

    /// Returns a persistent set without one element.
    pub fn set_remove<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
        element: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
        self.map_remove(&descriptor.map, set.cast(), element, semantics)
            .map(TvmRef::cast)
    }

    /// Returns an empty set while preserving an already empty root.
    pub fn set_clear(
        &mut self,
        descriptor: &ManagedSetDescriptor,
        set: TvmRef<ManagedSet>,
    ) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
        self.map_clear(&descriptor.map, set.cast())
            .map(TvmRef::cast)
    }
}

#[cfg(test)]
#[path = "managed_set_test.rs"]
mod managed_set_test;
