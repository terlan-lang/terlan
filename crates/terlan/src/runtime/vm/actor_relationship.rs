use super::super::failure::VmMonitorRef;
#[cfg(test)]
use super::super::failure::{is_monitor_down_message, VmTrapExitUpdate};
use super::super::process::VmProcessId;
use super::{VmActorRuntime, ACTOR_OPERATION_REDUCTIONS};

/// Options controlling removal of one actor monitor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmActorDemonitorOptions {
    flush_down: bool,
}

#[cfg(test)]
impl VmActorDemonitorOptions {
    /// Removes an already-delivered completion from the observer mailbox.
    pub(crate) fn flush_down(mut self) -> Self {
        self.flush_down = true;
        self
    }
}

/// Observable effects of removing one actor monitor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmActorDemonitorResult {
    pub(crate) removed: bool,
    pub(crate) flushed_down: bool,
}

impl VmActorRuntime {
    /// Links two live actors and reports whether a relationship was created.
    pub(crate) fn link_actors(
        &mut self,
        left: VmProcessId,
        right: VmProcessId,
    ) -> Result<bool, String> {
        self.link_actors_with_priority(left, right, false)
    }

    /// Links two actors and controls whether trapped exits received by `left`
    /// use the priority mailbox lane. Re-linking from either endpoint updates
    /// only that endpoint's directional priority state.
    pub(crate) fn link_actors_with_priority(
        &mut self,
        left: VmProcessId,
        right: VmProcessId,
        priority: bool,
    ) -> Result<bool, String> {
        let created = !self.failures.is_linked(left, right);
        self.failures
            .link_with_priority(&self.processes, left, right, priority)?;
        self.charge_actor_reductions(left, ACTOR_OPERATION_REDUCTIONS);
        Ok(created)
    }

    /// Removes a relationship between two live actors when present.
    #[cfg(test)]
    pub(crate) fn unlink_actors(
        &mut self,
        left: VmProcessId,
        right: VmProcessId,
    ) -> Result<bool, String> {
        if left == right {
            return Err(format!(
                "cannot unlink process {} from itself",
                left.as_u64()
            ));
        }
        self.ensure_live_process(left, "unlink")?;
        self.ensure_live_process(right, "unlink")?;
        let removed = self.failures.is_linked(left, right);
        self.failures.unlink(left, right);
        self.charge_actor_reductions(left, ACTOR_OPERATION_REDUCTIONS);
        Ok(removed)
    }

    /// Creates a stable monitor from one live actor to another.
    pub(crate) fn monitor_actor(
        &mut self,
        observer: VmProcessId,
        target: VmProcessId,
    ) -> Result<VmMonitorRef, String> {
        self.monitor_actor_with_priority(observer, target, false)
    }

    /// Creates a monitor whose completion uses the priority mailbox lane.
    pub(crate) fn monitor_actor_with_priority(
        &mut self,
        observer: VmProcessId,
        target: VmProcessId,
        priority: bool,
    ) -> Result<VmMonitorRef, String> {
        let registration = self.failures.monitor_or_complete_with_priority(
            &mut self.references,
            &mut self.processes,
            observer,
            target,
            priority,
        )?;
        if registration.completed {
            self.scheduler
                .wake_process(&mut self.processes, observer)
                .expect("validated monitor observer must wake for immediate completion");
        }
        self.charge_actor_reductions(observer, ACTOR_OPERATION_REDUCTIONS);
        Ok(registration.monitor_ref)
    }

    /// Reports whether an actor owns an active priority link or monitor.
    #[cfg(test)]
    pub(crate) fn actor_has_priority_messages(&self, pid: VmProcessId) -> Result<bool, String> {
        self.ensure_live_process(pid, "inspect priority messages for")?;
        Ok(self.failures.has_priority_relationship(pid) || self.aliases.has_priority_alias(pid))
    }

    /// Removes an observer-owned monitor and optionally flushes its completion.
    #[cfg(test)]
    pub(crate) fn demonitor_actor(
        &mut self,
        observer: VmProcessId,
        monitor_ref: VmMonitorRef,
        options: VmActorDemonitorOptions,
    ) -> Result<VmActorDemonitorResult, String> {
        self.ensure_live_process(observer, "demonitor from")?;
        let removed = self.failures.demonitor_for(observer, monitor_ref.clone())?;
        let flushed = if options.flush_down {
            self.memory
                .selective_receive_message(&mut self.processes, observer, |message| {
                    is_monitor_down_message(&message.payload, &monitor_ref)
                })?
        } else {
            None
        };
        if let Some(message) = &flushed {
            self.scheduler
                .charge_memory_reductions(&mut self.processes, observer, message.accounted_bytes)
                .expect("demonitor observer remains live while charging flush reductions");
        }
        self.charge_actor_reductions(observer, ACTOR_OPERATION_REDUCTIONS);
        Ok(VmActorDemonitorResult {
            removed,
            flushed_down: flushed.is_some(),
        })
    }

    /// Changes whether a live actor converts linked exits into messages.
    #[cfg(test)]
    pub(crate) fn set_actor_trap_exits(
        &mut self,
        pid: VmProcessId,
        enabled: bool,
    ) -> Result<VmTrapExitUpdate, String> {
        let update = self
            .failures
            .set_trap_exits(&self.processes, pid, enabled)
            .map_err(|error| error.to_string())?;
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(update)
    }
}
