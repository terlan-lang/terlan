//! Source supervision runs against the actor runtime's actual process table.

use super::*;
use crate::runtime::vm::supervision::source::SourceChildStart;
use crate::runtime::vm::supervision::{VmSupervisionError, VmSupervisionErrorKind};

impl VmActorRuntime {
    pub(crate) fn take_supervised_retired(&mut self) -> Option<VmProcessId> {
        self.source_supervision.take_retired()
    }

    pub(crate) fn start_supervised_child(
        &mut self,
        owner: VmProcessId,
        policy: &ReplValue,
        spec: &ReplValue,
        recovery: Option<&ReplValue>,
        source: VmProcessSource,
    ) -> Result<VmProcessId, VmSupervisionError> {
        let child = self.source_supervision.start(
            &mut self.processes,
            owner,
            policy,
            spec,
            recovery,
            source,
        )?;
        self.scheduler
            .enqueue_runnable(&self.processes, child)
            .map_err(|message| {
                VmSupervisionError::new(VmSupervisionErrorKind::ChildLifecycle, message)
            })?;
        Ok(child)
    }

    pub(crate) fn supervised_operation(
        &mut self,
        owner: VmProcessId,
        operation: &str,
        args: &[ReplValue],
    ) -> Result<ReplValue, VmSupervisionError> {
        match (operation, args) {
            ("current_child", [policy, spec]) => self
                .source_supervision
                .current(owner, policy, spec, &self.processes)
                .map(|pid| ReplValue::Int(pid.as_u64() as i64)),
            ("snapshot", [policy]) => {
                self.source_supervision
                    .snapshot(owner, policy, &self.processes)
            }
            ("stop_child", [policy, spec]) => {
                if let Some(pid) = self.source_supervision.stop(
                    owner,
                    policy,
                    spec,
                    &mut self.processes,
                    &mut self.timers,
                    &mut self.scheduler,
                )? {
                    self.exit_actor(pid, VmExitReason::Killed)
                        .map_err(|message| {
                            VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message)
                        })?;
                }
                Ok(ReplValue::Unit)
            }
            ("checkpoint" | "append_event" | "restore", _) => {
                self.source_supervision.persistence(owner, operation, args)
            }
            _ => Err(VmSupervisionError::new(
                VmSupervisionErrorKind::Configuration,
                "unknown source supervision operation",
            )),
        }
    }

    /// Cleans selected siblings through the actor exit pipeline before the VM
    /// policy creates replacement identities or installs backoff deadlines.
    pub(crate) fn take_supervised_start(
        &mut self,
    ) -> Result<Option<SourceChildStart>, VmSupervisionError> {
        while let Some((root, id, reason, cleanup)) = self
            .source_supervision
            .next_exit(self.timers.current_tick())
        {
            for pid in cleanup {
                if self.processes.is_alive(pid) {
                    self.exit_actor(pid, reason.clone()).map_err(|message| {
                        VmSupervisionError::new(VmSupervisionErrorKind::Restart, message)
                    })?;
                }
            }
            self.source_supervision.restart(
                &mut self.processes,
                &mut self.timers,
                root,
                &id,
                reason,
            )?;
        }
        while let Some((pid, reason)) = self.source_supervision.take_termination() {
            if self.processes.is_alive(pid) {
                self.exit_actor(pid, reason).map_err(|message| {
                    VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message)
                })?;
            }
        }
        let next = self.source_supervision.take_ready()?;
        if let Some(start) = &next {
            self.scheduler
                .enqueue_runnable(&self.processes, start.pid)
                .map_err(|message| {
                    VmSupervisionError::new(VmSupervisionErrorKind::Restart, message)
                })?;
        }
        Ok(next)
    }
}
