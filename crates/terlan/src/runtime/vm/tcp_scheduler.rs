use super::tcp::VmTcpWake;
#[cfg(test)]
use super::{
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
};

#[cfg(test)]
#[path = "tcp_scheduler_test.rs"]
#[cfg(test)]
mod tcp_scheduler_test;

/// Summary of scheduler wakeups produced by VM TCP readiness events.
///
/// Inputs:
/// - TCP wake intents and VM process/scheduler state.
///
/// Output:
/// - Counted accept/read wakeups plus stable diagnostics for stale processes.
///
/// Transformation:
/// - Keeps TCP resource readiness independent from scheduler queue internals
///   while still making the runtime handoff executable and testable.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmTcpWakeReport {
    pub(crate) accept_wakeups: usize,
    pub(crate) read_wakeups: usize,
    pub(crate) write_wakeups: usize,
    pub(crate) diagnostics: Vec<String>,
}

/// Applies TCP readiness wake intents to the VM scheduler.
///
/// Inputs:
/// - Mutable process table, mutable scheduler, and readiness intents emitted
///   by `VmTcpRuntime`.
///
/// Output:
/// - Wake report with successful wake counts and diagnostics.
///
/// Transformation:
/// - Wakes the target VM process for each valid TCP readiness event without
///   letting TCP own scheduler queue semantics.
#[cfg(test)]
pub(crate) fn apply_tcp_wakeups(
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
    wakeups: impl IntoIterator<Item = VmTcpWake>,
) -> VmTcpWakeReport {
    let mut report = VmTcpWakeReport::default();
    for wakeup in wakeups {
        let process = wakeup.process();
        match scheduler.wake_process(processes, process) {
            Ok(()) => match wakeup {
                VmTcpWake::Accept { .. } => report.accept_wakeups += 1,
                VmTcpWake::Read { .. } => report.read_wakeups += 1,
                VmTcpWake::Write { .. } => report.write_wakeups += 1,
            },
            Err(reason) => report.diagnostics.push(format!(
                "VM TCP wake for process {} failed: {reason}",
                process.as_u64()
            )),
        }
    }
    report
}

impl VmTcpWake {
    /// Returns the process targeted by this readiness wake intent.
    #[cfg(test)]
    fn process(self) -> VmProcessId {
        match self {
            VmTcpWake::Accept { process, .. }
            | VmTcpWake::Read { process, .. }
            | VmTcpWake::Write { process, .. } => process,
        }
    }
}
