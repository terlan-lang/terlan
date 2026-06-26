//! Opaque resource registry for SafeNative adapter-owned values.
//!
//! Terlan/BEAM terms must not carry Rust adapter values directly. This module
//! owns those values behind generation-tagged handles so the runtime bridge can
//! pass only stable opaque identifiers across process or language boundaries.

use std::collections::BTreeMap;

use crate::handle::SafeNativeHandle;
use crate::{http, json, path, postgres, uri, vector};

/// Resource kind stored in the SafeNative registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    /// `std.data.Json.Json`.
    Json,
    /// `std.http.Request.Request`.
    HttpRequest,
    /// `std.http.Response.Response`.
    HttpResponse,
    /// `std.http.Cookies.Jar`.
    HttpCookieJar,
    /// `std.io.Path.Path`.
    Path,
    /// `std.net.Uri.Uri`.
    Uri,
    /// `std.db.Postgres.Pool`.
    PostgresPool,
    /// `std.db.Postgres.Row`.
    PostgresRow,
    /// `std.native.collections.Vector.Vector[T]`.
    NativeVector,
}

/// Adapter-owned opaque resource value.
#[derive(Clone, Debug, PartialEq)]
pub enum ResourceValue {
    /// JSON resource owned by the Rust JSON adapter.
    Json(json::Json),
    /// HTTP request resource owned by the Rust HTTP adapter.
    HttpRequest(http::Request),
    /// HTTP response resource owned by the Rust HTTP adapter.
    HttpResponse(http::Response),
    /// HTTP cookie jar resource owned by the Rust HTTP adapter.
    HttpCookieJar(http::CookieJar),
    /// Path resource owned by the Rust path adapter.
    Path(path::Path),
    /// URI resource owned by the Rust URI adapter.
    Uri(uri::Uri),
    /// Postgres pool resource owned by the Rust Postgres adapter.
    PostgresPool(postgres::Pool),
    /// Postgres row resource owned by the Rust Postgres adapter.
    PostgresRow(postgres::Row),
    /// Native vector resource owned by the Rust vector adapter.
    NativeVector(vector::NativeVector),
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
            Self::HttpRequest(_) => ResourceKind::HttpRequest,
            Self::HttpResponse(_) => ResourceKind::HttpResponse,
            Self::HttpCookieJar(_) => ResourceKind::HttpCookieJar,
            Self::Path(_) => ResourceKind::Path,
            Self::Uri(_) => ResourceKind::Uri,
            Self::PostgresPool(_) => ResourceKind::PostgresPool,
            Self::PostgresRow(_) => ResourceKind::PostgresRow,
            Self::NativeVector(_) => ResourceKind::NativeVector,
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

/// Live resource entry stored behind a generation-tagged handle.
///
/// Inputs:
/// - `generation`: handle generation that must match before a resource can be
///   borrowed or removed.
/// - `value`: adapter-owned resource payload.
///
/// Output:
/// - Internal registry slot consumed only by `ResourceStore`.
///
/// Transformation:
/// - Keeps liveness metadata beside the owned resource value so stale handles
///   cannot access a removed or replaced resource.
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

    /// Returns an HTTP request resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify an HTTP request
    ///   resource.
    ///
    /// Output:
    /// - `Ok(&Request)` for a live HTTP request resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn http_request(&self, handle: SafeNativeHandle) -> Result<&http::Request, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::HttpRequest(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::HttpRequest, other.kind())),
        }
    }

    /// Returns an HTTP response resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify an HTTP response
    ///   resource.
    ///
    /// Output:
    /// - `Ok(&Response)` for a live HTTP response resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn http_response(
        &self,
        handle: SafeNativeHandle,
    ) -> Result<&http::Response, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::HttpResponse(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::HttpResponse, other.kind())),
        }
    }

    /// Returns an HTTP cookie jar resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify an HTTP cookie jar
    ///   resource.
    ///
    /// Output:
    /// - `Ok(&CookieJar)` for a live cookie jar resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn http_cookie_jar(
        &self,
        handle: SafeNativeHandle,
    ) -> Result<&http::CookieJar, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::HttpCookieJar(value) => Ok(value),
            other => Err(kind_error(
                handle,
                ResourceKind::HttpCookieJar,
                other.kind(),
            )),
        }
    }

    /// Returns a mutable HTTP cookie jar resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify an HTTP cookie jar
    ///   resource.
    ///
    /// Output:
    /// - `Ok(&mut CookieJar)` for a live cookie jar resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before mutably borrowing the
    ///   value for receiver-method updates.
    pub fn http_cookie_jar_mut(
        &mut self,
        handle: SafeNativeHandle,
    ) -> Result<&mut http::CookieJar, ResourceError> {
        match &mut self.slot_mut(handle)?.value {
            ResourceValue::HttpCookieJar(value) => Ok(value),
            other => Err(kind_error(
                handle,
                ResourceKind::HttpCookieJar,
                other.kind(),
            )),
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

    /// Returns a Postgres pool resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a Postgres pool.
    ///
    /// Output:
    /// - `Ok(&Pool)` for a live Postgres pool resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn postgres_pool(
        &self,
        handle: SafeNativeHandle,
    ) -> Result<&postgres::Pool, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::PostgresPool(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::PostgresPool, other.kind())),
        }
    }

    /// Returns a Postgres row resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a Postgres row.
    ///
    /// Output:
    /// - `Ok(&Row)` for a live Postgres row resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn postgres_row(&self, handle: SafeNativeHandle) -> Result<&postgres::Row, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::PostgresRow(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::PostgresRow, other.kind())),
        }
    }

    /// Returns a native vector resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a native vector.
    ///
    /// Output:
    /// - `Ok(&NativeVector)` for a live native vector resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before borrowing the value.
    pub fn native_vector(
        &self,
        handle: SafeNativeHandle,
    ) -> Result<&vector::NativeVector, ResourceError> {
        match &self.slot(handle)?.value {
            ResourceValue::NativeVector(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::NativeVector, other.kind())),
        }
    }

    /// Returns a mutable native vector resource for a live handle.
    ///
    /// Inputs:
    /// - `handle`: opaque handle expected to identify a native vector.
    ///
    /// Output:
    /// - `Ok(&mut NativeVector)` for a live native vector resource.
    /// - `Err(ResourceError)` for stale handles or kind mismatches.
    ///
    /// Transformation:
    /// - Validates liveness and resource kind before mutably borrowing the
    ///   vector for indexed updates.
    pub fn native_vector_mut(
        &mut self,
        handle: SafeNativeHandle,
    ) -> Result<&mut vector::NativeVector, ResourceError> {
        match &mut self.slot_mut(handle)?.value {
            ResourceValue::NativeVector(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::NativeVector, other.kind())),
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
    fn slot_mut(&mut self, handle: SafeNativeHandle) -> Result<&mut ResourceSlot, ResourceError> {
        match self.resources.get_mut(&handle.id) {
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
#[path = "resource_test.rs"]
mod resource_test;
