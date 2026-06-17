//! Opaque resource registry for SafeNative adapter-owned values.
//!
//! Terlan/BEAM terms must not carry Rust adapter values directly. This module
//! owns those values behind generation-tagged handles so the runtime bridge can
//! pass only stable opaque identifiers across process or language boundaries.

use std::collections::BTreeMap;

use crate::handle::SafeNativeHandle;
use crate::{json, path, uri};

/// Resource kind stored in the SafeNative registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    /// `std.data.Json.Json`.
    Json,
    /// `std.io.Path.Path`.
    Path,
    /// `std.net.Uri.Uri`.
    Uri,
}

/// Adapter-owned opaque resource value.
#[derive(Clone, Debug, PartialEq)]
pub enum ResourceValue {
    /// JSON resource owned by the Rust JSON adapter.
    Json(json::Json),
    /// Path resource owned by the Rust path adapter.
    Path(path::Path),
    /// URI resource owned by the Rust URI adapter.
    Uri(uri::Uri),
}

impl ResourceValue {
    /// Returns the resource kind.
    ///
    /// Inputs:
    /// - `self`: adapter-owned resource value.
    ///
    /// Output:
    /// - Closed resource kind used for type checks.
    ///
    /// Transformation:
    /// - Observes the enum variant without cloning or mutating the value.
    pub fn kind(&self) -> ResourceKind {
        match self {
            Self::Json(_) => ResourceKind::Json,
            Self::Path(_) => ResourceKind::Path,
            Self::Uri(_) => ResourceKind::Uri,
        }
    }
}

/// Stable resource-registry error returned by handle operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceError {
    code: &'static str,
    message: String,
}

impl ResourceError {
    /// Builds a resource-registry error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    ///
    /// Output:
    /// - A `ResourceError` suitable for native bridge diagnostics.
    ///
    /// Transformation:
    /// - Stores stable error fields without exposing backend resource details.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: resource error.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: resource error.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// SafeNative resource registry owned by one native worker.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceStore {
    next_id: u64,
    resources: BTreeMap<u64, ResourceSlot>,
}

#[derive(Clone, Debug, PartialEq)]
struct ResourceSlot {
    generation: u64,
    value: ResourceValue,
}

impl ResourceStore {
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
    pub fn insert(&mut self, value: ResourceValue) -> Result<SafeNativeHandle, ResourceError> {
        let id = self.next_id;
        let Some(next_id) = id.checked_add(1) else {
            return Err(ResourceError::new(
                "resource.id_overflow",
                "SafeNative resource id allocation overflowed.",
            ));
        };
        self.next_id = next_id;
        let handle = SafeNativeHandle { id, generation: 1 };
        self.resources.insert(
            id,
            ResourceSlot {
                generation: handle.generation,
                value,
            },
        );
        Ok(handle)
    }

    /// Returns the kind for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle supplied by bridge-side code.
    ///
    /// Output:
    /// - `Ok(kind)` when the handle is live.
    /// - `Err(ResourceError)` when the handle is stale or missing.
    ///
    /// Transformation:
    /// - Validates id/generation before exposing the stored resource kind.
    pub fn kind(&self, handle: SafeNativeHandle) -> Result<ResourceKind, ResourceError> {
        self.slot(handle).map(|slot| slot.value.kind())
    }

    /// Returns a JSON resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a JSON resource.
    ///
    /// Output:
    /// - `Ok(&Json)` for a live JSON resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn json(&self, handle: SafeNativeHandle) -> Result<&json::Json, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::Json(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Json, other.kind())),
        }
    }

    /// Returns a path resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a path resource.
    ///
    /// Output:
    /// - `Ok(&Path)` for a live path resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn path(&self, handle: SafeNativeHandle) -> Result<&path::Path, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::Path(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Path, other.kind())),
        }
    }

    /// Returns a URI resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a URI resource.
    ///
    /// Output:
    /// - `Ok(&Uri)` for a live URI resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn uri(&self, handle: SafeNativeHandle) -> Result<&uri::Uri, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::Uri(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Uri, other.kind())),
        }
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
    pub fn dispose(&mut self, handle: SafeNativeHandle) -> Result<(), ResourceError> {
        self.slot(handle)?;
        self.resources.remove(&handle.id);
        Ok(())
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
    fn slot(&self, handle: SafeNativeHandle) -> Result<&ResourceSlot, ResourceError> {
        match self.resources.get(&handle.id) {
            Some(slot) if slot.generation == handle.generation => Ok(slot),
            _ => Err(stale_error(handle)),
        }
    }
}

impl Default for ResourceStore {
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

/// Builds a stale-handle resource error.
///
/// Inputs:
/// - `handle`: rejected opaque handle.
///
/// Output:
/// - `ResourceError` with stable code `resource.stale_handle`.
///
/// Transformation:
/// - Converts a failed liveness lookup into stable diagnostic fields.
fn stale_error(handle: SafeNativeHandle) -> ResourceError {
    ResourceError::new(
        "resource.stale_handle",
        format!(
            "SafeNative resource handle {} generation {} is not live.",
            handle.id, handle.generation
        ),
    )
}

/// Builds a resource-kind mismatch error.
///
/// Inputs:
/// - `handle`: live handle whose stored resource kind is wrong.
/// - `expected`: expected resource kind.
/// - `actual`: actual resource kind.
///
/// Output:
/// - `ResourceError` with stable code `resource.kind`.
///
/// Transformation:
/// - Converts a live resource type mismatch into stable diagnostic fields.
fn kind_error(
    handle: SafeNativeHandle,
    expected: ResourceKind,
    actual: ResourceKind,
) -> ResourceError {
    ResourceError::new(
        "resource.kind",
        format!(
            "SafeNative resource handle {} is {:?}, expected {:?}.",
            handle.id, actual, expected
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Builds a JSON resource fixture.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - JSON resource wrapping a stable string value.
    ///
    /// Transformation:
    /// - Uses the JSON adapter constructor to avoid depending on raw
    ///   `serde_json` values outside tests.
    fn json_resource() -> ResourceValue {
        ResourceValue::Json(json::Json::from_serde(Value::String(String::from("Ada"))))
    }

    /// Builds a path resource fixture.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Path resource for `src/main.terl`, or an empty path after a failing
    ///   assertion.
    ///
    /// Transformation:
    /// - Converts adapter parsing into a resource value without unwrap/expect.
    fn path_resource() -> ResourceValue {
        let parsed = path::from_string("src/main.terl");
        assert!(parsed.is_ok());
        ResourceValue::Path(
            parsed.unwrap_or_else(|_| path::Path::from_path_buf(Default::default())),
        )
    }

    /// Validates resources are stored and retrieved by matching handles.
    ///
    /// Inputs:
    /// - One JSON resource.
    ///
    /// Output:
    /// - Test passes when the returned handle retrieves the same kind.
    ///
    /// Transformation:
    /// - Moves a resource into the store and checks handle-based lookup.
    #[test]
    fn insert_returns_live_handle_for_resource() {
        let mut store = ResourceStore::new();
        let result = store.insert(json_resource());
        assert!(result.is_ok());
        let Some(handle) = result.ok() else {
            return;
        };

        assert_eq!(store.kind(handle), Ok(ResourceKind::Json));
        assert!(store.json(handle).is_ok());
    }

    /// Validates resource-kind mismatches use a stable error code.
    ///
    /// Inputs:
    /// - One path resource accessed as JSON.
    ///
    /// Output:
    /// - Test passes when lookup returns `resource.kind`.
    ///
    /// Transformation:
    /// - Exercises live-handle type validation after liveness succeeds.
    #[test]
    fn rejects_wrong_resource_kind_with_stable_error_code() {
        let mut store = ResourceStore::new();
        let result = store.insert(path_resource());
        assert!(result.is_ok());
        let Some(handle) = result.ok() else {
            return;
        };

        let error = store
            .json(handle)
            .err()
            .unwrap_or_else(|| ResourceError::new("missing", ""));
        assert_eq!(error.code(), "resource.kind");
    }

    /// Validates disposed handles become stale.
    ///
    /// Inputs:
    /// - One JSON resource and its handle.
    ///
    /// Output:
    /// - Test passes when dispose succeeds once and later lookup fails stale.
    ///
    /// Transformation:
    /// - Removes a resource through a matching handle, then checks stale-handle
    ///   rejection for the old handle.
    #[test]
    fn dispose_removes_resource_and_rejects_stale_handle() {
        let mut store = ResourceStore::new();
        let result = store.insert(json_resource());
        assert!(result.is_ok());
        let Some(handle) = result.ok() else {
            return;
        };

        assert_eq!(store.dispose(handle), Ok(()));
        let error = store
            .json(handle)
            .err()
            .unwrap_or_else(|| ResourceError::new("missing", ""));
        assert_eq!(error.code(), "resource.stale_handle");
    }

    /// Validates stale generations are rejected.
    ///
    /// Inputs:
    /// - One JSON resource and a handle with a modified generation.
    ///
    /// Output:
    /// - Test passes when lookup returns `resource.stale_handle`.
    ///
    /// Transformation:
    /// - Exercises generation-tag validation without disposing the resource.
    #[test]
    fn rejects_stale_generation_with_stable_error_code() {
        let mut store = ResourceStore::new();
        let result = store.insert(json_resource());
        assert!(result.is_ok());
        let Some(mut handle) = result.ok() else {
            return;
        };
        handle.generation += 1;

        let error = store
            .json(handle)
            .err()
            .unwrap_or_else(|| ResourceError::new("missing", ""));
        assert_eq!(error.code(), "resource.stale_handle");
    }

    /// Validates id allocation overflow is rejected before insertion.
    ///
    /// Inputs:
    /// - Store whose next id is `u64::MAX`.
    ///
    /// Output:
    /// - Test passes when insertion returns `resource.id_overflow`.
    ///
    /// Transformation:
    /// - Exercises checked resource-id allocation.
    #[test]
    fn insert_rejects_id_overflow() {
        let mut store = ResourceStore {
            next_id: u64::MAX,
            resources: BTreeMap::new(),
        };

        let error = store
            .insert(json_resource())
            .err()
            .unwrap_or_else(|| ResourceError::new("missing", ""));
        assert_eq!(error.code(), "resource.id_overflow");
    }
}
