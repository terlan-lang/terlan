//! Linear transfer of one parked actor's local runtime state.

use std::collections::BTreeSet;
use std::fmt;

use super::{VmActorRuntime, VmDelayedActorMessage, VmProcessId};
use crate::runtime::vm::memory::VmMemoryTransfer;
use crate::runtime::vm::process::{VmProcessState, VmProcessTransfer};
use crate::runtime::vm::process_alias::VmProcessAliasTransfer;
use crate::runtime::vm::resource::VmResourceTransfer;
use crate::runtime::vm::scheduler::VmSchedulerPlacementTransfer;
use crate::runtime::vm::timer::{VmTimerId, VmTimerTransfer};

/// Process and scheduler state detached for one parked native actor.
#[derive(Debug)]
pub(crate) struct VmActorRuntimeTransfer {
    owner: VmProcessId,
    process: VmProcessTransfer,
    placement: VmSchedulerPlacementTransfer,
    aliases: VmProcessAliasTransfer,
    resources: VmResourceTransfer,
    memory: VmMemoryTransfer,
    timers: VmTimerTransfer,
    delayed_messages: Vec<(VmTimerId, VmDelayedActorMessage)>,
    native_continuation: (u64, u64),
    explicitly_suspended: bool,
}

impl VmActorRuntimeTransfer {
    /// Returns the exact actor owner carried by the envelope.
    pub(crate) const fn owner(&self) -> VmProcessId {
        self.owner
    }

    /// Returns the exact native request and continuation identities.
    pub(crate) const fn native_continuation(&self) -> (u64, u64) {
        self.native_continuation
    }
}

/// Failed actor-runtime import retaining the complete rollback envelope.
#[derive(Debug)]
pub(crate) struct VmActorRuntimeImportFailure {
    reason: String,
    transfer: VmActorRuntimeTransfer,
}

impl VmActorRuntimeImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the entire actor-runtime state for source restoration.
    pub(crate) fn into_transfer(self) -> VmActorRuntimeTransfer {
        self.transfer
    }
}

impl fmt::Display for VmActorRuntimeImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmActorRuntimeImportFailure {}

impl VmActorRuntime {
    /// Detaches one parked native actor after validating owner-local state.
    pub(crate) fn detach_actor_runtime(
        &mut self,
        owner: VmProcessId,
    ) -> Result<VmActorRuntimeTransfer, String> {
        let process = self
            .processes
            .get(owner)
            .ok_or_else(|| format!("actor transfer process {} is missing", owner.as_u64()))?;
        if !matches!(process.state, VmProcessState::Suspended(_)) {
            return Err(format!(
                "actor transfer process {} is not parked at a native safepoint",
                owner.as_u64()
            ));
        }
        let native_continuation = self
            .native_continuations_by_owner
            .get(&owner)
            .copied()
            .ok_or_else(|| {
                format!(
                    "actor transfer process {} has no native continuation",
                    owner.as_u64()
                )
            })?;
        if self.native_continuations.get(&native_continuation) != Some(&owner) {
            return Err(format!(
                "actor transfer process {} has inconsistent continuation ownership",
                owner.as_u64()
            ));
        }
        self.validate_detachable_owner_state(owner)?;

        let placement = self.scheduler.detach_process_placement(owner);
        let process = match self.processes.detach_process_for_transfer(owner) {
            Ok(process) => process,
            Err(error) => {
                self.scheduler
                    .import_process_placement(&self.processes, placement)
                    .expect("source process remains available for placement rollback");
                return Err(error);
            }
        };
        self.native_continuations.remove(&native_continuation);
        self.native_continuations_by_owner.remove(&owner);
        let explicitly_suspended = self.explicit_native_suspensions.remove(&owner);
        let aliases = self.aliases.detach_owner_aliases(owner);
        let resources = self.resources.detach_owner_resources(owner);
        let memory = self.memory.detach_owner_memory(owner);
        let timers = self.timers.detach_owner_timer_state(owner);
        let delayed_messages = timers
            .timer_ids()
            .filter_map(|timer| {
                self.delayed_messages
                    .remove(&timer)
                    .map(|message| (timer, message))
            })
            .collect();
        Ok(VmActorRuntimeTransfer {
            owner,
            process,
            placement,
            aliases,
            resources,
            memory,
            timers,
            delayed_messages,
            native_continuation,
            explicitly_suspended,
        })
    }

    /// Imports one parked actor or returns every component for rollback.
    pub(crate) fn import_actor_runtime(
        &mut self,
        transfer: VmActorRuntimeTransfer,
    ) -> Result<(), VmActorRuntimeImportFailure> {
        if let Err(reason) = self.validate_actor_runtime_import(&transfer) {
            return Err(VmActorRuntimeImportFailure { reason, transfer });
        }
        let VmActorRuntimeTransfer {
            owner,
            process,
            placement,
            aliases,
            resources,
            memory,
            timers,
            delayed_messages,
            native_continuation,
            explicitly_suspended,
        } = transfer;
        let process_heap_bytes = process.heap_bytes();
        if let Err(failure) = self.processes.import_process_transfer(process) {
            return Err(VmActorRuntimeImportFailure {
                reason: failure.reason().to_string(),
                transfer: VmActorRuntimeTransfer {
                    owner,
                    process: failure.into_transfer(),
                    placement,
                    aliases,
                    resources,
                    memory,
                    timers,
                    delayed_messages,
                    native_continuation,
                    explicitly_suspended,
                },
            });
        }
        if let Err(failure) = self
            .scheduler
            .import_process_placement(&self.processes, placement)
        {
            let process = self
                .processes
                .detach_process_for_transfer(owner)
                .expect("just-imported process can be detached for rollback");
            return Err(VmActorRuntimeImportFailure {
                reason: failure.reason().to_string(),
                transfer: VmActorRuntimeTransfer {
                    owner,
                    process,
                    placement: failure.into_transfer(),
                    aliases,
                    resources,
                    memory,
                    timers,
                    delayed_messages,
                    native_continuation,
                    explicitly_suspended,
                },
            });
        }
        if let Err(failure) = self.aliases.import_alias_transfer(aliases) {
            let (process, placement) = self.detach_imported_base(owner);
            return Err(VmActorRuntimeImportFailure {
                reason: failure.reason().to_string(),
                transfer: VmActorRuntimeTransfer {
                    owner,
                    process,
                    placement,
                    aliases: failure.into_transfer(),
                    resources,
                    memory,
                    timers,
                    delayed_messages,
                    native_continuation,
                    explicitly_suspended,
                },
            });
        }
        if let Err(failure) = self.resources.import_resource_transfer(resources) {
            let aliases = self.aliases.detach_owner_aliases(owner);
            let (process, placement) = self.detach_imported_base(owner);
            return Err(VmActorRuntimeImportFailure {
                reason: failure.reason().to_string(),
                transfer: VmActorRuntimeTransfer {
                    owner,
                    process,
                    placement,
                    aliases,
                    resources: failure.into_transfer(),
                    memory,
                    timers,
                    delayed_messages,
                    native_continuation,
                    explicitly_suspended,
                },
            });
        }
        if let Err(failure) = self
            .memory
            .import_memory_transfer(memory, process_heap_bytes)
        {
            let resources = self.resources.detach_owner_resources(owner);
            let aliases = self.aliases.detach_owner_aliases(owner);
            let (process, placement) = self.detach_imported_base(owner);
            return Err(VmActorRuntimeImportFailure {
                reason: failure.reason().to_string(),
                transfer: VmActorRuntimeTransfer {
                    owner,
                    process,
                    placement,
                    aliases,
                    resources,
                    memory: failure.into_transfer(),
                    timers,
                    delayed_messages,
                    native_continuation,
                    explicitly_suspended,
                },
            });
        }
        if let Err(failure) = self.timers.import_timer_transfer(timers) {
            let memory = self.memory.detach_owner_memory(owner);
            let resources = self.resources.detach_owner_resources(owner);
            let aliases = self.aliases.detach_owner_aliases(owner);
            let (process, placement) = self.detach_imported_base(owner);
            return Err(VmActorRuntimeImportFailure {
                reason: failure.reason().to_string(),
                transfer: VmActorRuntimeTransfer {
                    owner,
                    process,
                    placement,
                    aliases,
                    resources,
                    memory,
                    timers: failure.into_transfer(),
                    delayed_messages,
                    native_continuation,
                    explicitly_suspended,
                },
            });
        }
        for (timer, message) in delayed_messages {
            self.delayed_messages.insert(timer, message);
        }
        self.native_continuations.insert(native_continuation, owner);
        self.native_continuations_by_owner
            .insert(owner, native_continuation);
        if explicitly_suspended {
            self.explicit_native_suspensions.insert(owner);
        }
        Ok(())
    }

    /// Validates destination indexes before consuming any transfer component.
    pub(crate) fn validate_actor_runtime_import(
        &self,
        transfer: &VmActorRuntimeTransfer,
    ) -> Result<(), String> {
        if transfer.process.process_id() != transfer.owner
            || transfer.placement.process_id() != transfer.owner
            || transfer.aliases.owner() != transfer.owner
            || transfer.resources.owner() != transfer.owner
            || transfer.memory.owner() != transfer.owner
            || transfer.timers.owner() != transfer.owner
        {
            return Err("actor transfer component owner mismatch".to_string());
        }
        self.processes.validate_process_import(&transfer.process)?;
        self.aliases.validate_alias_import(&transfer.aliases)?;
        self.resources
            .validate_resource_import(&transfer.resources)?;
        self.memory
            .validate_memory_import(&transfer.memory, transfer.process.heap_bytes())?;
        let resource_ids = transfer.resources.resource_ids().collect::<BTreeSet<_>>();
        let accounted_resource_ids = transfer.memory.resource_ids().collect::<BTreeSet<_>>();
        if resource_ids != accounted_resource_ids {
            return Err("actor transfer resource memory graph mismatch".to_string());
        }
        self.timers.validate_timer_import(&transfer.timers)?;
        let timer_ids = transfer.timers.timer_ids().collect::<BTreeSet<_>>();
        let mut delayed_ids = BTreeSet::new();
        for (timer, _) in &transfer.delayed_messages {
            if !timer_ids.contains(timer) {
                return Err("actor transfer delayed message has no transferred timer".to_string());
            }
            if !delayed_ids.insert(*timer) {
                return Err("actor transfer contains a duplicate delayed message".to_string());
            }
            if self.delayed_messages.contains_key(timer) {
                return Err(format!(
                    "actor transfer destination already contains delayed timer {}",
                    timer.as_u64()
                ));
            }
        }
        if self
            .native_continuations
            .contains_key(&transfer.native_continuation)
            || self
                .native_continuations_by_owner
                .contains_key(&transfer.owner)
        {
            return Err(format!(
                "actor transfer process {} has a destination continuation collision",
                transfer.owner.as_u64()
            ));
        }
        Ok(())
    }

    /// Validates owner graphs before any actor component is detached.
    fn validate_detachable_owner_state(&self, owner: VmProcessId) -> Result<(), String> {
        let process_heap_bytes = self
            .processes
            .get(owner)
            .expect("actor process was validated before owner-state validation")
            .heap_bytes;
        let resource_ids = self
            .resources
            .snapshots()
            .into_iter()
            .filter_map(|record| (record.owner == owner).then_some(record.id.as_u64()));
        self.memory
            .validate_memory_detach(owner, process_heap_bytes, resource_ids)?;
        let relationships = self
            .failures
            .snapshot(&self.processes, owner)
            .map_err(|error| format!("actor transfer relationship inspection failed: {error:?}"))?;
        if relationships.trap_exits
            || !relationships.links.is_empty()
            || !relationships.monitoring.is_empty()
            || !relationships.monitored_by.is_empty()
        {
            return Err("actor transfer relationships are not yet transferable".to_string());
        }
        Ok(())
    }

    /// Re-detaches an imported base after a later owner table rejects admission.
    fn detach_imported_base(
        &mut self,
        owner: VmProcessId,
    ) -> (VmProcessTransfer, VmSchedulerPlacementTransfer) {
        let placement = self.scheduler.detach_process_placement(owner);
        let process = self
            .processes
            .detach_process_for_transfer(owner)
            .expect("just-imported process can be detached for rollback");
        (process, placement)
    }
}
