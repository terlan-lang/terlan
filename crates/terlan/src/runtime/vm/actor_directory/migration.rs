//! Generation-qualified authority captured at actor migration boundaries.

use super::VmActorHandle;

/// Generation stamp captured only while an actor is unowned and migrating.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorMigrationStamp {
    pub(super) handle: VmActorHandle,
    pub(super) owner_generation: u64,
}

impl VmActorMigrationStamp {
    /// Returns the exact actor generation authorized for transfer.
    pub(crate) const fn handle(self) -> VmActorHandle {
        self.handle
    }

    /// Returns the last scheduler ownership generation before transfer.
    pub(crate) const fn owner_generation(self) -> u64 {
        self.owner_generation
    }
}
