//! Trace, restart, abort, and finalization control for one debug actor.

use super::*;

impl NativeDebuggerRuntime<'_> {
    pub(super) fn update_trace(
        &mut self,
        filter: &str,
        enabled: bool,
    ) -> Result<(), DebugCliError> {
        let named = matches!(
            filter,
            "calls"
                | "returns"
                | "transitions"
                | "sends"
                | "receives"
                | "mailbox"
                | "processes"
                | "resources"
                | "native_boundary"
                | "http"
                | "supervisors"
        );
        let qualified = ["process:", "module:", "function:", "message:", "resource:"]
            .iter()
            .any(|prefix| filter.starts_with(prefix));
        if !named && !qualified {
            return Err(format!(
                "error[vm.debugger.trace_filter]: unsupported trace filter `{filter}`"
            )
            .into());
        }
        if enabled {
            self.trace_filters.insert(filter.to_string());
            self.report.events.push(format!("trace_enabled:{filter}"));
        } else if self.trace_filters.remove(filter) {
            self.report.events.push(format!("trace_disabled:{filter}"));
        } else {
            return Err(format!(
                "error[vm.debugger.trace_missing]: trace filter `{filter}` is not enabled"
            )
            .into());
        }
        Ok(())
    }

    pub(super) fn restart(&mut self, choice: &str) -> Result<(), DebugCliError> {
        match choice {
            "skip" => self.resume_selected_restart("skip"),
            "retry" | "restart_process" => {
                let active = self.active.take().ok_or_else(|| {
                    "error[vm.debugger.restart_missing]: no recoverable condition is currently stopped"
                        .to_string()
                })?;
                if !matches!(
                    &active.state,
                    ActiveCallState::Suspended(suspension)
                        if suspension.operation() == TvmTransitionOperation::Failure
                ) {
                    self.active = Some(active);
                    return Err(
                        "error[vm.debugger.restart_missing]: retry requires a stopped failure"
                            .to_string()
                            .into(),
                    );
                }
                self.shard.cancel_call(active.owner, "debugger retry")?;
                let owner = self
                    .shard
                    .spawn_fixed_owner_actor(&active.function, active.source.arity)?;
                self.active = Some(ActiveCall {
                    owner,
                    instruction_offset: 0,
                    state: ActiveCallState::Ready,
                    ..active
                });
                self.control.apply(VmDebuggerControlCommand::Pause)?;
                self.report.execution_state = "paused".to_string();
                self.report.events.push(format!("{choice}:{owner:?}"));
                Ok(())
            }
            "abort_process" => self.abort("debugger abort_process restart"),
            choice => Err(format!(
                "error[vm.debugger.restart_unknown]: unknown restart `{choice}`"
            )
            .into()),
        }
    }

    pub(super) fn use_value(&mut self, value: &str) -> Result<(), DebugCliError> {
        if value != "Unit" {
            return Err(
                "error[vm.debugger.restart_value]: this stopped continuation accepts only Unit"
                    .to_string()
                    .into(),
            );
        }
        self.resume_selected_restart("use Unit")
    }

    fn resume_selected_restart(&mut self, event: &str) -> Result<(), DebugCliError> {
        let active = self.active.take().ok_or_else(|| {
            "error[vm.debugger.restart_missing]: no recoverable condition is currently stopped"
                .to_string()
        })?;
        let ActiveCallState::Suspended(suspension) = active.state else {
            self.active = Some(active);
            return Err(
                "error[vm.debugger.restart_missing]: active process is not stopped at a condition"
                    .to_string()
                    .into(),
            );
        };
        if !matches!(
            suspension.operation(),
            TvmTransitionOperation::Failure | TvmTransitionOperation::Debug
        ) {
            self.active = Some(ActiveCall {
                state: ActiveCallState::Suspended(suspension),
                ..active
            });
            return Err(
                "error[vm.debugger.restart_missing]: stopped transition has no typed restart"
                    .to_string()
                    .into(),
            );
        }
        let execution = self.shard.resume_debug_restart(active.owner, *suspension)?;
        let completed = self.accept_execution(
            ActiveCall {
                state: ActiveCallState::Ready,
                ..active
            },
            execution,
        )?;
        self.report.events.push(match completed {
            Some(operation) => format!("{event}:transition:{operation:?}"),
            None => format!("{event}:complete"),
        });
        Ok(())
    }

    pub(super) fn abort(&mut self, reason: &str) -> Result<(), DebugCliError> {
        if let Some(active) = self.active.take() {
            self.shard.cancel_call(active.owner, reason)?;
            self.report
                .events
                .push(format!("process_exited:{}:aborted", active.owner.as_u64()));
        }
        self.report.execution_state = "stopped".to_string();
        Ok(())
    }

    pub(in crate::commands::debug) fn finish(
        mut self,
    ) -> Result<NativeDebuggerExecutionReport, DebugCliError> {
        if let Some(active) = self.active.take() {
            self.shard
                .cancel_call(active.owner, "debugger script ended")
                .map_err(|message| DebugCliError {
                    code: "debug_native_runtime_failed",
                    message,
                })?;
            self.report.events.push(format!(
                "process_exited:{}:script_end",
                active.owner.as_u64()
            ));
        }
        Ok(self.report)
    }
}
