//! Linear transfer of one actor's logical memory-accounting state.

use std::collections::BTreeSet;
use std::fmt;

use super::{
    VmAccountedResourceOwnership, VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureDecision,
    VmProcessMemoryMetrics, VmSharedAllocation,
};
use crate::runtime::vm::process::VmProcessId;

/// Complete actor-scoped memory state detached at a migration safepoint.
#[derive(Debug)]
pub(crate) struct VmMemoryTransfer {
    owner: VmProcessId,
    limits: VmMemoryLimits,
    metrics: Option<VmProcessMemoryMetrics>,
    decisions: Vec<VmMemoryPressureDecision>,
    resource_ownership: Vec<VmAccountedResourceOwnership>,
    shared_allocations: Vec<VmSharedAllocation>,
    shared_identity_watermark: u64,
}

impl VmMemoryTransfer {
    /// Returns the actor that owns all process-local records in this transfer.
    pub(crate) const fn owner(&self) -> VmProcessId {
        self.owner
    }

    /// Returns the current heap bytes represented by process metrics.
    pub(crate) fn current_bytes(&self) -> usize {
        self.metrics
            .as_ref()
            .map_or(0, |metrics| metrics.current_bytes)
    }

    /// Returns exact resource identities carrying logical memory charges.
    pub(crate) fn resource_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.resource_ownership
            .iter()
            .map(|ownership| ownership.resource_id)
    }
}

/// Failed memory import retaining every record for source rollback.
#[derive(Debug)]
pub(crate) struct VmMemoryImportFailure {
    reason: String,
    transfer: VmMemoryTransfer,
}

impl VmMemoryImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the complete memory state for source restoration.
    pub(crate) fn into_transfer(self) -> VmMemoryTransfer {
        self.transfer
    }
}

impl fmt::Display for VmMemoryImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmMemoryImportFailure {}

impl VmMemoryAccountant {
    /// Validates that one actor's memory graph can be detached independently.
    pub(crate) fn validate_memory_detach(
        &self,
        owner: VmProcessId,
        process_heap_bytes: usize,
        resource_ids: impl IntoIterator<Item = u64>,
    ) -> Result<(), String> {
        let resource_ids = resource_ids.into_iter().collect::<BTreeSet<_>>();
        let accounted_ids = self
            .resource_ownership
            .values()
            .filter_map(|record| (record.owner == owner.as_u64()).then_some(record.resource_id))
            .collect::<BTreeSet<_>>();
        if resource_ids != accounted_ids {
            return Err(format!(
                "actor transfer process {} resource memory graph mismatch",
                owner.as_u64()
            ));
        }
        let current_bytes = self
            .processes
            .get(&owner.as_u64())
            .map_or(0, |metrics| metrics.current_bytes);
        if current_bytes != process_heap_bytes {
            return Err(format!(
                "actor transfer process {} heap accounting mismatch: process {}, memory {}",
                owner.as_u64(),
                process_heap_bytes,
                current_bytes
            ));
        }
        if self.shared_allocations.values().any(|allocation| {
            allocation.owners.contains(&owner.as_u64())
                && allocation.owners != BTreeSet::from([owner.as_u64()])
        }) {
            return Err(format!(
                "actor transfer process {} has a cross-actor shared allocation",
                owner.as_u64()
            ));
        }
        Ok(())
    }

    /// Detaches validated process-local accounting without remapping identities.
    pub(crate) fn detach_owner_memory(&mut self, owner: VmProcessId) -> VmMemoryTransfer {
        let metrics = self.processes.remove(&owner.as_u64());
        let (decisions, retained_decisions) = self
            .decisions
            .drain(..)
            .partition(|decision| decision.pid == owner.as_u64());
        self.decisions = retained_decisions;
        let resource_ids = self
            .resource_ownership
            .values()
            .filter_map(|record| (record.owner == owner.as_u64()).then_some(record.resource_id))
            .collect::<Vec<_>>();
        let resource_ownership = resource_ids
            .into_iter()
            .map(|identity| {
                self.resource_ownership
                    .remove(&identity)
                    .expect("inventoried resource memory record remains present")
            })
            .collect();
        let shared_ids = self
            .shared_allocations
            .values()
            .filter_map(|record| record.owners.contains(&owner.as_u64()).then_some(record.id))
            .collect::<Vec<_>>();
        let shared_allocations = shared_ids
            .into_iter()
            .map(|identity| {
                self.shared_allocations
                    .remove(&identity)
                    .expect("validated actor-local allocation remains present")
            })
            .collect();
        VmMemoryTransfer {
            owner,
            limits: self.limits,
            metrics,
            decisions,
            resource_ownership,
            shared_allocations,
            shared_identity_watermark: self.next_shared_allocation_id,
        }
    }

    /// Validates limits, owners, and destination identities before mutation.
    pub(crate) fn validate_memory_import(
        &self,
        transfer: &VmMemoryTransfer,
        process_heap_bytes: usize,
    ) -> Result<(), String> {
        if transfer.owner.as_u64() == 0 {
            return Err("memory transfer owner identity must be nonzero".to_string());
        }
        if self.limits != transfer.limits {
            return Err("memory transfer limits differ from destination limits".to_string());
        }
        if transfer.current_bytes() != process_heap_bytes {
            return Err(format!(
                "memory transfer heap mismatch: process {}, memory {}",
                process_heap_bytes,
                transfer.current_bytes()
            ));
        }
        if self.processes.contains_key(&transfer.owner.as_u64()) {
            return Err(format!(
                "memory transfer destination already contains process {} metrics",
                transfer.owner.as_u64()
            ));
        }
        if transfer
            .metrics
            .as_ref()
            .is_some_and(|metrics| metrics.pid != transfer.owner.as_u64())
            || transfer
                .decisions
                .iter()
                .any(|decision| decision.pid != transfer.owner.as_u64())
            || transfer
                .resource_ownership
                .iter()
                .any(|record| record.owner != transfer.owner.as_u64())
            || transfer
                .shared_allocations
                .iter()
                .any(|record| record.owners != BTreeSet::from([transfer.owner.as_u64()]))
        {
            return Err("memory transfer contains cross-actor state".to_string());
        }
        if let Some(identity) = transfer
            .resource_ownership
            .iter()
            .map(|record| record.resource_id)
            .find(|identity| self.resource_ownership.contains_key(identity))
        {
            return Err(format!(
                "memory transfer destination already contains resource {identity} ownership"
            ));
        }
        if let Some(identity) = transfer
            .shared_allocations
            .iter()
            .map(|record| record.id)
            .find(|identity| self.shared_allocations.contains_key(identity))
        {
            return Err(format!(
                "memory transfer destination already contains shared allocation {identity}"
            ));
        }
        Ok(())
    }

    /// Imports actor-local accounting or returns it unchanged for rollback.
    pub(crate) fn import_memory_transfer(
        &mut self,
        transfer: VmMemoryTransfer,
        process_heap_bytes: usize,
    ) -> Result<(), VmMemoryImportFailure> {
        if let Err(reason) = self.validate_memory_import(&transfer, process_heap_bytes) {
            return Err(VmMemoryImportFailure { reason, transfer });
        }
        self.next_shared_allocation_id = self
            .next_shared_allocation_id
            .max(transfer.shared_identity_watermark);
        if let Some(metrics) = transfer.metrics {
            self.processes.insert(transfer.owner.as_u64(), metrics);
        }
        self.decisions.extend(transfer.decisions);
        for record in transfer.resource_ownership {
            self.resource_ownership.insert(record.resource_id, record);
        }
        for record in transfer.shared_allocations {
            self.shared_allocations.insert(record.id, record);
        }
        Ok(())
    }
}
