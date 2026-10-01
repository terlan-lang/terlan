//! Opaque resource registry for NativeBoundary adapter-owned values.
//!
//! Terlan/VM terms must not carry Rust adapter values directly. This module
//! owns those values behind generation-tagged handles so the runtime bridge can
//! pass only stable opaque identifiers across process or language boundaries.

use crate::terlan_native::{json, path, postgres, random, regex, vector};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;

mod json_store;

pub use terlan_runtime_abi::{ResourceError, ResourceRegistry, SYSTEM_RESOURCE_OWNER};

/// Resource kind stored in the NativeBoundary registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    /// `std.random.Random.Generator`.
    RandomGenerator,
    /// `std.data.Json.Json`.
    Json,
    /// `std.regex.Regex.Regex`.
    Regex,
    /// `std.io.Path.Path`.
    Path,
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
    /// Immutable random generator state owned by the Rust random adapter.
    RandomGenerator(Box<random::Generator>),
    /// JSON resource owned by the Rust JSON adapter.
    Json(json::Json),
    /// Compiled regex resource owned by the Rust regex adapter.
    Regex(regex::Regex),
    /// Path resource owned by the Rust path adapter.
    Path(path::Path),
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
            Self::RandomGenerator(_) => ResourceKind::RandomGenerator,
            Self::Json(_) => ResourceKind::Json,
            Self::Regex(_) => ResourceKind::Regex,
            Self::Path(_) => ResourceKind::Path,
            Self::PostgresPool(_) => ResourceKind::PostgresPool,
            Self::PostgresRow(_) => ResourceKind::PostgresRow,
            Self::NativeVector(_) => ResourceKind::NativeVector,
        }
    }
}

/// Adapter registry using the shared owner-checked storage implementation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceStore(ResourceRegistry<ResourceValue>);

impl std::ops::Deref for ResourceStore {
    type Target = ResourceRegistry<ResourceValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ResourceStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ResourceStore {
    /// Builds the legacy adapter facade over the shared resource registry.
    pub fn new() -> Self {
        Self::default()
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
    pub fn kind(&self, handle: NativeBoundaryHandle) -> Result<ResourceKind, ResourceError> {
        self.get(handle).map(ResourceValue::kind)
    }

    /// Borrows immutable RNG state after validating handle liveness and kind.
    pub fn random_generator(
        &self,
        handle: NativeBoundaryHandle,
    ) -> Result<&random::Generator, ResourceError> {
        match self.get(handle)? {
            ResourceValue::RandomGenerator(value) => Ok(value),
            other => Err(kind_error(
                handle,
                ResourceKind::RandomGenerator,
                other.kind(),
            )),
        }
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
    pub fn json(&self, handle: NativeBoundaryHandle) -> Result<&json::Json, ResourceError> {
        match self.get(handle)? {
            ResourceValue::Json(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Json, other.kind())),
        }
    }

    /// Returns a mutable JSON resource for a live handle.
    pub fn json_mut(
        &mut self,
        handle: NativeBoundaryHandle,
    ) -> Result<&mut json::Json, ResourceError> {
        match self.get_mut(handle)? {
            ResourceValue::Json(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Json, other.kind())),
        }
    }

    /// Returns a compiled regex resource for a live handle.
    pub fn regex(&self, handle: NativeBoundaryHandle) -> Result<&regex::Regex, ResourceError> {
        match self.get(handle)? {
            ResourceValue::Regex(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Regex, other.kind())),
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
    pub fn path(&self, handle: NativeBoundaryHandle) -> Result<&path::Path, ResourceError> {
        match self.get(handle)? {
            ResourceValue::Path(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::Path, other.kind())),
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
        handle: NativeBoundaryHandle,
    ) -> Result<&postgres::Pool, ResourceError> {
        match self.get(handle)? {
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
    pub fn postgres_row(
        &self,
        handle: NativeBoundaryHandle,
    ) -> Result<&postgres::Row, ResourceError> {
        match self.get(handle)? {
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
        handle: NativeBoundaryHandle,
    ) -> Result<&vector::NativeVector, ResourceError> {
        match self.get(handle)? {
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
        handle: NativeBoundaryHandle,
    ) -> Result<&mut vector::NativeVector, ResourceError> {
        match self.get_mut(handle)? {
            ResourceValue::NativeVector(value) => Ok(value),
            other => Err(kind_error(handle, ResourceKind::NativeVector, other.kind())),
        }
    }
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
    handle: NativeBoundaryHandle,
    expected: ResourceKind,
    actual: ResourceKind,
) -> ResourceError {
    ResourceError::new(
        "resource.kind",
        format!(
            "NativeBoundary resource handle {} is {:?}, expected {:?}.",
            handle.id, actual, expected
        ),
    )
}

#[cfg(test)]
#[path = "resource_test.rs"]
mod resource_test;
