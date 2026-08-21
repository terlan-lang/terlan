//! Scheduler-owned execution control for live debugger sessions.

/// Maximum number of runnable slices one debugger command may authorize.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) const VM_DEBUGGER_MAX_STEP_SLICES: u64 = 1_024;

/// Live execution state owned by one fixed scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDebuggerExecutionState {
    /// Runnable actor slices receive normal scheduler service.
    Running,
    /// Runnable actor slices remain queued while owner commands still run.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    Paused,
    /// Only the retained number of debugger-authorized slices may run.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    Stepping,
}

/// One command sent to the scheduler that exclusively owns runnable mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) enum VmDebuggerControlCommand {
    /// Stops future runnable slices after the current owner command completes.
    Pause,
    /// Restores normal runnable service and clears unused step permits.
    Continue,
    /// Authorizes a bounded number of runnable actor slices while paused.
    Step {
        /// Positive number of scheduler slices to execute.
        slices: u64,
    },
}

/// Immutable debugger execution state returned by the scheduler owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) struct VmDebuggerControlSnapshot {
    /// Current scheduler-owned execution mode.
    pub(crate) state: VmDebuggerExecutionState,
    /// Runnable slices still authorized by the current step command.
    pub(crate) remaining_step_slices: u64,
}

/// Permit returned when the scheduler is allowed to service runnable work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDebuggerSlicePermit {
    /// Normal scheduler service while the debugger is not paused.
    Running,
    /// One debugger-authorized actor slice.
    Step,
}

/// Mutable debugger control state confined to one scheduler owner thread.
#[derive(Debug)]
pub(crate) struct VmDebuggerScheduleControl {
    state: VmDebuggerExecutionState,
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    remaining_step_slices: u64,
}

impl VmDebuggerScheduleControl {
    /// Creates one scheduler in its normal runnable state.
    pub(crate) const fn running() -> Self {
        Self {
            state: VmDebuggerExecutionState::Running,
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            remaining_step_slices: 0,
        }
    }

    /// Applies one owner-thread command and returns the resulting state.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn apply(
        &mut self,
        command: VmDebuggerControlCommand,
    ) -> Result<VmDebuggerControlSnapshot, String> {
        match command {
            VmDebuggerControlCommand::Pause => {
                self.state = VmDebuggerExecutionState::Paused;
                self.remaining_step_slices = 0;
            }
            VmDebuggerControlCommand::Continue => {
                self.state = VmDebuggerExecutionState::Running;
                self.remaining_step_slices = 0;
            }
            VmDebuggerControlCommand::Step { slices } => {
                if self.state == VmDebuggerExecutionState::Running {
                    return Err(
                        "error[vm.debugger.step_running]: pause the scheduler before stepping"
                            .to_string(),
                    );
                }
                if slices == 0 {
                    return Err(
                        "error[vm.debugger.step_count]: step count must be positive".to_string()
                    );
                }
                if slices > VM_DEBUGGER_MAX_STEP_SLICES {
                    return Err(format!(
                        "error[vm.debugger.step_count]: step count {slices} exceeds {}",
                        VM_DEBUGGER_MAX_STEP_SLICES
                    ));
                }
                self.state = VmDebuggerExecutionState::Stepping;
                self.remaining_step_slices = slices;
            }
        }
        Ok(self.snapshot())
    }

    /// Claims permission for one already-observed runnable actor slice.
    pub(crate) fn claim_runnable_slice(&mut self) -> Option<VmDebuggerSlicePermit> {
        match self.state {
            VmDebuggerExecutionState::Running => Some(VmDebuggerSlicePermit::Running),
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            VmDebuggerExecutionState::Paused => None,
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            VmDebuggerExecutionState::Stepping => {
                let remaining = self
                    .remaining_step_slices
                    .checked_sub(1)
                    .expect("stepping state always retains a positive permit");
                self.remaining_step_slices = remaining;
                if remaining == 0 {
                    self.state = VmDebuggerExecutionState::Paused;
                }
                Some(VmDebuggerSlicePermit::Step)
            }
        }
    }

    /// Returns whether queued work can currently claim a runnable slice.
    pub(crate) const fn can_service_runnable(&self) -> bool {
        match self.state {
            VmDebuggerExecutionState::Running => true,
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            VmDebuggerExecutionState::Paused => false,
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            VmDebuggerExecutionState::Stepping => self.remaining_step_slices > 0,
        }
    }

    /// Returns immutable scheduler-owned debugger state.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) const fn snapshot(&self) -> VmDebuggerControlSnapshot {
        VmDebuggerControlSnapshot {
            state: self.state,
            remaining_step_slices: self.remaining_step_slices,
        }
    }
}

#[cfg(test)]
#[path = "debugger_control_test.rs"]
#[cfg(test)]
mod debugger_control_test;
