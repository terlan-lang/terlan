#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmExitReason, VmMessage, VmProcessId, VmProcessSource, VmProcessTable};
use super::scheduler::{VmScheduler, VmSchedulerDecision, VmSchedulerRun, VmSchedulerSlice};
use super::ReplValue;

/// Result of an actor receive operation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmActorReceive {
    Message(VmMessage),
    Blocked,
    Timeout,
}

/// Local VM actor runtime facade.
///
/// Inputs:
/// - Process spawn requests, name registrations, sends, receives, and
///   scheduler polls.
///
/// Output:
/// - Actor/process effects expressed through Terlan-owned VM primitives.
///
/// Transformation:
/// - Composes the process table and cooperative scheduler into the first
///   higher-level actor surface without depending on OTP process machinery.
#[derive(Debug, Default)]
pub(crate) struct VmActorRuntime {
    processes: VmProcessTable,
    scheduler: VmScheduler,
    names: BTreeMap<String, VmProcessId>,
}

impl VmActorRuntime {
    /// Spawns and schedules a root actor.
    pub(crate) fn spawn_root(&mut self, source: VmProcessSource) -> VmProcessId {
        let pid = self.processes.spawn_root(source);
        self.scheduler
            .enqueue_runnable(&self.processes, pid)
            .expect("fresh root process must be runnable");
        pid
    }

    /// Spawns and schedules a child actor.
    pub(crate) fn spawn_child(
        &mut self,
        parent: VmProcessId,
        source: VmProcessSource,
    ) -> Result<VmProcessId, String> {
        let pid = self.processes.spawn_child(parent, source)?;
        self.scheduler.enqueue_runnable(&self.processes, pid)?;
        Ok(pid)
    }

    /// Returns the process table for inspection.
    pub(crate) fn processes(&self) -> &VmProcessTable {
        &self.processes
    }

    /// Returns the number of scheduled actor processes.
    pub(crate) fn scheduled_len(&self) -> usize {
        self.scheduler.queued_len()
    }

    /// Registers a stable actor name.
    pub(crate) fn register_name(
        &mut self,
        name: impl Into<String>,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("actor name cannot be empty".to_string());
        }
        self.ensure_live_process(pid, "register")?;
        match self.names.get(&name).copied() {
            Some(existing) if existing == pid => Ok(()),
            Some(existing) => Err(format!(
                "actor name `{name}` is already registered to process {}",
                existing.as_u64()
            )),
            None => {
                self.names.insert(name, pid);
                Ok(())
            }
        }
    }

    /// Looks up an actor name.
    pub(crate) fn lookup_name(&self, name: &str) -> Option<VmProcessId> {
        self.names.get(name).copied()
    }

    /// Sends a message to a process and schedules the recipient.
    pub(crate) fn send(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let message_id = self.processes.send(sender, recipient, payload)?;
        self.scheduler
            .wake_process(&mut self.processes, recipient)?;
        Ok(message_id)
    }

    /// Sends a message to a named process.
    pub(crate) fn send_named(
        &mut self,
        sender: VmProcessId,
        recipient_name: &str,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let recipient = self
            .names
            .get(recipient_name)
            .copied()
            .ok_or_else(|| format!("actor name `{recipient_name}` is not registered"))?;
        self.send(sender, recipient, payload)
    }

    /// Receives the oldest message or blocks the actor when the mailbox is
    /// empty.
    pub(crate) fn receive_next_or_block(
        &mut self,
        pid: VmProcessId,
    ) -> Result<VmActorReceive, String> {
        self.ensure_live_process(pid, "receive")?;
        let process = self
            .processes
            .get_mut(pid)
            .expect("process was checked before receive");
        if let Some(message) = process.receive_next() {
            Ok(VmActorReceive::Message(message))
        } else {
            process.block();
            Ok(VmActorReceive::Blocked)
        }
    }

    /// Receives the first selected message or blocks the actor.
    pub(crate) fn selective_receive_or_block(
        &mut self,
        pid: VmProcessId,
        predicate: impl FnMut(&VmMessage) -> bool,
    ) -> Result<VmActorReceive, String> {
        self.ensure_live_process(pid, "receive")?;
        let process = self
            .processes
            .get_mut(pid)
            .expect("process was checked before receive");
        if let Some(message) = process.selective_receive(predicate) {
            Ok(VmActorReceive::Message(message))
        } else {
            process.block();
            Ok(VmActorReceive::Blocked)
        }
    }

    /// Receives a message or reports an immediate timeout.
    pub(crate) fn receive_with_timeout(
        &mut self,
        pid: VmProcessId,
        timeout_ticks: u64,
    ) -> Result<VmActorReceive, String> {
        self.ensure_live_process(pid, "receive")?;
        let process = self
            .processes
            .get_mut(pid)
            .expect("process was checked before timeout receive");
        if let Some(message) = process.receive_next() {
            return Ok(VmActorReceive::Message(message));
        }
        if timeout_ticks == 0 {
            Ok(VmActorReceive::Timeout)
        } else {
            process.block();
            Ok(VmActorReceive::Blocked)
        }
    }

    /// Exits an actor and removes all names pointing at it.
    pub(crate) fn exit_actor(
        &mut self,
        pid: VmProcessId,
        reason: VmExitReason,
    ) -> Result<Vec<String>, String> {
        let cleanup = self.processes.exit_process(pid, reason)?;
        self.names.retain(|_, named_pid| *named_pid != pid);
        Ok(cleanup)
    }

    /// Runs the next scheduled actor slice.
    pub(crate) fn run_next(
        &mut self,
        run_slice: impl FnMut(&mut super::process::VmProcess, VmSchedulerSlice) -> VmSchedulerDecision,
    ) -> Result<VmSchedulerRun, String> {
        self.scheduler.run_next(&mut self.processes, run_slice)
    }

    fn ensure_live_process(&self, pid: VmProcessId, action: &str) -> Result<(), String> {
        let process = self
            .processes
            .get(pid)
            .ok_or_else(|| format!("cannot {action} missing process {}", pid.as_u64()))?;
        if matches!(process.state, super::process::VmProcessState::Exited(_)) {
            return Err(format!("cannot {action} exited process {}", pid.as_u64()));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "actor_test.rs"]
mod actor_test;
