//! Compatibility view over the shared registry while legacy adapters migrate.

use super::{json, NativeBoundaryHandle, ResourceError, ResourceStore, ResourceValue};
use terlan_runtime_abi::NativeResourceStore;

impl NativeResourceStore<json::Json> for ResourceStore {
    fn borrow(
        &self,
        owner: u64,
        handle: NativeBoundaryHandle,
    ) -> Result<&json::Json, ResourceError> {
        self.validate_owner(handle, owner)?;
        self.json(handle)
    }

    fn borrow_mut(
        &mut self,
        owner: u64,
        handle: NativeBoundaryHandle,
    ) -> Result<&mut json::Json, ResourceError> {
        self.validate_owner(handle, owner)?;
        self.json_mut(handle)
    }

    fn insert(
        &mut self,
        owner: u64,
        value: json::Json,
    ) -> Result<NativeBoundaryHandle, ResourceError> {
        self.insert_for_owner(owner, ResourceValue::Json(value))
    }
}
