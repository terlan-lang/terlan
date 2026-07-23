use std::collections::BTreeSet;

use super::{
    require_live_process, stale_shared_allocation, VmMemoryAccountant, VmMemoryPressureOutcome,
    VmSharedAllocation, VmSharedAllocationDecision, VmSharedAllocationId, VmSharedAllocationKind,
};
use crate::runtime::vm::process::{VmProcessId, VmProcessTable};

impl VmMemoryAccountant {
    pub(crate) fn register_shared_allocation(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        kind: VmSharedAllocationKind,
        logical_bytes: usize,
    ) -> Result<VmSharedAllocationDecision, String> {
        let next_id = self
            .next_shared_allocation_id
            .checked_add(1)
            .ok_or_else(|| "VM shared allocation id overflow".to_string())?;
        let pressure = self.account_heap(processes, owner, logical_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmSharedAllocationDecision {
                allocation_id: None,
                pressure,
            });
        }
        self.next_shared_allocation_id = next_id;
        let id = VmSharedAllocationId(next_id);
        self.shared_allocations.insert(
            next_id,
            VmSharedAllocation {
                id: next_id,
                kind,
                logical_bytes,
                owners: BTreeSet::from([owner.as_u64()]),
            },
        );
        Ok(VmSharedAllocationDecision {
            allocation_id: Some(id),
            pressure,
        })
    }

    pub(crate) fn retain_shared_allocation(
        &mut self,
        processes: &mut VmProcessTable,
        allocation: VmSharedAllocationId,
        current_owner: VmProcessId,
        new_owner: VmProcessId,
    ) -> Result<VmSharedAllocationDecision, String> {
        require_live_process(processes, current_owner)?;
        require_live_process(processes, new_owner)?;
        let record = self
            .shared_allocations
            .get(&allocation.as_u64())
            .ok_or_else(|| stale_shared_allocation(allocation))?;
        if !record.owners.contains(&current_owner.as_u64()) {
            return Err(format!(
                "shared allocation {} is not owned by process {}",
                allocation.as_u64(),
                current_owner.as_u64()
            ));
        }
        let logical_bytes = if record.owners.contains(&new_owner.as_u64()) {
            0
        } else {
            record.logical_bytes
        };
        let pressure = self.account_heap(processes, new_owner, logical_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmSharedAllocationDecision {
                allocation_id: None,
                pressure,
            });
        }
        self.shared_allocations
            .get_mut(&allocation.as_u64())
            .expect("shared allocation was validated before retain mutation")
            .owners
            .insert(new_owner.as_u64());
        Ok(VmSharedAllocationDecision {
            allocation_id: Some(allocation),
            pressure,
        })
    }

    pub(crate) fn release_shared_allocation(
        &mut self,
        processes: &mut VmProcessTable,
        allocation: VmSharedAllocationId,
        owner: VmProcessId,
    ) -> Result<bool, String> {
        require_live_process(processes, owner)?;
        let record = self
            .shared_allocations
            .get(&allocation.as_u64())
            .ok_or_else(|| stale_shared_allocation(allocation))?;
        if !record.owners.contains(&owner.as_u64()) {
            return Err(format!(
                "shared allocation {} is not owned by process {}",
                allocation.as_u64(),
                owner.as_u64()
            ));
        }
        let logical_bytes = record.logical_bytes;
        self.release_heap(processes, owner, logical_bytes)?;
        let record = self
            .shared_allocations
            .get_mut(&allocation.as_u64())
            .expect("shared allocation was validated before release mutation");
        record.owners.remove(&owner.as_u64());
        let deallocated = record.owners.is_empty();
        if deallocated {
            self.shared_allocations.remove(&allocation.as_u64());
        }
        Ok(deallocated)
    }

    pub(crate) fn release_shared_allocations(
        &mut self,
        processes: &mut VmProcessTable,
        allocations: &[VmSharedAllocationId],
        owner: VmProcessId,
    ) -> Result<usize, String> {
        require_live_process(processes, owner)?;
        let mut seen = BTreeSet::new();
        let mut logical_bytes = 0usize;
        for allocation in allocations {
            if !seen.insert(allocation.as_u64()) {
                return Err(format!(
                    "duplicate VM shared allocation {} in bulk release",
                    allocation.as_u64()
                ));
            }
            let record = self
                .shared_allocations
                .get(&allocation.as_u64())
                .ok_or_else(|| stale_shared_allocation(*allocation))?;
            if !record.owners.contains(&owner.as_u64()) {
                return Err(format!(
                    "shared allocation {} is not owned by process {}",
                    allocation.as_u64(),
                    owner.as_u64()
                ));
            }
            logical_bytes = logical_bytes
                .checked_add(record.logical_bytes)
                .ok_or_else(|| "VM shared allocation bulk release byte overflow".to_string())?;
        }
        let heap_bytes = processes
            .get(owner)
            .expect("shared allocation owner was validated")
            .heap_bytes;
        if heap_bytes < logical_bytes {
            return Err(format!(
                "process {} shared allocation bytes {} exceed accounted heap {}",
                owner.as_u64(),
                logical_bytes,
                heap_bytes
            ));
        }
        self.release_heap(processes, owner, logical_bytes)?;
        let mut deallocated = 0usize;
        for allocation in allocations {
            let record = self
                .shared_allocations
                .get_mut(&allocation.as_u64())
                .expect("bulk shared allocation release was validated before mutation");
            record.owners.remove(&owner.as_u64());
            if record.owners.is_empty() {
                self.shared_allocations.remove(&allocation.as_u64());
                deallocated += 1;
            }
        }
        Ok(deallocated)
    }

    pub(crate) fn reclassify_shared_allocation(
        &mut self,
        allocation: VmSharedAllocationId,
        owner: VmProcessId,
        expected: VmSharedAllocationKind,
        replacement: VmSharedAllocationKind,
    ) -> Result<(), String> {
        let record = self
            .shared_allocations
            .get_mut(&allocation.as_u64())
            .ok_or_else(|| stale_shared_allocation(allocation))?;
        if !record.owners.contains(&owner.as_u64()) {
            return Err(format!(
                "shared allocation {} is not owned by process {}",
                allocation.as_u64(),
                owner.as_u64()
            ));
        }
        if record.kind != expected {
            return Err(format!(
                "shared allocation {} has kind {:?}, expected {:?}",
                allocation.as_u64(),
                record.kind,
                expected
            ));
        }
        record.kind = replacement;
        Ok(())
    }

    pub(crate) fn shared_allocation_kind(
        &self,
        allocation: VmSharedAllocationId,
    ) -> Option<VmSharedAllocationKind> {
        self.shared_allocation(allocation).map(|record| record.kind)
    }

    pub(super) fn shared_allocation(
        &self,
        allocation: VmSharedAllocationId,
    ) -> Option<&VmSharedAllocation> {
        self.shared_allocations.get(&allocation.as_u64())
    }
}
