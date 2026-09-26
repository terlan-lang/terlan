//! Aggregate ownership bounds include immutable views and reserved volatile capacity.

use super::*;
use std::collections::BTreeMap;

pub(super) const LOCAL_BYTES: usize = 16 * 1024 * 1024;
pub(super) const LOCAL_CHECKPOINTS: usize = 1024;
const OWNER_BYTES: usize = 64 * 1024 * 1024;
const RUNTIME_BYTES: usize = 256 * 1024 * 1024;
const OWNER_RESOURCES: usize = 4096;
const RUNTIME_RESOURCES: usize = 16384;

/// Logical allocation budget, not an allocator-specific resident-memory measurement.
#[derive(Default)]
pub(super) struct Budget {
    owners: BTreeMap<u64, (usize, usize)>,
    resources: usize,
    bytes: usize,
}

impl Budget {
    /// Checks without charging; callers on this owner thread cannot interleave mutation.
    pub(super) fn check(&self, owner: u64, bytes: usize) -> VmRuntimeResult<()> {
        let (count, retained) = self.owners.get(&owner).copied().unwrap_or_default();
        if count >= OWNER_RESOURCES
            || self.resources >= RUNTIME_RESOURCES
            || bytes > OWNER_BYTES.saturating_sub(retained)
            || bytes > RUNTIME_BYTES.saturating_sub(self.bytes)
        {
            return Err(
                "error[vm.distributed_storage.capacity]: storage owner/runtime budget exhausted"
                    .into(),
            );
        }
        Ok(())
    }

    /// Charges only a successfully registered resource after the capacity check.
    pub(super) fn charge(&mut self, owner: u64, bytes: usize) {
        let entry = self.owners.entry(owner).or_default();
        entry.0 += 1;
        entry.1 += bytes;
        self.resources += 1;
        self.bytes += bytes;
    }

    /// Releases descriptors, views, and reserved store capacity together at actor exit.
    pub(super) fn release(&mut self, owner: u64) {
        if let Some((count, bytes)) = self.owners.remove(&owner) {
            self.resources -= count;
            self.bytes -= bytes;
        }
    }
}

#[cfg(test)]
#[path = "budget_test.rs"]
mod tests;
