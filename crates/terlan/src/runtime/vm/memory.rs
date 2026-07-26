#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::Path,
};

use serde::Serialize;

use super::{
    process::{VmExitReason, VmMessage, VmProcessId, VmProcessState, VmProcessTable},
    resource::{
        VmResourceDescriptor, VmResourceEvent, VmResourceId, VmResourceTable,
        VmResourceTransferPolicy,
    },
    ReplValue,
};

#[path = "memory/checkpoint.rs"]
pub(crate) mod checkpoint;
#[path = "memory/collection.rs"]
mod collection;
#[path = "memory/publication.rs"]
mod publication;
#[path = "memory/shared.rs"]
mod shared;
#[path = "memory/transfer.rs"]
pub(crate) mod transfer;

pub(crate) use publication::{VmAccountedMessageSend, VmMailboxPublication};

/// Per-process logical heap limits enforced before host allocation paths run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmMemoryLimits {
    pub(crate) soft_bytes: usize,
    pub(crate) hard_bytes: usize,
}

impl VmMemoryLimits {
    pub(crate) fn new(soft_bytes: usize, hard_bytes: usize) -> Result<Self, String> {
        if soft_bytes == 0 {
            return Err("VM memory soft limit must be greater than zero".to_string());
        }
        if hard_bytes < soft_bytes {
            return Err(
                "VM memory hard limit must be greater than or equal to soft limit".to_string(),
            );
        }
        Ok(Self {
            soft_bytes,
            hard_bytes,
        })
    }
}

/// Typed result of one VM heap-accounting decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum VmMemoryPressureOutcome {
    Accounted,
    SoftLimitExceeded,
    HardLimitRejected,
}

/// Stable failure returned when a VM value cannot be measured safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmValueSizeError {
    OpaqueValue { kind: &'static str },
    Overflow,
}

impl fmt::Display for VmValueSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpaqueValue { kind } => write!(
                formatter,
                "error[vm_memory_unaccounted_value]: `{kind}` requires a dedicated ownership contract"
            ),
            Self::Overflow => formatter.write_str(
                "error[vm_memory_value_size_overflow]: logical VM value size exceeds usize",
            ),
        }
    }
}

impl Error for VmValueSizeError {}

/// Deterministic evidence for one heap charge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmMemoryPressureDecision {
    pub(crate) pid: u64,
    pub(crate) requested_bytes: usize,
    pub(crate) previous_bytes: usize,
    pub(crate) projected_bytes: usize,
    pub(crate) outcome: VmMemoryPressureOutcome,
}

/// Result of a selective mailbox scan with its deterministic work count.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmSelectiveReceiveOutcome {
    pub(crate) message: Option<VmMessage>,
    pub(crate) inspected_messages: usize,
}

/// Logical heap ownership retained for one VM native resource handle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmAccountedResourceOwnership {
    pub(crate) resource_id: u64,
    pub(crate) owner: u64,
    pub(crate) logical_bytes: usize,
}

/// Typed result of resource registration or transfer under memory pressure.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmAccountedResourceDecision {
    pub(crate) event: Option<VmResourceEvent>,
    pub(crate) pressure: VmMemoryPressureDecision,
}

/// Typed result of exiting a process through accounted resource cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAccountedProcessExit {
    pub(crate) resource_events: Vec<VmResourceEvent>,
    pub(crate) released_shared_allocations: Vec<VmSharedAllocationId>,
    pub(crate) remaining_cleanup_handles: Vec<String>,
}

/// Stable identity for one VM-owned shared allocation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmSharedAllocationId(u64);

impl VmSharedAllocationId {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// Runtime category for shared allocation accounting and inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VmSharedAllocationKind {
    Binary,
    NativeBoundaryBuffer,
    ProtocolBuffer,
    ResponseBuffer,
    TemplateOutput,
}

/// Typed outcome of registering or retaining one shared allocation reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSharedAllocationDecision {
    pub(crate) allocation_id: Option<VmSharedAllocationId>,
    pub(crate) pressure: VmMemoryPressureDecision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct VmSharedAllocation {
    id: u64,
    kind: VmSharedAllocationKind,
    logical_bytes: usize,
    owners: BTreeSet<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmSharedAllocationCounts {
    active_allocations: usize,
    unique_logical_bytes: usize,
    owner_references: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmProcessMemoryMetrics {
    pub(crate) pid: u64,
    pub(crate) current_bytes: usize,
    pub(crate) high_water_bytes: usize,
    pub(crate) collection_events: u64,
    pub(crate) released_bytes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmMemoryPressureReport<'a> {
    schema: &'static str,
    limits: VmMemoryLimitsReport,
    process_metrics: Vec<&'a VmProcessMemoryMetrics>,
    pressure_decisions: &'a [VmMemoryPressureDecision],
    resource_ownership: Vec<&'a VmAccountedResourceOwnership>,
    shared_allocation_counts: VmSharedAllocationCounts,
    shared_allocations: Vec<&'a VmSharedAllocation>,
    leak_classifications: Vec<VmMemoryLeakClassification>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmMemoryLimitsReport {
    soft_bytes: usize,
    hard_bytes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmMemoryLeakClassification {
    pid: u64,
    classification: &'static str,
    retained_bytes: usize,
}

/// VM-owned logical heap accountant.
#[derive(Debug)]
pub(crate) struct VmMemoryAccountant {
    limits: VmMemoryLimits,
    processes: BTreeMap<u64, VmProcessMemoryMetrics>,
    decisions: Vec<VmMemoryPressureDecision>,
    resource_ownership: BTreeMap<u64, VmAccountedResourceOwnership>,
    next_shared_allocation_id: u64,
    shared_allocations: BTreeMap<u64, VmSharedAllocation>,
}

impl VmMemoryAccountant {
    pub(crate) fn new(limits: VmMemoryLimits) -> Self {
        Self {
            limits,
            processes: BTreeMap::new(),
            decisions: Vec::new(),
            resource_ownership: BTreeMap::new(),
            next_shared_allocation_id: 0,
            shared_allocations: BTreeMap::new(),
        }
    }

    pub(crate) fn account_heap(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        requested_bytes: usize,
    ) -> Result<VmMemoryPressureDecision, String> {
        let soft_bytes = self.limits.soft_bytes;
        let hard_bytes = self.limits.hard_bytes;
        let (previous_bytes, projected_bytes, outcome, current_bytes) =
            with_live_process_mut(processes, pid, |process| {
                let previous_bytes = process.heap_bytes;
                let projected = previous_bytes.checked_add(requested_bytes);
                let projected_bytes = projected.unwrap_or(usize::MAX);
                let outcome = if projected.is_none() || projected_bytes > hard_bytes {
                    VmMemoryPressureOutcome::HardLimitRejected
                } else if projected_bytes > soft_bytes {
                    process.heap_bytes = projected_bytes;
                    VmMemoryPressureOutcome::SoftLimitExceeded
                } else {
                    process.heap_bytes = projected_bytes;
                    VmMemoryPressureOutcome::Accounted
                };
                (previous_bytes, projected_bytes, outcome, process.heap_bytes)
            })?;
        let decision = VmMemoryPressureDecision {
            pid: pid.as_u64(),
            requested_bytes,
            previous_bytes,
            projected_bytes,
            outcome,
        };
        let metrics = self.processes.entry(pid.as_u64()).or_default();
        metrics.pid = pid.as_u64();
        metrics.current_bytes = current_bytes;
        metrics.high_water_bytes = metrics.high_water_bytes.max(current_bytes);
        self.decisions.push(decision.clone());
        Ok(decision)
    }

    pub(crate) fn release_heap(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        released_bytes: usize,
    ) -> Result<usize, String> {
        let (actual_release, current_bytes) = with_live_process_mut(processes, pid, |process| {
            let actual_release = released_bytes.min(process.heap_bytes);
            process.heap_bytes -= actual_release;
            (actual_release, process.heap_bytes)
        })?;
        let metrics = self.processes.entry(pid.as_u64()).or_default();
        metrics.pid = pid.as_u64();
        metrics.current_bytes = current_bytes;
        if actual_release > 0 {
            metrics.collection_events = metrics.collection_events.saturating_add(1);
            metrics.released_bytes = metrics.released_bytes.saturating_add(actual_release);
        }
        Ok(actual_release)
    }

    /// Delivers one mailbox payload only after its logical bytes are reserved.
    pub(crate) fn send_message(
        &mut self,
        processes: &mut VmProcessTable,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        accounted_bytes: usize,
    ) -> Result<VmAccountedMessageSend, String> {
        processes.validate_send(sender, recipient)?;
        let pressure = self.account_heap(processes, recipient, accounted_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedMessageSend {
                publication: None,
                pressure,
            });
        }
        let message_id = processes.send_accounted(sender, recipient, payload, accounted_bytes)?;
        Ok(VmAccountedMessageSend {
            publication: Some(VmMailboxPublication::after_enqueue(
                message_id,
                recipient,
                accounted_bytes,
            )),
            pressure,
        })
    }

    /// Measures and sends a structural VM value through accounted mailbox ownership.
    pub(crate) fn send_value_message(
        &mut self,
        processes: &mut VmProcessTable,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<VmAccountedMessageSend, String> {
        let logical_bytes = logical_value_bytes(&payload).map_err(|error| error.to_string())?;
        self.send_message(processes, sender, recipient, payload, logical_bytes)
    }

    /// Measures and sends an exactly typed value through mailbox ownership.
    pub(crate) fn send_typed_value_message(
        &mut self,
        processes: &mut VmProcessTable,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        boundary_type: crate::runtime::native_image::TvmBoundaryType,
    ) -> Result<VmAccountedMessageSend, String> {
        processes.validate_send(sender, recipient)?;
        let logical_bytes = logical_value_bytes(&payload).map_err(|error| error.to_string())?;
        let pressure = self.account_heap(processes, recipient, logical_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedMessageSend {
                publication: None,
                pressure,
            });
        }
        let message_id = processes.send_typed_accounted(
            sender,
            recipient,
            payload,
            boundary_type,
            logical_bytes,
        )?;
        Ok(VmAccountedMessageSend {
            publication: Some(VmMailboxPublication::after_enqueue(
                message_id,
                recipient,
                logical_bytes,
            )),
            pressure,
        })
    }

    /// Accounts and publishes one receiver-owned managed mailbox graph.
    pub(crate) fn send_typed_managed_message(
        &mut self,
        processes: &mut VmProcessTable,
        sender: VmProcessId,
        recipient: VmProcessId,
        fragment: super::process::VmManagedMailboxToken,
        boundary_type: crate::runtime::native_image::TvmBoundaryType,
    ) -> Result<VmAccountedMessageSend, String> {
        processes.validate_send(sender, recipient)?;
        if fragment.sender() != sender.as_u64() || fragment.receiver() != recipient.as_u64() {
            return Err(
                "error[vm_memory_managed_mailbox_owner]: managed mailbox fragment route mismatch"
                    .to_string(),
            );
        }
        let logical_bytes = fragment.accounted_bytes();
        let pressure = self.account_heap(processes, recipient, logical_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedMessageSend {
                publication: None,
                pressure,
            });
        }
        let message_id = processes.send_typed_managed_accounted(
            sender,
            recipient,
            fragment,
            boundary_type,
            logical_bytes,
        )?;
        Ok(VmAccountedMessageSend {
            publication: Some(VmMailboxPublication::after_enqueue(
                message_id,
                recipient,
                logical_bytes,
            )),
            pressure,
        })
    }

    /// Measures and sends a value through the priority mailbox lane.
    pub(crate) fn send_priority_value_message(
        &mut self,
        processes: &mut VmProcessTable,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<VmAccountedMessageSend, String> {
        processes.validate_send(sender, recipient)?;
        let logical_bytes = logical_value_bytes(&payload).map_err(|error| error.to_string())?;
        let pressure = self.account_heap(processes, recipient, logical_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedMessageSend {
                publication: None,
                pressure,
            });
        }
        let message_id =
            processes.send_priority_accounted(sender, recipient, payload, logical_bytes)?;
        Ok(VmAccountedMessageSend {
            publication: Some(VmMailboxPublication::after_enqueue(
                message_id,
                recipient,
                logical_bytes,
            )),
            pressure,
        })
    }

    /// Receives one mailbox payload and releases its logical ownership charge.
    pub(crate) fn receive_message(
        &mut self,
        processes: &mut VmProcessTable,
        recipient: VmProcessId,
    ) -> Result<Option<VmMessage>, String> {
        let message =
            with_live_process_mut(processes, recipient, |process| process.receive_next())?;
        if let Some(message) = &message {
            self.release_heap(processes, recipient, message.accounted_bytes)?;
        }
        Ok(message)
    }

    /// Selectively receives one payload and releases only its ownership charge.
    pub(crate) fn selective_receive_message(
        &mut self,
        processes: &mut VmProcessTable,
        recipient: VmProcessId,
        predicate: impl FnMut(&VmMessage) -> bool,
    ) -> Result<Option<VmMessage>, String> {
        Ok(self
            .selective_receive_message_with_scan(processes, recipient, predicate)?
            .message)
    }

    /// Selectively receives one payload and reports entries inspected in order.
    pub(crate) fn selective_receive_message_with_scan(
        &mut self,
        processes: &mut VmProcessTable,
        recipient: VmProcessId,
        mut predicate: impl FnMut(&VmMessage) -> bool,
    ) -> Result<VmSelectiveReceiveOutcome, String> {
        let mut inspected_messages = 0;
        let message = with_live_process_mut(processes, recipient, |process| {
            process.selective_receive(|message| {
                inspected_messages += 1;
                predicate(message)
            })
        })?;
        if let Some(message) = &message {
            self.release_heap(processes, recipient, message.accounted_bytes)?;
        }
        Ok(VmSelectiveReceiveOutcome {
            message,
            inspected_messages,
        })
    }

    /// Registers a native resource only after reserving its owner heap charge.
    pub(crate) fn register_resource(
        &mut self,
        processes: &mut VmProcessTable,
        resources: &mut VmResourceTable,
        owner: VmProcessId,
        descriptor: VmResourceDescriptor,
        transfer_policy: VmResourceTransferPolicy,
        logical_bytes: usize,
    ) -> Result<VmAccountedResourceDecision, String> {
        let pressure = self.account_heap(processes, owner, logical_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedResourceDecision {
                event: None,
                pressure,
            });
        }
        let event = match resources.register(processes, owner, descriptor, transfer_policy) {
            Ok(event) => event,
            Err(error) => {
                self.release_heap(processes, owner, logical_bytes)?;
                return Err(error);
            }
        };
        let VmResourceEvent::Registered { id, .. } = event else {
            unreachable!("resource registration must return a registered event")
        };
        self.resource_ownership.insert(
            id.as_u64(),
            VmAccountedResourceOwnership {
                resource_id: id.as_u64(),
                owner: owner.as_u64(),
                logical_bytes,
            },
        );
        Ok(VmAccountedResourceDecision {
            event: Some(event),
            pressure,
        })
    }

    /// Transfers a resource and its logical bytes as one validated operation.
    pub(crate) fn transfer_resource(
        &mut self,
        processes: &mut VmProcessTable,
        resources: &mut VmResourceTable,
        resource: VmResourceId,
        from: VmProcessId,
        to: VmProcessId,
    ) -> Result<VmAccountedResourceDecision, String> {
        resources.validate_transfer(processes, resource, from, to)?;
        let ownership = self
            .resource_ownership
            .get(&resource.as_u64())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "resource {} has no VM memory ownership record",
                    resource.as_u64()
                )
            })?;
        if ownership.owner != from.as_u64() {
            return Err(format!(
                "resource {} memory is owned by process {}, not {}",
                resource.as_u64(),
                ownership.owner,
                from.as_u64()
            ));
        }
        let charge = if from == to {
            0
        } else {
            ownership.logical_bytes
        };
        let pressure = self.account_heap(processes, to, charge)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedResourceDecision {
                event: None,
                pressure,
            });
        }
        let event = match resources.transfer(processes, resource, from, to) {
            Ok(event) => event,
            Err(error) => {
                self.release_heap(processes, to, charge)?;
                return Err(error);
            }
        };
        if from != to {
            self.release_heap(processes, from, ownership.logical_bytes)?;
            self.resource_ownership
                .get_mut(&resource.as_u64())
                .expect("resource memory ownership was checked before transfer")
                .owner = to.as_u64();
        }
        Ok(VmAccountedResourceDecision {
            event: Some(event),
            pressure,
        })
    }

    /// Releases a native resource and its owner heap charge together.
    pub(crate) fn release_resource(
        &mut self,
        processes: &mut VmProcessTable,
        resources: &mut VmResourceTable,
        owner: VmProcessId,
        resource: VmResourceId,
    ) -> Result<VmResourceEvent, String> {
        resources.validate_release(processes, owner, resource)?;
        let ownership = self
            .resource_ownership
            .get(&resource.as_u64())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "resource {} has no VM memory ownership record",
                    resource.as_u64()
                )
            })?;
        if ownership.owner != owner.as_u64() {
            return Err(format!(
                "resource {} memory is owned by process {}, not {}",
                resource.as_u64(),
                ownership.owner,
                owner.as_u64()
            ));
        }
        let event = resources.release(processes, owner, resource)?;
        self.release_heap(processes, owner, ownership.logical_bytes)?;
        self.resource_ownership.remove(&resource.as_u64());
        Ok(event)
    }

    /// Exits a process after atomically validating and releasing owned resources.
    pub(crate) fn exit_process_with_memory_cleanup(
        &mut self,
        processes: &mut VmProcessTable,
        resources: &mut VmResourceTable,
        owner: VmProcessId,
        reason: VmExitReason,
    ) -> Result<VmAccountedProcessExit, String> {
        require_live_process(processes, owner)?;
        let resource_ids = resources
            .snapshots()
            .into_iter()
            .filter_map(|snapshot| (snapshot.owner == owner).then_some(snapshot.id))
            .collect::<Vec<_>>();
        let accounted_ids = self
            .resource_ownership
            .values()
            .filter_map(|ownership| {
                (ownership.owner == owner.as_u64()).then_some(ownership.resource_id)
            })
            .collect::<Vec<_>>();
        let resource_id_values = resource_ids
            .iter()
            .map(|resource| resource.as_u64())
            .collect::<Vec<_>>();
        if resource_id_values != accounted_ids {
            return Err(format!(
                "process {} resource ownership graph mismatch: table {:?}, memory {:?}",
                owner.as_u64(),
                resource_id_values,
                accounted_ids
            ));
        }
        let logical_bytes = resource_ids.iter().try_fold(0usize, |total, resource| {
            let ownership = self
                .resource_ownership
                .get(&resource.as_u64())
                .expect("resource ids and memory ownership were compared before cleanup");
            total.checked_add(ownership.logical_bytes).ok_or_else(|| {
                format!(
                    "process {} resource memory cleanup overflow",
                    owner.as_u64()
                )
            })
        })?;
        let shared_ids = self
            .shared_allocations
            .values()
            .filter_map(|allocation| {
                allocation
                    .owners
                    .contains(&owner.as_u64())
                    .then_some(allocation.id)
            })
            .collect::<Vec<_>>();
        let shared_logical_bytes = shared_ids.iter().try_fold(0usize, |total, allocation| {
            let logical_bytes = self
                .shared_allocations
                .get(allocation)
                .expect("shared allocation id came from the active registry")
                .logical_bytes;
            total.checked_add(logical_bytes).ok_or_else(|| {
                format!(
                    "process {} shared allocation cleanup overflow",
                    owner.as_u64()
                )
            })
        })?;
        let released_logical_bytes = logical_bytes
            .checked_add(shared_logical_bytes)
            .ok_or_else(|| format!("process {} memory cleanup overflow", owner.as_u64()))?;
        let released_shared_allocations = shared_ids
            .iter()
            .map(|allocation| VmSharedAllocationId(*allocation))
            .collect::<Vec<_>>();

        let resource_events = resources.cleanup_owner_handles(processes, owner);
        self.release_heap(processes, owner, released_logical_bytes)?;
        for resource in resource_ids {
            self.resource_ownership.remove(&resource.as_u64());
        }
        for allocation in shared_ids {
            let record = self
                .shared_allocations
                .get_mut(&allocation)
                .expect("shared allocation was collected from active registry");
            record.owners.remove(&owner.as_u64());
            if record.owners.is_empty() {
                self.shared_allocations.remove(&allocation);
            }
        }
        let remaining_cleanup_handles = processes.exit_process(owner, reason)?;
        self.synchronize_process(processes, owner)?;
        Ok(VmAccountedProcessExit {
            resource_events,
            released_shared_allocations,
            remaining_cleanup_handles,
        })
    }

    pub(crate) fn resource_ownership(
        &self,
        resource: VmResourceId,
    ) -> Option<&VmAccountedResourceOwnership> {
        self.resource_ownership.get(&resource.as_u64())
    }

    pub(crate) fn process_metrics(&self, pid: VmProcessId) -> Option<&VmProcessMemoryMetrics> {
        self.processes.get(&pid.as_u64())
    }

    /// Releases zero-live-byte accounting after postmortem capture.
    pub(crate) fn reap_process_metrics(&mut self, pid: VmProcessId) -> Result<(), String> {
        if self
            .processes
            .get(&pid.as_u64())
            .is_some_and(|metrics| metrics.current_bytes != 0)
        {
            return Err(format!(
                "cannot reap process {} with live VM memory",
                pid.as_u64()
            ));
        }
        self.processes.remove(&pid.as_u64());
        self.decisions
            .retain(|decision| decision.pid != pid.as_u64());
        Ok(())
    }

    /// Synchronizes accounting after lifecycle operations such as process exit.
    pub(crate) fn synchronize_process(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let process = processes
            .get(pid)
            .ok_or_else(|| format!("missing process {} for VM memory accounting", pid.as_u64()))?;
        let metrics = self.processes.entry(pid.as_u64()).or_default();
        metrics.pid = pid.as_u64();
        if process.heap_bytes < metrics.current_bytes {
            let released = metrics.current_bytes - process.heap_bytes;
            metrics.collection_events = metrics.collection_events.saturating_add(1);
            metrics.released_bytes = metrics.released_bytes.saturating_add(released);
        }
        metrics.current_bytes = process.heap_bytes;
        metrics.high_water_bytes = metrics.high_water_bytes.max(process.heap_bytes);
        Ok(())
    }

    pub(crate) fn write_pressure_report(&self, path: &Path) -> Result<(), String> {
        let unique_logical_bytes =
            self.shared_allocations
                .values()
                .try_fold(0usize, |total, allocation| {
                    total
                        .checked_add(allocation.logical_bytes)
                        .ok_or_else(|| "VM shared allocation report byte overflow".to_string())
                })?;
        let owner_references =
            self.shared_allocations
                .values()
                .try_fold(0usize, |total, allocation| {
                    total
                        .checked_add(allocation.owners.len())
                        .ok_or_else(|| "VM shared allocation report owner overflow".to_string())
                })?;
        let leak_classifications = self
            .processes
            .values()
            .map(|metrics| VmMemoryLeakClassification {
                pid: metrics.pid,
                classification: if metrics.current_bytes == 0 {
                    "released"
                } else {
                    "retained_live"
                },
                retained_bytes: metrics.current_bytes,
            })
            .collect();
        let report = VmMemoryPressureReport {
            schema: "terlan-vm-memory-pressure-report-v1",
            limits: VmMemoryLimitsReport {
                soft_bytes: self.limits.soft_bytes,
                hard_bytes: self.limits.hard_bytes,
            },
            process_metrics: self.processes.values().collect(),
            pressure_decisions: &self.decisions,
            resource_ownership: self.resource_ownership.values().collect(),
            shared_allocation_counts: VmSharedAllocationCounts {
                active_allocations: self.shared_allocations.len(),
                unique_logical_bytes,
                owner_references,
            },
            shared_allocations: self.shared_allocations.values().collect(),
            leak_classifications,
        };
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to serialize VM memory pressure report: {error}"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create VM memory report directory: {error}"))?;
        }
        std::fs::write(path, format!("{json}\n"))
            .map_err(|error| format!("failed to write VM memory pressure report: {error}"))
    }
}

const LOGICAL_VALUE_SLOT_BYTES: usize = 8;
const LOGICAL_SEQUENCE_HEADER_BYTES: usize = 16;
const LOGICAL_STRING_HEADER_BYTES: usize = 16;

/// Computes a deterministic retained-size estimate without recursive host calls.
pub(crate) fn logical_value_bytes(value: &ReplValue) -> Result<usize, VmValueSizeError> {
    let mut total = 0usize;
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            ReplValue::Unit => {}
            ReplValue::Bool(_) => checked_add_size(&mut total, 1)?,
            ReplValue::Int(_) => checked_add_size(&mut total, 8)?,
            ReplValue::Float(value)
            | ReplValue::String(value)
            | ReplValue::Atom(value)
            | ReplValue::Type(value) => add_logical_string(&mut total, value)?,
            ReplValue::StringBytes(value) => {
                checked_add_size(&mut total, LOGICAL_STRING_HEADER_BYTES)?;
                checked_add_size(&mut total, value.len())?;
            }
            ReplValue::Bytes(value) => {
                checked_add_size(&mut total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
                checked_add_size(&mut total, value.len())?;
            }
            ReplValue::BitString(value) => {
                checked_add_size(&mut total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
                checked_add_size(&mut total, 8)?;
                checked_add_size(&mut total, value.byte_len())?;
            }
            ReplValue::Tuple(items) | ReplValue::List(items) | ReplValue::Set(items) => {
                add_sequence_storage(&mut total, items.len())?;
                pending.extend(items);
            }
            ReplValue::Record { name, fields } => {
                add_logical_string(&mut total, name)?;
                add_sequence_storage(&mut total, fields.len())?;
                for (field, value) in fields {
                    add_logical_string(&mut total, field)?;
                    pending.push(value);
                }
            }
            ReplValue::Map(entries) => {
                add_sequence_storage(
                    &mut total,
                    entries
                        .len()
                        .checked_mul(2)
                        .ok_or(VmValueSizeError::Overflow)?,
                )?;
                for (key, value) in entries {
                    pending.push(key);
                    pending.push(value);
                }
            }
            #[cfg(test)]
            ReplValue::MapIndexed(map) => {
                checked_add_size(&mut total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
                let mut retained_error = None;
                map.visit_retained_entries(|key, value| {
                    if retained_error.is_some() {
                        return;
                    }
                    if let Err(error) = checked_add_size(&mut total, LOGICAL_VALUE_SLOT_BYTES * 2) {
                        retained_error = Some(error);
                        return;
                    }
                    pending.push(key);
                    if let Some(value) = value {
                        pending.push(value);
                    }
                });
                if let Some(error) = retained_error {
                    return Err(error);
                }
            }
            #[cfg(test)]
            ReplValue::Iterator { items, .. } => {
                add_sequence_storage(&mut total, items.len())?;
                checked_add_size(&mut total, 8)?;
                pending.extend(items);
            }
            #[cfg(test)]
            ReplValue::RandomGenerator(_) => return Err(opaque_value("RandomGenerator")),
        }
    }
    Ok(total)
}

fn opaque_value(kind: &'static str) -> VmValueSizeError {
    VmValueSizeError::OpaqueValue { kind }
}

fn add_sequence_storage(total: &mut usize, slots: usize) -> Result<(), VmValueSizeError> {
    checked_add_size(total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
    let bytes = slots
        .checked_mul(LOGICAL_VALUE_SLOT_BYTES)
        .ok_or(VmValueSizeError::Overflow)?;
    checked_add_size(total, bytes)
}

fn add_logical_string(total: &mut usize, value: &str) -> Result<(), VmValueSizeError> {
    checked_add_size(total, LOGICAL_STRING_HEADER_BYTES)?;
    checked_add_size(total, value.len())
}

fn checked_add_size(total: &mut usize, bytes: usize) -> Result<(), VmValueSizeError> {
    *total = total.checked_add(bytes).ok_or(VmValueSizeError::Overflow)?;
    Ok(())
}

/// Runs one memory mutation under the process table's scoped actor ownership.
fn with_live_process_mut<R>(
    processes: &mut VmProcessTable,
    pid: VmProcessId,
    mutate: impl FnOnce(&mut super::process::VmProcess) -> R,
) -> Result<R, String> {
    require_live_process(processes, pid)?;
    processes.with_process_control_mutator(pid, mutate)
}

/// Validates a live process before memory ownership is read or changed.
fn require_live_process(processes: &VmProcessTable, pid: VmProcessId) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("missing process {} for VM memory accounting", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!(
            "exited process {} cannot own VM heap bytes",
            pid.as_u64()
        ));
    }
    Ok(())
}

fn stale_shared_allocation(allocation: VmSharedAllocationId) -> String {
    format!("stale VM shared allocation {}", allocation.as_u64())
}

#[cfg(test)]
#[path = "memory_collection_test.rs"]
mod memory_collection_test;

#[cfg(test)]
#[path = "memory_conformance_test.rs"]
mod memory_conformance_test;

#[cfg(test)]
#[path = "memory_alloc_beam_suite_parity_test.rs"]
mod memory_alloc_beam_suite_parity_test;

#[cfg(test)]
#[path = "memory_test.rs"]
mod memory_test;

#[cfg(test)]
#[path = "memory_transfer_test.rs"]
mod memory_transfer_test;

#[cfg(test)]
#[path = "term_model_parity_test.rs"]
mod term_model_parity_test;
