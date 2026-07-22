use super::super::failure::exit_signal_message;
use super::super::fatal_diagnostics::{VmFatalDiagnosticBundle, VmFatalDiagnosticPolicy};
use super::{VmActorRuntime, VmExitReason, VmProcessId, ACTOR_OPERATION_REDUCTIONS};
use crate::runtime::vm::process::VmProcessState;

/// Observable result of delivering one local actor exit signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmActorExitSignalOutcome {
    IgnoredNormal,
    DeliveredMessage { message_id: u64 },
    Exited,
}

impl VmActorRuntime {
    /// Sends one local exit signal with BEAM-compatible trap, normal, and kill
    /// behavior while retaining Terlan-owned process identities.
    pub(crate) fn send_exit_signal(
        &mut self,
        sender: VmProcessId,
        target: VmProcessId,
        reason: VmExitReason,
    ) -> Result<VmActorExitSignalOutcome, String> {
        self.processes.validate_sender(sender)?;
        self.ensure_live_process(target, "signal")?;
        let traps = self
            .failures
            .trap_exits(&self.processes, target)
            .map_err(|error| error.to_string())?;

        if sender != target && reason == VmExitReason::Normal && !traps {
            self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
            return Ok(VmActorExitSignalOutcome::IgnoredNormal);
        }
        if reason != VmExitReason::Killed && traps {
            let message_id = self.processes.send_system_message(
                sender,
                target,
                exit_signal_message(sender, &reason),
            )?;
            self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
            self.scheduler
                .wake_process(&mut self.processes, target)
                .expect("exit-signal target was validated before wake");
            return Ok(VmActorExitSignalOutcome::DeliveredMessage { message_id });
        }

        self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
        self.exit_actor(target, reason)?;
        Ok(VmActorExitSignalOutcome::Exited)
    }

    /// Exits an actor and removes all names pointing at it.
    pub(crate) fn exit_actor(
        &mut self,
        pid: VmProcessId,
        reason: VmExitReason,
    ) -> Result<Vec<String>, String> {
        if self.processes.get(pid).is_none() {
            return Err(format!("missing process {}", pid.as_u64()));
        }
        if let Some(cause_code) = abnormal_exit_cause(&reason) {
            if let Ok(Some(bundle)) = VmFatalDiagnosticBundle::capture_with_native_image(
                VmFatalDiagnosticPolicy::enabled(4_096, 1024 * 1024)
                    .expect("fixed fatal diagnostic limits are valid"),
                pid.as_u64(),
                cause_code,
                &self.processes,
                &self.scheduler,
                &[pid],
                self.native_image_diagnostics.clone(),
            ) {
                self.latest_fatal_diagnostic = Some(bundle);
            }
        }
        let report = self
            .failures
            .exit_process(&mut self.processes, pid, reason)?;
        let initiated_exit = report.exited.contains(&pid);
        for exited in &report.exited {
            self.meta_trace.observer_exited(*exited);
            self.scheduler.forget_process(*exited);
            self.remove_native_continuation_for_owner(*exited);
            self.resources.cleanup_owner(*exited);
            self.code_server.release_process_bindings(*exited)?;
            self.dynamic_modules.cleanup_owner(*exited);
            for event in self.remove_delayed_messages_for_owner(*exited) {
                self.consume_postgres_timer_event(&event)?;
            }
            self.cleanup_postgres_owner(*exited);
            self.aliases.remove_process(*exited);
            self.memory.synchronize_process(&self.processes, *exited)?;
        }
        for recipient in report.message_recipients {
            if self
                .processes
                .get(recipient)
                .is_some_and(|process| !matches!(process.state, VmProcessState::Exited(_)))
            {
                self.scheduler
                    .wake_process(&mut self.processes, recipient)?;
            }
        }
        if initiated_exit {
            self.scheduler
                .charge_terminal_reductions(&mut self.processes, pid, ACTOR_OPERATION_REDUCTIONS)
                .expect("successfully exited actor remains available for terminal accounting");
        }
        Ok(report.cleanup_handles)
    }
}

/// Maps abnormal actor exits to bounded diagnostic cause identities.
fn abnormal_exit_cause(reason: &VmExitReason) -> Option<&'static str> {
    match reason {
        VmExitReason::Normal => None,
        VmExitReason::Error(_) => Some("actor.error"),
        VmExitReason::Killed => Some("actor.killed"),
        VmExitReason::ShutdownTimeout { .. } => Some("actor.shutdown-timeout"),
        VmExitReason::MemoryLimitExceeded { .. } => Some("actor.memory-limit-exceeded"),
    }
}
