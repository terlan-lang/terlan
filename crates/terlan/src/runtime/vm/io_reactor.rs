#![allow(dead_code)]

use std::collections::VecDeque;

use super::acme_worker::VmAcmeWorkerWake;
use super::debugger_transport::VmDebuggerWake;
use super::package_transport::VmPackageDownloadWake;
use super::process::{VmProcessId, VmProcessTable};
use super::scheduler::VmScheduler;
use super::tcp::VmTcpWake;
use super::timer::VmTimerEvent;
use super::udp::VmUdpWake;

const MAX_CONSECUTIVE_TIMER_WAKEUPS: usize = 32;

#[cfg(test)]
#[path = "io_reactor_test.rs"]
mod io_reactor_test;

/// VM-owned I/O wake intent normalized from concrete protocol runtimes.
///
/// Inputs:
/// - TCP, UDP, package download, debugger, ACME, and timer wake events.
///
/// Output:
/// - One scheduler-facing wake item with stable diagnostic metadata.
///
/// Transformation:
/// - Keeps protocol-specific readiness producers independent from scheduler
///   queue details while giving the VM a single place to own readiness order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmIoReactorWake {
    TcpAccept { process: VmProcessId },
    TcpRead { process: VmProcessId },
    TcpWrite { process: VmProcessId },
    UdpReceive { process: VmProcessId },
    PackageChunk { process: VmProcessId },
    PackageComplete { process: VmProcessId },
    DebuggerCommand { process: VmProcessId },
    DebuggerEvent { process: VmProcessId },
    AcmeRenewalDue { process: VmProcessId },
    AcmeIssuanceReady { process: VmProcessId },
    AcmeChallengeReady { process: VmProcessId },
    AcmeCacheWriteReady { process: VmProcessId },
    AcmeTerminal { process: VmProcessId },
    TimerFired { process: VmProcessId },
    TimerDeadlineMissed { process: VmProcessId },
    TimerCoalesced { process: VmProcessId },
    TimerOverflow { process: VmProcessId },
}

impl VmIoReactorWake {
    fn process(&self) -> VmProcessId {
        match self {
            Self::TcpAccept { process }
            | Self::TcpRead { process }
            | Self::TcpWrite { process }
            | Self::UdpReceive { process }
            | Self::PackageChunk { process }
            | Self::PackageComplete { process }
            | Self::DebuggerCommand { process }
            | Self::DebuggerEvent { process }
            | Self::AcmeRenewalDue { process }
            | Self::AcmeIssuanceReady { process }
            | Self::AcmeChallengeReady { process }
            | Self::AcmeCacheWriteReady { process }
            | Self::AcmeTerminal { process }
            | Self::TimerFired { process }
            | Self::TimerDeadlineMissed { process }
            | Self::TimerCoalesced { process }
            | Self::TimerOverflow { process } => *process,
        }
    }

    fn trace_label(&self) -> &'static str {
        match self {
            Self::TcpAccept { .. } => "tcp.accept",
            Self::TcpRead { .. } => "tcp.read",
            Self::TcpWrite { .. } => "tcp.write",
            Self::UdpReceive { .. } => "udp.receive",
            Self::PackageChunk { .. } => "package.chunk",
            Self::PackageComplete { .. } => "package.complete",
            Self::DebuggerCommand { .. } => "debugger.command",
            Self::DebuggerEvent { .. } => "debugger.event",
            Self::AcmeRenewalDue { .. } => "acme.renewal_due",
            Self::AcmeIssuanceReady { .. } => "acme.issuance_ready",
            Self::AcmeChallengeReady { .. } => "acme.challenge_ready",
            Self::AcmeCacheWriteReady { .. } => "acme.cache_write_ready",
            Self::AcmeTerminal { .. } => "acme.terminal",
            Self::TimerFired { .. } => "timer.fired",
            Self::TimerDeadlineMissed { .. } => "timer.deadline_missed",
            Self::TimerCoalesced { .. } => "timer.coalesced",
            Self::TimerOverflow { .. } => "timer.overflow",
        }
    }
}

/// Wake counts emitted by one deterministic reactor drain.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmIoReactorWakeCounts {
    pub(crate) tcp: usize,
    pub(crate) udp: usize,
    pub(crate) package_download: usize,
    pub(crate) debugger: usize,
    pub(crate) acme_worker: usize,
    pub(crate) timer: usize,
}

/// Stable result of draining VM I/O readiness into the scheduler.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmIoReactorDrain {
    pub(crate) counts: VmIoReactorWakeCounts,
    pub(crate) deterministic_trace: Vec<String>,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) max_consecutive_timer_wakeups: usize,
    pub(crate) fairness_interleaves: usize,
    pub(crate) timer_outcomes: Vec<VmTimerOutcomeTrace>,
}

/// Typed timer outcome retained independently from scheduler wake decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmTimerOutcomeTrace {
    pub(crate) timer_id: u64,
    pub(crate) owner: u64,
    pub(crate) kind: &'static str,
    pub(crate) outcome: &'static str,
    pub(crate) detail: Option<String>,
}

impl VmIoReactorDrain {
    /// Returns the number of wake attempts handled by this drain.
    pub(crate) fn total_wakeups(&self) -> usize {
        self.counts.tcp
            + self.counts.udp
            + self.counts.package_download
            + self.counts.debugger
            + self.counts.acme_worker
            + self.counts.timer
    }
}

/// Single VM-owned I/O reactor loop.
///
/// Inputs:
/// - Readiness wakeups produced by protocol-specific VM runtimes.
///
/// Output:
/// - Deterministic scheduler wakeups plus trace and stale-process diagnostics.
///
/// Transformation:
/// - Centralizes scheduler ownership for I/O readiness so TCP, UDP, package
///   transport, debugger transport, ACME worker, and timer behavior can move
///   through one VM reactor path instead of independent special cases.
#[derive(Debug, Default)]
pub(crate) struct VmIoReactorLoop {
    pending: VecDeque<VmIoReactorWake>,
    timer_outcomes: VecDeque<VmTimerOutcomeTrace>,
}

impl VmIoReactorLoop {
    /// Creates an empty VM I/O reactor loop.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns pending wakeup count.
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Enqueues one normalized wakeup.
    pub(crate) fn enqueue_wake(&mut self, wake: VmIoReactorWake) {
        self.pending.push_back(wake);
    }

    /// Enqueues TCP readiness.
    pub(crate) fn enqueue_tcp_wake(&mut self, wake: VmTcpWake) {
        match wake {
            VmTcpWake::Accept { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::TcpAccept { process });
            }
            VmTcpWake::Read { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::TcpRead { process });
            }
            VmTcpWake::Write { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::TcpWrite { process });
            }
        }
    }

    /// Enqueues UDP packet readiness.
    pub(crate) fn enqueue_udp_wake(&mut self, wake: VmUdpWake) {
        match wake {
            VmUdpWake::Receive { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::UdpReceive { process });
            }
        }
    }

    /// Enqueues package download readiness.
    pub(crate) fn enqueue_package_download_wake(&mut self, wake: VmPackageDownloadWake) {
        match wake {
            VmPackageDownloadWake::Chunk { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::PackageChunk { process });
            }
            VmPackageDownloadWake::Complete { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::PackageComplete { process });
            }
        }
    }

    /// Enqueues debugger command/event readiness.
    pub(crate) fn enqueue_debugger_wake(&mut self, wake: VmDebuggerWake) {
        match wake {
            VmDebuggerWake::Command { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::DebuggerCommand { process });
            }
            VmDebuggerWake::Event { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::DebuggerEvent { process });
            }
        }
    }

    /// Enqueues ACME worker readiness.
    pub(crate) fn enqueue_acme_worker_wake(&mut self, wake: VmAcmeWorkerWake) {
        match wake {
            VmAcmeWorkerWake::RenewalDue { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::AcmeRenewalDue { process: owner });
            }
            VmAcmeWorkerWake::IssuanceReady { process, .. } => {
                self.enqueue_wake(VmIoReactorWake::AcmeIssuanceReady { process });
            }
            VmAcmeWorkerWake::ChallengeReady { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::AcmeChallengeReady { process: owner });
            }
            VmAcmeWorkerWake::CacheWriteReady { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::AcmeCacheWriteReady { process: owner });
            }
            VmAcmeWorkerWake::Terminal { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::AcmeTerminal { process: owner });
            }
        }
    }

    /// Enqueues a timer event that should make its owner runnable.
    pub(crate) fn enqueue_timer_event(&mut self, event: VmTimerEvent) {
        self.timer_outcomes.push_back(timer_outcome_trace(&event));
        match event {
            VmTimerEvent::Fired { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::TimerFired { process: owner });
            }
            VmTimerEvent::DeadlineMissed { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::TimerDeadlineMissed { process: owner });
            }
            VmTimerEvent::Coalesced { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::TimerCoalesced { process: owner });
            }
            VmTimerEvent::Overflow { owner, .. } => {
                self.enqueue_wake(VmIoReactorWake::TimerOverflow { process: owner });
            }
            VmTimerEvent::Cancelled { .. } | VmTimerEvent::OwnerExited { .. } => {}
        }
    }

    /// Drains all pending readiness into the VM scheduler.
    pub(crate) fn drain_ready(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
    ) -> VmIoReactorDrain {
        let mut drain = VmIoReactorDrain {
            timer_outcomes: self.timer_outcomes.drain(..).collect(),
            ..VmIoReactorDrain::default()
        };
        let mut consecutive_timer_wakeups = 0;
        while let Some((wake, fairness_interleaved)) = self.pop_next_wake(consecutive_timer_wakeups)
        {
            if fairness_interleaved {
                drain.fairness_interleaves += 1;
            }
            if wake.is_timer() {
                consecutive_timer_wakeups += 1;
                drain.max_consecutive_timer_wakeups = drain
                    .max_consecutive_timer_wakeups
                    .max(consecutive_timer_wakeups);
            } else {
                consecutive_timer_wakeups = 0;
            }
            count_wake(&mut drain.counts, &wake);
            let label = wake.trace_label();
            let process = wake.process();
            drain
                .deterministic_trace
                .push(format!("{label}:{}", process.as_u64()));
            if let Err(error) = scheduler.wake_process(processes, process) {
                drain.diagnostics.push(format!(
                    "{label}: failed to wake process {}: {error}",
                    process.as_u64()
                ));
            }
        }
        drain
    }

    fn pop_next_wake(
        &mut self,
        consecutive_timer_wakeups: usize,
    ) -> Option<(VmIoReactorWake, bool)> {
        if consecutive_timer_wakeups < MAX_CONSECUTIVE_TIMER_WAKEUPS {
            return self.pending.pop_front().map(|wake| (wake, false));
        }
        if let Some(non_timer_index) = self.pending.iter().position(|wake| !wake.is_timer()) {
            return self
                .pending
                .remove(non_timer_index)
                .map(|wake| (wake, true));
        }
        self.pending.pop_front().map(|wake| (wake, false))
    }
}

fn timer_outcome_trace(event: &VmTimerEvent) -> VmTimerOutcomeTrace {
    let (timer_id, owner, kind, outcome, detail) = match *event {
        VmTimerEvent::Fired {
            timer_id,
            owner,
            kind,
        } => (timer_id, owner, kind, "fired", None),
        VmTimerEvent::DeadlineMissed {
            timer_id,
            owner,
            kind,
            late_by_ticks,
        } => (
            timer_id,
            owner,
            kind,
            "deadline_missed",
            Some(format!("late_by_ticks={late_by_ticks}")),
        ),
        VmTimerEvent::Coalesced {
            timer_id,
            owner,
            kind,
            skipped_intervals,
            next_deadline_tick,
        } => (
            timer_id,
            owner,
            kind,
            "coalesced",
            Some(format!(
                "skipped_intervals={skipped_intervals},next_deadline_tick={next_deadline_tick}"
            )),
        ),
        VmTimerEvent::Overflow {
            timer_id,
            owner,
            kind,
        } => (timer_id, owner, kind, "overflow", None),
        VmTimerEvent::Cancelled {
            timer_id,
            owner,
            kind,
        } => (timer_id, owner, kind, "cancelled", None),
        VmTimerEvent::OwnerExited {
            timer_id,
            owner,
            kind,
        } => (timer_id, owner, kind, "owner_exited", None),
    };
    VmTimerOutcomeTrace {
        timer_id: timer_id.as_u64(),
        owner: owner.as_u64(),
        kind: kind.as_str(),
        outcome,
        detail,
    }
}

impl VmIoReactorWake {
    fn is_timer(&self) -> bool {
        matches!(
            self,
            Self::TimerFired { .. }
                | Self::TimerDeadlineMissed { .. }
                | Self::TimerCoalesced { .. }
                | Self::TimerOverflow { .. }
        )
    }
}

fn count_wake(counts: &mut VmIoReactorWakeCounts, wake: &VmIoReactorWake) {
    match wake {
        VmIoReactorWake::TcpAccept { .. }
        | VmIoReactorWake::TcpRead { .. }
        | VmIoReactorWake::TcpWrite { .. } => {
            counts.tcp += 1;
        }
        VmIoReactorWake::UdpReceive { .. } => {
            counts.udp += 1;
        }
        VmIoReactorWake::PackageChunk { .. } | VmIoReactorWake::PackageComplete { .. } => {
            counts.package_download += 1;
        }
        VmIoReactorWake::DebuggerCommand { .. } | VmIoReactorWake::DebuggerEvent { .. } => {
            counts.debugger += 1;
        }
        VmIoReactorWake::AcmeRenewalDue { .. }
        | VmIoReactorWake::AcmeIssuanceReady { .. }
        | VmIoReactorWake::AcmeChallengeReady { .. }
        | VmIoReactorWake::AcmeCacheWriteReady { .. }
        | VmIoReactorWake::AcmeTerminal { .. } => {
            counts.acme_worker += 1;
        }
        VmIoReactorWake::TimerFired { .. }
        | VmIoReactorWake::TimerDeadlineMissed { .. }
        | VmIoReactorWake::TimerCoalesced { .. }
        | VmIoReactorWake::TimerOverflow { .. } => {
            counts.timer += 1;
        }
    }
}
