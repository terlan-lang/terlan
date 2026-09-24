//! Shared owner-checked handle storage for adapters and in-process VM services.

use std::collections::BTreeMap;

use super::{owner_error, stale_error, NativeBoundaryHandle, ResourceError, SYSTEM_RESOURCE_OWNER};

#[cfg(test)]
#[path = "registry_test.rs"]
mod tests;

/// Typed resource registry owned by one VM service or native adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceRegistry<T> {
    pub(super) next_id: u64,
    pub(super) resources: BTreeMap<u64, ResourceSlot<T>>,
}

/// Live resource entry stored behind a generation-tagged handle.
///
/// Inputs:
/// - `generation`: handle generation that must match before a resource can be
///   borrowed or removed.
/// - `value`: service-owned resource payload.
///
/// Output:
/// - Internal registry slot consumed only by `ResourceRegistry`.
///
/// Transformation:
/// - Keeps liveness metadata beside the owned resource value so stale handles
///   cannot access a removed or replaced resource.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ResourceSlot<T> {
    generation: u64,
    owner_process_id: u64,
    pub(super) value: T,
}

impl<T> ResourceRegistry<T> {
    /// Builds an empty resource store.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Empty store whose first handle id is `1`.
    ///
    /// Transformation:
    /// - Initializes deterministic resource-id allocation state.
    pub fn new() -> Self {
        Self {
            next_id: 1,
            resources: BTreeMap::new(),
        }
    }

    /// Inserts an owned resource and returns its opaque handle.
    ///
    /// Inputs:
    /// - `value`: adapter-owned resource value to store.
    ///
    /// Output:
    /// - `Ok(handle)` for a live registry entry.
    /// - `Err(ResourceError)` when handle id allocation would overflow.
    ///
    /// Transformation:
    /// - Moves the value into the store, assigns generation `1`, and advances
    ///   the next id with checked arithmetic.
    pub fn insert(&mut self, value: T) -> Result<NativeBoundaryHandle, ResourceError> {
        self.insert_for_owner(SYSTEM_RESOURCE_OWNER, value)
    }

    /// Inserts a resource owned by one VM process.
    pub fn insert_for_owner(
        &mut self,
        owner_process_id: u64,
        value: T,
    ) -> Result<NativeBoundaryHandle, ResourceError> {
        let id = self.next_id;
        let Some(next_id) = id.checked_add(1) else {
            return Err(ResourceError::new(
                "resource.id_overflow",
                "NativeBoundary resource id allocation overflowed.",
            ));
        };
        self.next_id = next_id;
        let handle = NativeBoundaryHandle { id, generation: 1 };
        self.resources.insert(
            id,
            ResourceSlot {
                generation: handle.generation,
                owner_process_id,
                value,
            },
        );
        Ok(handle)
    }

    /// Verifies that a live resource belongs to the calling VM process.
    pub fn validate_owner(
        &self,
        handle: NativeBoundaryHandle,
        caller_process_id: u64,
    ) -> Result<(), ResourceError> {
        let slot = self.slot(handle)?;
        if slot.owner_process_id == caller_process_id {
            return Ok(());
        }
        Err(owner_error(
            handle,
            slot.owner_process_id,
            caller_process_id,
        ))
    }

    /// Mutably borrows a value after checking the caller and live generation.
    pub fn get_mut_for_owner(
        &mut self,
        handle: NativeBoundaryHandle,
        owner_process_id: u64,
    ) -> Result<&mut T, ResourceError> {
        self.validate_owner(handle, owner_process_id)?;
        self.resources
            .get_mut(&handle.id)
            .map(|slot| &mut slot.value)
            .ok_or_else(|| stale_error(handle))
    }

    /// Borrows a value only after validating both its owner and live generation.
    pub fn get_for_owner(
        &self,
        handle: NativeBoundaryHandle,
        owner_process_id: u64,
    ) -> Result<&T, ResourceError> {
        self.validate_owner(handle, owner_process_id)?;
        Ok(&self.slot(handle)?.value)
    }

    /// Disposes a live resource handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle to dispose.
    ///
    /// Output:
    /// - `Ok(())` when a live resource was removed.
    /// - `Err(ResourceError)` when the handle is stale or missing.
    ///
    /// Transformation:
    /// - Validates generation before removing the resource from the store.
    pub fn dispose(&mut self, handle: NativeBoundaryHandle) -> Result<(), ResourceError> {
        self.dispose_for_owner(handle, SYSTEM_RESOURCE_OWNER)
    }

    /// Disposes a resource only when it belongs to the calling VM process.
    pub fn dispose_for_owner(
        &mut self,
        handle: NativeBoundaryHandle,
        caller_process_id: u64,
    ) -> Result<(), ResourceError> {
        self.validate_owner(handle, caller_process_id)?;
        self.resources.remove(&handle.id);
        Ok(())
    }

    /// Disposes every resource owned by one completed VM process.
    pub fn dispose_owner(&mut self, owner_process_id: u64) -> usize {
        let before = self.resources.len();
        self.resources
            .retain(|_, slot| slot.owner_process_id != owner_process_id);
        before - self.resources.len()
    }

    /// Returns a live resource slot.
    ///
    /// Inputs:
    /// - `handle`: opaque handle supplied by bridge-side code.
    ///
    /// Output:
    /// - `Ok(&ResourceSlot)` when id and generation match.
    /// - `Err(ResourceError)` when the handle is stale or missing.
    ///
    /// Transformation:
    /// - Applies the same stale-handle rule as the proof-track handle module.
    pub(super) fn slot(
        &self,
        handle: NativeBoundaryHandle,
    ) -> Result<&ResourceSlot<T>, ResourceError> {
        match self.resources.get(&handle.id) {
            Some(slot) if slot.generation == handle.generation => Ok(slot),
            _ => Err(stale_error(handle)),
        }
    }

    /// Returns a mutable live resource slot.
    ///
    /// Inputs:
    /// - `handle`: opaque handle supplied by bridge-side code.
    ///
    /// Output:
    /// - `Ok(&mut ResourceSlot)` when id and generation match.
    /// - `Err(ResourceError)` when the handle is stale or missing.
    ///
    /// Transformation:
    /// - Applies the same stale-handle rule as immutable lookup before
    ///   exposing mutable adapter state.
    pub(super) fn slot_mut(
        &mut self,
        handle: NativeBoundaryHandle,
    ) -> Result<&mut ResourceSlot<T>, ResourceError> {
        match self.resources.get_mut(&handle.id) {
            Some(slot) if slot.generation == handle.generation => Ok(slot),
            _ => Err(stale_error(handle)),
        }
    }
}

impl<T> Default for ResourceRegistry<T> {
    /// Builds the default resource store.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Empty `ResourceStore`.
    ///
    /// Transformation:
    /// - Delegates to `ResourceStore::new`.
    fn default() -> Self {
        Self::new()
    }
}
