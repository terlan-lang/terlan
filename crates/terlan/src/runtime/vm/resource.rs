#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};

#[path = "resource/transfer.rs"]
pub(crate) mod transfer;

/// VM-owned native resource identifier.
///
/// Inputs:
/// - Monotonic runtime allocation.
///
/// Output:
/// - Stable handle id used by NativeBoundary resource references.
///
/// Transformation:
/// - Keeps Terlan resource identity independent from host pointers, NIF
///   environments, file descriptors, or library-specific handle values.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmResourceId(u64);

impl VmResourceId {
    /// Returns the numeric resource id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// Resource ownership transfer policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmResourceTransferPolicy {
    OwnerOnly,
    Transferable,
}

/// Native resource metadata recorded for inspection and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmResourceDescriptor {
    pub(crate) kind: String,
    pub(crate) label: String,
}

impl VmResourceDescriptor {
    /// Creates resource metadata for inspection rows.
    pub(crate) fn new(kind: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            label: label.into(),
        }
    }
}

/// Live resource row owned by the VM runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmResourceRecord {
    pub(crate) id: VmResourceId,
    pub(crate) owner: VmProcessId,
    pub(crate) descriptor: VmResourceDescriptor,
    pub(crate) transfer_policy: VmResourceTransferPolicy,
}

/// Read-only resource row for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmResourceSnapshot {
    pub(crate) id: VmResourceId,
    pub(crate) owner: VmProcessId,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) transfer_policy: VmResourceTransferPolicy,
}

/// Resource lifecycle event emitted by ownership operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmResourceEvent {
    Registered {
        id: VmResourceId,
        owner: VmProcessId,
    },
    Transferred {
        id: VmResourceId,
        from: VmProcessId,
        to: VmProcessId,
    },
    Released {
        id: VmResourceId,
        owner: VmProcessId,
    },
    CleanedUpOnExit {
        id: VmResourceId,
        owner: VmProcessId,
    },
}

/// VM-owned NativeBoundary resource table.
///
/// Inputs:
/// - Live VM processes and resource lifecycle requests.
///
/// Output:
/// - Validated resource handles with owner identity, transfer rules, cleanup
///   events, and inspection rows.
///
/// Transformation:
/// - Ensures native resources are represented as typed VM handles instead of
///   leaking raw host handles into Terlan code.
#[derive(Debug, Default)]
pub(crate) struct VmResourceTable {
    next_id: u64,
    resources: BTreeMap<VmResourceId, VmResourceRecord>,
}

impl VmResourceTable {
    /// Registers a new resource owned by a live VM process.
    pub(crate) fn register(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        descriptor: VmResourceDescriptor,
        transfer_policy: VmResourceTransferPolicy,
    ) -> Result<VmResourceEvent, String> {
        ensure_live_process(processes, owner, "owner")?;

        self.next_id = self.next_id.saturating_add(1);
        let id = VmResourceId(self.next_id);
        processes.with_process_control_mutator(owner, |process| {
            process.add_resource_handle(resource_handle_name(id));
        })?;
        self.resources.insert(
            id,
            VmResourceRecord {
                id,
                owner,
                descriptor,
                transfer_policy,
            },
        );
        Ok(VmResourceEvent::Registered { id, owner })
    }

    /// Returns a resource record if the requester is the current owner.
    pub(crate) fn get_for_owner(
        &self,
        requester: VmProcessId,
        resource: VmResourceId,
    ) -> Result<&VmResourceRecord, String> {
        let record = self.live_resource(resource)?;
        if record.owner != requester {
            return Err(format!(
                "resource {} is owned by process {}, not {}",
                resource.as_u64(),
                record.owner.as_u64(),
                requester.as_u64()
            ));
        }
        Ok(record)
    }

    /// Transfers an owned resource to another live process when policy allows.
    pub(crate) fn transfer(
        &mut self,
        processes: &mut VmProcessTable,
        resource: VmResourceId,
        from: VmProcessId,
        to: VmProcessId,
    ) -> Result<VmResourceEvent, String> {
        self.validate_transfer(processes, resource, from, to)?;

        let record = self
            .resources
            .get_mut(&resource)
            .expect("resource was validated before transfer mutation");
        let handle = resource_handle_name(resource);
        processes.with_process_control_mutator(from, |process| {
            process.remove_resource_handle(&handle);
        })?;
        processes.with_process_control_mutator(to, |process| {
            process.add_resource_handle(handle);
        })?;
        record.owner = to;
        Ok(VmResourceEvent::Transferred {
            id: resource,
            from,
            to,
        })
    }

    /// Validates resource transfer without mutating ownership state.
    pub(crate) fn validate_transfer(
        &self,
        processes: &VmProcessTable,
        resource: VmResourceId,
        from: VmProcessId,
        to: VmProcessId,
    ) -> Result<(), String> {
        ensure_live_process(processes, from, "source")?;
        ensure_live_process(processes, to, "target")?;

        let record = self.live_resource(resource)?;
        if record.owner != from {
            return Err(format!(
                "resource {} is owned by process {}, not {}",
                resource.as_u64(),
                record.owner.as_u64(),
                from.as_u64()
            ));
        }
        if record.transfer_policy != VmResourceTransferPolicy::Transferable {
            return Err(format!(
                "resource {} cannot be transferred",
                resource.as_u64()
            ));
        }
        Ok(())
    }

    /// Releases a resource by its current owner.
    pub(crate) fn release(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        resource: VmResourceId,
    ) -> Result<VmResourceEvent, String> {
        self.validate_release(processes, owner, resource)?;

        self.resources.remove(&resource);
        processes.with_process_control_mutator(owner, |process| {
            process.remove_resource_handle(&resource_handle_name(resource));
        })?;
        Ok(VmResourceEvent::Released {
            id: resource,
            owner,
        })
    }

    /// Validates resource release without mutating ownership state.
    pub(crate) fn validate_release(
        &self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        resource: VmResourceId,
    ) -> Result<(), String> {
        ensure_live_process(processes, owner, "owner")?;
        let record = self.live_resource(resource)?;
        if record.owner != owner {
            return Err(format!(
                "resource {} is owned by process {}, not {}",
                resource.as_u64(),
                record.owner.as_u64(),
                owner.as_u64()
            ));
        }
        Ok(())
    }

    /// Cleans up every live resource owned by an exiting process.
    pub(crate) fn cleanup_owner(&mut self, owner: VmProcessId) -> Vec<VmResourceEvent> {
        let owned: Vec<VmResourceId> = self
            .resources
            .iter()
            .filter_map(|(id, record)| (record.owner == owner).then_some(*id))
            .collect();

        owned
            .into_iter()
            .filter_map(|id| {
                self.resources
                    .remove(&id)
                    .map(|record| VmResourceEvent::CleanedUpOnExit {
                        id,
                        owner: record.owner,
                    })
            })
            .collect()
    }

    /// Cleans up owned resources and removes matching process handle rows when
    /// cleanup is triggered before the process table exit path runs.
    pub(crate) fn cleanup_owner_handles(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
    ) -> Vec<VmResourceEvent> {
        let owned_ids = self
            .resources
            .values()
            .filter_map(|record| (record.owner == owner).then_some(record.id))
            .collect::<Vec<_>>();
        let events = self.cleanup_owner(owner);
        if processes.get(owner).is_some() {
            let _ = processes.with_process_control_mutator(owner, |process| {
                for id in owned_ids {
                    process.remove_resource_handle(&resource_handle_name(id));
                }
            });
        }
        events
    }

    /// Returns live resource rows for runtime inspection.
    pub(crate) fn snapshots(&self) -> Vec<VmResourceSnapshot> {
        self.resources.values().map(resource_snapshot).collect()
    }

    /// Returns deterministic live resource rows for one process owner.
    pub(crate) fn snapshots_for_owner(&self, owner: VmProcessId) -> Vec<VmResourceSnapshot> {
        self.resources
            .values()
            .filter(|record| record.owner == owner)
            .map(resource_snapshot)
            .collect()
    }

    fn live_resource(&self, resource: VmResourceId) -> Result<&VmResourceRecord, String> {
        self.resources
            .get(&resource)
            .ok_or_else(|| stale_resource_diagnostic(resource))
    }
}

fn resource_snapshot(record: &VmResourceRecord) -> VmResourceSnapshot {
    VmResourceSnapshot {
        id: record.id,
        owner: record.owner,
        kind: record.descriptor.kind.clone(),
        label: record.descriptor.label.clone(),
        transfer_policy: record.transfer_policy,
    }
}

fn ensure_live_process(
    processes: &VmProcessTable,
    pid: VmProcessId,
    role: &str,
) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("missing {role} process {}", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!("{role} process {} has exited", pid.as_u64()));
    }
    Ok(())
}

fn resource_handle_name(resource: VmResourceId) -> String {
    format!("resource:{}", resource.as_u64())
}

fn stale_resource_diagnostic(resource: VmResourceId) -> String {
    format!("stale native resource handle {}", resource.as_u64())
}

#[cfg(test)]
#[path = "resource_cancellation_test.rs"]
mod resource_cancellation_test;

#[cfg(test)]
#[path = "resource_owner_test.rs"]
mod resource_owner_test;

#[cfg(test)]
#[path = "resource_test.rs"]
mod resource_test;

#[cfg(test)]
#[path = "resource_transfer_test.rs"]
mod resource_transfer_test;
