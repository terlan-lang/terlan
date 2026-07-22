#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};

/// Stable identifier for a live use of a dynamically registered VM module.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmDynamicModuleLeaseId(u64);

impl VmDynamicModuleLeaseId {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// Validated module metadata supplied by a NativeBoundary implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDynamicModuleDescriptor {
    pub(crate) name: String,
    pub(crate) declared_name: String,
    pub(crate) artifact_id: String,
    pub(crate) permanent: bool,
    pub(crate) init_succeeds: bool,
}

impl VmDynamicModuleDescriptor {
    pub(crate) fn new(name: impl Into<String>, artifact_id: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            declared_name: name.clone(),
            name,
            artifact_id: artifact_id.into(),
            permanent: false,
            init_succeeds: true,
        }
    }

    pub(crate) fn with_declared_name(mut self, declared_name: impl Into<String>) -> Self {
        self.declared_name = declared_name.into();
        self
    }

    pub(crate) fn with_permanent(mut self, permanent: bool) -> Self {
        self.permanent = permanent;
        self
    }

    pub(crate) fn with_init_success(mut self, init_succeeds: bool) -> Self {
        self.init_succeeds = init_succeeds;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDynamicModulePendingAction {
    Unload,
    Reload(VmDynamicModuleDescriptor),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDynamicModuleSnapshot {
    pub(crate) name: String,
    pub(crate) artifact_id: String,
    pub(crate) owner_references: Vec<(VmProcessId, usize)>,
    pub(crate) leases: Vec<(VmDynamicModuleLeaseId, VmProcessId)>,
    pub(crate) pending: Option<VmDynamicModulePendingAction>,
    pub(crate) permanent: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDynamicModuleLeaseCloseReason {
    Explicit,
    OwnerExited,
    ForcedUnload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDynamicModuleEvent {
    Loaded {
        name: String,
        artifact_id: String,
    },
    LoadReused {
        name: String,
        owner: VmProcessId,
    },
    UnloadPending {
        name: String,
    },
    UnloadCancelled {
        name: String,
    },
    Unloaded {
        name: String,
        artifact_id: String,
    },
    ReloadPending {
        name: String,
        replacement_artifact_id: String,
    },
    Reloaded {
        name: String,
        previous_artifact_id: String,
        artifact_id: String,
    },
    LeaseOpened {
        name: String,
        lease: VmDynamicModuleLeaseId,
        owner: VmProcessId,
    },
    LeaseClosed {
        name: String,
        lease: VmDynamicModuleLeaseId,
        reason: VmDynamicModuleLeaseCloseReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDynamicModuleLoadOutcome {
    Loaded,
    Reused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDynamicModuleUnloadOutcome {
    ReferenceReleased,
    Pending,
    Unloaded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDynamicModuleReloadOutcome {
    Pending,
    Reloaded,
    Unchanged,
}

#[derive(Debug)]
struct VmDynamicModuleRecord {
    descriptor: VmDynamicModuleDescriptor,
    owner_references: BTreeMap<VmProcessId, usize>,
    leases: BTreeMap<VmDynamicModuleLeaseId, VmProcessId>,
    pending: Option<VmDynamicModulePendingAction>,
}

/// VM-owned lifecycle registry for dynamically supplied native modules.
///
/// The registry deliberately models portable runtime behavior rather than a
/// host-specific `dlopen` or ERTS driver API: module generations are validated
/// before mutation, process references keep modules live, leases delay unload
/// and reload, and every transition has deterministic inspection state.
#[derive(Debug, Default)]
pub(crate) struct VmDynamicModuleRegistry {
    next_lease_id: u64,
    modules: BTreeMap<String, VmDynamicModuleRecord>,
    events: Vec<VmDynamicModuleEvent>,
}

impl VmDynamicModuleRegistry {
    pub(crate) fn load(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        descriptor: VmDynamicModuleDescriptor,
    ) -> Result<VmDynamicModuleLoadOutcome, String> {
        ensure_live_process(processes, owner, "module owner")?;
        validate_descriptor(&descriptor)?;

        if let Some(record) = self.modules.get_mut(&descriptor.name) {
            if record.descriptor.artifact_id != descriptor.artifact_id {
                return Err(format!(
                    "module {} already has artifact {}; request an explicit reload",
                    descriptor.name, record.descriptor.artifact_id
                ));
            }
            *record.owner_references.entry(owner).or_default() += 1;
            if matches!(record.pending, Some(VmDynamicModulePendingAction::Unload)) {
                record.pending = None;
                self.events.push(VmDynamicModuleEvent::UnloadCancelled {
                    name: descriptor.name.clone(),
                });
            }
            self.events.push(VmDynamicModuleEvent::LoadReused {
                name: descriptor.name,
                owner,
            });
            return Ok(VmDynamicModuleLoadOutcome::Reused);
        }

        let name = descriptor.name.clone();
        let artifact_id = descriptor.artifact_id.clone();
        self.modules.insert(
            name.clone(),
            VmDynamicModuleRecord {
                descriptor,
                owner_references: BTreeMap::from([(owner, 1)]),
                leases: BTreeMap::new(),
                pending: None,
            },
        );
        self.events
            .push(VmDynamicModuleEvent::Loaded { name, artifact_id });
        Ok(VmDynamicModuleLoadOutcome::Loaded)
    }

    pub(crate) fn open_lease(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        module_name: &str,
    ) -> Result<VmDynamicModuleLeaseId, String> {
        ensure_live_process(processes, owner, "lease owner")?;
        let record = self
            .modules
            .get_mut(module_name)
            .ok_or_else(|| format!("module {module_name} is not loaded"))?;
        if matches!(record.pending, Some(VmDynamicModulePendingAction::Unload)) {
            return Err(format!("module {module_name} is pending unload"));
        }

        self.next_lease_id = self.next_lease_id.saturating_add(1);
        let lease = VmDynamicModuleLeaseId(self.next_lease_id);
        record.leases.insert(lease, owner);
        self.events.push(VmDynamicModuleEvent::LeaseOpened {
            name: module_name.to_string(),
            lease,
            owner,
        });
        Ok(lease)
    }

    pub(crate) fn close_lease(&mut self, lease: VmDynamicModuleLeaseId) -> Result<(), String> {
        let module_name = self
            .modules
            .iter()
            .find_map(|(name, record)| record.leases.contains_key(&lease).then(|| name.clone()))
            .ok_or_else(|| format!("stale dynamic module lease {}", lease.as_u64()))?;
        self.modules
            .get_mut(&module_name)
            .expect("lease module was resolved before mutation")
            .leases
            .remove(&lease);
        self.events.push(VmDynamicModuleEvent::LeaseClosed {
            name: module_name.clone(),
            lease,
            reason: VmDynamicModuleLeaseCloseReason::Explicit,
        });
        self.complete_if_drained(&module_name);
        Ok(())
    }

    pub(crate) fn request_unload(
        &mut self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        module_name: &str,
        force: bool,
    ) -> Result<VmDynamicModuleUnloadOutcome, String> {
        ensure_live_process(processes, requester, "module owner")?;
        let record = self
            .modules
            .get(module_name)
            .ok_or_else(|| format!("module {module_name} is not loaded"))?;
        if record.descriptor.permanent {
            return Err(format!("module {module_name} is permanent"));
        }
        if !record.owner_references.contains_key(&requester) {
            return Err(format!(
                "process {} does not own module {module_name}",
                requester.as_u64()
            ));
        }

        if force {
            self.force_unload(module_name);
            return Ok(VmDynamicModuleUnloadOutcome::Unloaded);
        }

        let record = self
            .modules
            .get_mut(module_name)
            .expect("module ownership was validated before mutation");
        decrement_reference(&mut record.owner_references, requester);
        record.pending = record.pending.take().and_then(|pending| {
            (!matches!(pending, VmDynamicModulePendingAction::Reload(_))).then_some(pending)
        });
        if !record.owner_references.is_empty() {
            return Ok(VmDynamicModuleUnloadOutcome::ReferenceReleased);
        }
        if !record.leases.is_empty() {
            record.pending = Some(VmDynamicModulePendingAction::Unload);
            self.events.push(VmDynamicModuleEvent::UnloadPending {
                name: module_name.to_string(),
            });
            return Ok(VmDynamicModuleUnloadOutcome::Pending);
        }

        self.remove_module(module_name);
        Ok(VmDynamicModuleUnloadOutcome::Unloaded)
    }

    pub(crate) fn request_reload(
        &mut self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        replacement: VmDynamicModuleDescriptor,
    ) -> Result<VmDynamicModuleReloadOutcome, String> {
        ensure_live_process(processes, requester, "module owner")?;
        validate_descriptor(&replacement)?;
        let record = self
            .modules
            .get(&replacement.name)
            .ok_or_else(|| format!("module {} is not loaded", replacement.name))?;
        if record.descriptor.permanent {
            return Err(format!("module {} is permanent", replacement.name));
        }
        if !record.owner_references.contains_key(&requester) {
            return Err(format!(
                "process {} does not own module {}",
                requester.as_u64(),
                replacement.name
            ));
        }
        if record.descriptor.artifact_id == replacement.artifact_id {
            return Ok(VmDynamicModuleReloadOutcome::Unchanged);
        }

        let name = replacement.name.clone();
        if record.leases.is_empty() {
            self.apply_reload(&name, replacement);
            return Ok(VmDynamicModuleReloadOutcome::Reloaded);
        }
        let replacement_artifact_id = replacement.artifact_id.clone();
        self.modules
            .get_mut(&name)
            .expect("reload module was validated before mutation")
            .pending = Some(VmDynamicModulePendingAction::Reload(replacement));
        self.events.push(VmDynamicModuleEvent::ReloadPending {
            name,
            replacement_artifact_id,
        });
        Ok(VmDynamicModuleReloadOutcome::Pending)
    }

    pub(crate) fn cleanup_owner(&mut self, owner: VmProcessId) {
        let names = self.modules.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let leases = self.modules[&name]
                .leases
                .iter()
                .filter_map(|(lease, lease_owner)| (*lease_owner == owner).then_some(*lease))
                .collect::<Vec<_>>();
            let record = self
                .modules
                .get_mut(&name)
                .expect("module name came from live registry");
            record.owner_references.remove(&owner);
            for lease in leases {
                record.leases.remove(&lease);
                self.events.push(VmDynamicModuleEvent::LeaseClosed {
                    name: name.clone(),
                    lease,
                    reason: VmDynamicModuleLeaseCloseReason::OwnerExited,
                });
            }

            let record = &self.modules[&name];
            if record.owner_references.is_empty()
                && record.leases.is_empty()
                && !record.descriptor.permanent
            {
                self.remove_module(&name);
            } else {
                self.complete_if_drained(&name);
            }
        }
    }

    pub(crate) fn snapshots(&self) -> Vec<VmDynamicModuleSnapshot> {
        self.modules
            .values()
            .map(|record| VmDynamicModuleSnapshot {
                name: record.descriptor.name.clone(),
                artifact_id: record.descriptor.artifact_id.clone(),
                owner_references: record
                    .owner_references
                    .iter()
                    .map(|(owner, count)| (*owner, *count))
                    .collect(),
                leases: record
                    .leases
                    .iter()
                    .map(|(lease, owner)| (*lease, *owner))
                    .collect(),
                pending: record.pending.clone(),
                permanent: record.descriptor.permanent,
            })
            .collect()
    }

    pub(crate) fn events(&self) -> &[VmDynamicModuleEvent] {
        &self.events
    }

    fn complete_if_drained(&mut self, module_name: &str) {
        let action = self.modules.get(module_name).and_then(|record| {
            record
                .leases
                .is_empty()
                .then(|| record.pending.clone())
                .flatten()
        });
        match action {
            Some(VmDynamicModulePendingAction::Unload) => {
                if self.modules[module_name].owner_references.is_empty() {
                    self.remove_module(module_name);
                }
            }
            Some(VmDynamicModulePendingAction::Reload(replacement)) => {
                self.apply_reload(module_name, replacement);
            }
            None => {}
        }
    }

    fn apply_reload(&mut self, module_name: &str, replacement: VmDynamicModuleDescriptor) {
        let record = self
            .modules
            .get_mut(module_name)
            .expect("reload applies only to a live module");
        let previous_artifact_id = record.descriptor.artifact_id.clone();
        let artifact_id = replacement.artifact_id.clone();
        record.descriptor = replacement;
        record.pending = None;
        self.events.push(VmDynamicModuleEvent::Reloaded {
            name: module_name.to_string(),
            previous_artifact_id,
            artifact_id,
        });
    }

    fn force_unload(&mut self, module_name: &str) {
        let leases = self.modules[module_name]
            .leases
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for lease in leases {
            self.events.push(VmDynamicModuleEvent::LeaseClosed {
                name: module_name.to_string(),
                lease,
                reason: VmDynamicModuleLeaseCloseReason::ForcedUnload,
            });
        }
        self.remove_module(module_name);
    }

    fn remove_module(&mut self, module_name: &str) {
        if let Some(record) = self.modules.remove(module_name) {
            self.events.push(VmDynamicModuleEvent::Unloaded {
                name: module_name.to_string(),
                artifact_id: record.descriptor.artifact_id,
            });
        }
    }
}

fn decrement_reference(references: &mut BTreeMap<VmProcessId, usize>, owner: VmProcessId) {
    let count = references
        .get_mut(&owner)
        .expect("module ownership was validated before decrement");
    *count -= 1;
    if *count == 0 {
        references.remove(&owner);
    }
}

fn validate_descriptor(descriptor: &VmDynamicModuleDescriptor) -> Result<(), String> {
    if descriptor.name.trim().is_empty() {
        return Err("dynamic module name cannot be empty".to_string());
    }
    if descriptor.artifact_id.trim().is_empty() {
        return Err(format!(
            "module {} has no artifact identity",
            descriptor.name
        ));
    }
    if descriptor.declared_name != descriptor.name {
        return Err(format!(
            "module {} declares mismatched name {}",
            descriptor.name, descriptor.declared_name
        ));
    }
    if !descriptor.init_succeeds {
        return Err(format!("module {} initialization failed", descriptor.name));
    }
    Ok(())
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

#[cfg(test)]
#[path = "dynamic_module_ddll_beam_suite_parity_test.rs"]
mod dynamic_module_ddll_beam_suite_parity_test;
