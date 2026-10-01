//! Host-owned actor state primitives for package services.

use crate::BoundaryError;

/// A package can own policy over actor-owned keyed state without importing VM
/// process/table types. Handles are host-scoped and release is idempotent.
/// Every access must reject foreign or stale handles. Release must still clean
/// resources after the owning actor exits; it must not affect other owners.
/// This interface manages resources; it does not run a package's actor loop.
pub trait ActorStateStore<V>: Send {
    type Handle: Clone + Send;

    fn create(&mut self, owner: &str, name: &str) -> Result<Self::Handle, BoundaryError>;
    fn check_live(&self, handle: &Self::Handle) -> Result<(), BoundaryError>;
    fn release(&mut self, handle: &Self::Handle) -> Result<(), BoundaryError>;
    fn read(&self, handle: &Self::Handle, key: &str) -> Result<Option<V>, BoundaryError>;
    fn write(&mut self, handle: &Self::Handle, key: &str, value: V) -> Result<(), BoundaryError>;
    fn delete(&mut self, handle: &Self::Handle, key: &str) -> Result<(), BoundaryError>;
}
