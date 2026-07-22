//! Immutable aggregate layouts admitted with one native executable image.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::runtime::native_image::{TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor};

use super::{
    decode_aggregate_layout, decode_collection_layout, ActorHeap, AtomIndex, AtomTable,
    ManagedAggregateDescriptor, ManagedCollectionDescriptor, ManagedMemoryError, SemanticTypeId,
    TvmRef,
};

/// Canonical fixed layouts grouped by their shared semantic type identity.
#[derive(Debug, Default)]
pub(crate) struct ManagedLayoutRegistry {
    /// Ordered active layouts for every aggregate semantic identity.
    layouts: BTreeMap<SemanticTypeId, Box<[Arc<ManagedAggregateDescriptor>]>>,
    /// Unique collection schema for every admitted collection semantic identity.
    collections: BTreeMap<SemanticTypeId, Arc<ManagedCollectionDescriptor>>,
    /// Finite image-local atom identities shared by every actor heap.
    atoms: AtomTable,
}

impl ManagedLayoutRegistry {
    /// Decodes the already admitted image table into immutable runtime descriptors.
    pub(crate) fn from_image(
        layouts: &[TvmManagedLayoutDescriptor],
        collections: &[TvmManagedCollectionDescriptor],
        atoms: &[String],
    ) -> Result<Self, String> {
        let mut grouped = BTreeMap::<SemanticTypeId, Vec<Arc<ManagedAggregateDescriptor>>>::new();
        for layout in layouts {
            let semantic = SemanticTypeId::from_bytes(layout.semantic_id);
            let descriptor = Arc::new(
                decode_aggregate_layout(&layout.encoded_layout)
                    .map_err(|error| format!("error[managed_layout_registry.decode]: {error}"))?,
            );
            if descriptor.managed().semantic_id() != semantic {
                return Err(
                    "error[managed_layout_registry.identity]: aggregate layout identity mismatch"
                        .to_string(),
                );
            }
            grouped.entry(semantic).or_default().push(descriptor);
        }
        let mut decoded_collections = BTreeMap::new();
        for collection in collections {
            let semantic = SemanticTypeId::from_bytes(collection.semantic_id);
            let descriptor = Arc::new(
                decode_collection_layout(&collection.encoded_layout).map_err(|error| {
                    format!("error[managed_layout_registry.collection_decode]: {error}")
                })?,
            );
            if descriptor.semantic_id() != semantic {
                return Err(
                    "error[managed_layout_registry.collection_identity]: collection schema identity mismatch"
                        .to_string(),
                );
            }
            if decoded_collections.insert(semantic, descriptor).is_some() {
                return Err(
                    "error[managed_layout_registry.collection_duplicate]: duplicate collection schema"
                        .to_string(),
                );
            }
        }
        Ok(Self {
            layouts: grouped
                .into_iter()
                .map(|(semantic, descriptors)| (semantic, descriptors.into_boxed_slice()))
                .collect(),
            collections: decoded_collections,
            atoms: AtomTable::new(atoms.iter().cloned())
                .map_err(|error| format!("error[managed_layout_registry.atoms]: {error}"))?,
        })
    }

    /// Resolves public atom text into its compact image-local index.
    pub(crate) fn atom_index(&self, identity: &str) -> Result<AtomIndex, ManagedMemoryError> {
        self.atoms.index(identity)
    }

    /// Resolves one compact image-local atom index into public atom text.
    pub(crate) fn atom_identity(&self, index: AtomIndex) -> Result<&str, ManagedMemoryError> {
        self.atoms.identity(index)
    }

    /// Returns one admitted collection schema by exact semantic identity.
    pub(crate) fn collection(
        &self,
        semantic: SemanticTypeId,
    ) -> Option<&Arc<ManagedCollectionDescriptor>> {
        self.collections.get(&semantic)
    }

    /// Returns every admitted active layout for one semantic type.
    pub(crate) fn layouts(&self, semantic: SemanticTypeId) -> &[Arc<ManagedAggregateDescriptor>] {
        self.layouts
            .get(&semantic)
            .map(Box::as_ref)
            .unwrap_or_default()
    }

    /// Resolves the exact active layout attached to one live aggregate object.
    pub(crate) fn layout_for_reference(
        &self,
        heap: &ActorHeap,
        semantic: SemanticTypeId,
        reference: TvmRef<()>,
    ) -> Result<Arc<ManagedAggregateDescriptor>, String> {
        let actual = heap
            .descriptor(reference)
            .map_err(|error| format!("error[managed_layout_registry.reference]: {error}"))?;
        if actual.semantic_id() != semantic {
            return Err(
                "error[managed_layout_registry.reference]: managed semantic identity mismatch"
                    .to_string(),
            );
        }
        self.layouts(semantic)
            .iter()
            .find(|layout| layout.managed().fingerprint() == actual.fingerprint())
            .cloned()
            .ok_or_else(|| {
                "error[managed_layout_registry.layout]: live aggregate has no admitted layout"
                    .to_string()
            })
    }
}
