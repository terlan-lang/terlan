#![allow(dead_code)]

use std::collections::{BTreeSet, VecDeque};

use super::process::{VmExitReason, VmProcess, VmProcessId, VmProcessState, VmProcessTable};

/// Scheduler configuration for local VM process execution.
///
/// Inputs:
/// - Reduction budget and empty-poll guard values supplied by runtime setup.
///
/// Output:
/// - Clamped scheduler configuration.
///
/// Transformation:
/// - Prevents zero-value configuration from creating non-progressing runtime
///   loops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulerConfig {
    pub(crate) reductions_per_slice: u64,
    pub(crate) max_empty_polls: usize,
}

impl VmSchedulerConfig {
    /// Creates a scheduler configuration and clamps zero values.
    pub(crate) fn new(reductions_per_slice: u64, max_empty_polls: usize) -> Self {
        Self {
            reductions_per_slice: reductions_per_slice.max(1),
            max_empty_polls: max_empty_polls.max(1),
        }
    }
}

impl Default for VmSchedulerConfig {
    fn default() -> Self {
        Self::new(100, 64)
    }
}

/// One scheduler slice handed to process execution.
///
/// Inputs:
/// - Selected process id, scheduler tick, and configured reduction budget.
///
/// Output:
/// - Immutable execution-slice metadata.
///
/// Transformation:
/// - Keeps the process runner independent from queue internals while still
///   making reductions and ticks visible to tests and future diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulerSlice {
    pub(crate) pid: VmProcessId,
    pub(crate) tick: u64,
    pub(crate) reduction_budget: u64,
}

/// Decision returned by one process-slice execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSchedulerDecision {
    Yield {
        reductions: u64,
    },
    Block {
        reductions: u64,
    },
    Exit {
        reductions: u64,
        reason: VmExitReason,
    },
}

impl VmSchedulerDecision {
    /// Returns reductions charged by the slice decision.
    fn reductions(&self) -> u64 {
        match self {
            VmSchedulerDecision::Yield { reductions }
            | VmSchedulerDecision::Block { reductions }
            | VmSchedulerDecision::Exit { reductions, .. } => *reductions,
        }
    }
}

/// Outcome of one scheduler poll.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSchedulerOutcome {
    Ran,
    Blocked,
    Exited(Vec<String>),
    Cancelled(Vec<String>),
    Idle,
}

/// Stable result of one scheduler poll.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulerRun {
    pub(crate) pid: Option<VmProcessId>,
    pub(crate) tick: u64,
    pub(crate) reductions_charged: u64,
    pub(crate) outcome: VmSchedulerOutcome,
}

/// VM-owned cooperative scheduler.
///
/// Inputs:
/// - Process ids from the VM process table.
///
/// Output:
/// - Deterministic process-slice execution order and diagnostics.
///
/// Transformation:
/// - Owns runnable queue semantics without relying on OTP, Tokio, or host
///   runtime scheduling behavior.
#[derive(Debug)]
pub(crate) struct VmScheduler {
    config: VmSchedulerConfig,
    queue: VecDeque<VmProcessId>,
    queued: BTreeSet<VmProcessId>,
    tick: u64,
}

impl VmScheduler {
    /// Creates a scheduler from explicit configuration.
    pub(crate) fn new(config: VmSchedulerConfig) -> Self {
        Self {
            config,
            queue: VecDeque::new(),
            queued: BTreeSet::new(),
            tick: 0,
        }
    }

    /// Returns the number of queued process ids.
    pub(crate) fn queued_len(&self) -> usize {
        self.queue.len()
    }

    /// Enqueues a runnable process if it is not already queued.
    pub(crate) fn enqueue_runnable(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let process = processes
            .get(pid)
            .ok_or_else(|| format!("cannot enqueue missing process {}", pid.as_u64()))?;
        match &process.state {
            VmProcessState::Runnable => self.enqueue_unchecked(pid),
            VmProcessState::Blocked => {
                Err(format!("cannot enqueue blocked process {}", pid.as_u64()))
            }
            VmProcessState::Exited(_) => {
                Err(format!("cannot enqueue exited process {}", pid.as_u64()))
            }
        }
    }

    /// Wakes a blocked process and enqueues it.
    pub(crate) fn wake_process(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let process = processes
            .get_mut(pid)
            .ok_or_else(|| format!("cannot wake missing process {}", pid.as_u64()))?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!("cannot wake exited process {}", pid.as_u64()));
        }
        process.wake();
        self.enqueue_unchecked(pid)
    }

    /// Requests cooperative cancellation for a process.
    pub(crate) fn request_cancellation(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let process = processes
            .get_mut(pid)
            .ok_or_else(|| format!("cannot cancel missing process {}", pid.as_u64()))?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!("cannot cancel exited process {}", pid.as_u64()));
        }
        process.request_cancellation();
        Ok(())
    }

    /// Runs the next runnable process slice.
    pub(crate) fn run_next(
        &mut self,
        processes: &mut VmProcessTable,
        mut run_slice: impl FnMut(&mut VmProcess, VmSchedulerSlice) -> VmSchedulerDecision,
    ) -> Result<VmSchedulerRun, String> {
        for _ in 0..self.config.max_empty_polls {
            let Some(pid) = self.dequeue() else {
                return Ok(self.idle_run());
            };
            let Some(process) = processes.get(pid) else {
                return Err(format!("scheduled process {} is missing", pid.as_u64()));
            };
            if process.state != VmProcessState::Runnable {
                continue;
            }
            if process.cancellation_requested {
                return self.cancel_process(processes, pid);
            }
            self.tick = self.tick.saturating_add(1);
            let slice = VmSchedulerSlice {
                pid,
                tick: self.tick,
                reduction_budget: self.config.reductions_per_slice,
            };
            let process = processes
                .get_mut(pid)
                .expect("process was checked immediately before slice execution");
            let decision = run_slice(process, slice);
            process.charge_reductions(decision.reductions());
            return Ok(self.apply_decision(processes, pid, decision));
        }
        Ok(self.idle_run())
    }

    /// Injects a queued id for adversarial scheduler tests.
    #[cfg(test)]
    pub(crate) fn enqueue_for_test(&mut self, pid: VmProcessId) {
        let _ = self.enqueue_unchecked(pid);
    }

    fn enqueue_unchecked(&mut self, pid: VmProcessId) -> Result<(), String> {
        if self.queued.insert(pid) {
            self.queue.push_back(pid);
        }
        Ok(())
    }

    fn dequeue(&mut self) -> Option<VmProcessId> {
        let pid = self.queue.pop_front()?;
        self.queued.remove(&pid);
        Some(pid)
    }

    fn idle_run(&self) -> VmSchedulerRun {
        VmSchedulerRun {
            pid: None,
            tick: self.tick,
            reductions_charged: 0,
            outcome: VmSchedulerOutcome::Idle,
        }
    }

    fn cancel_process(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
    ) -> Result<VmSchedulerRun, String> {
        self.tick = self.tick.saturating_add(1);
        let cleanup = processes.exit_process(pid, VmExitReason::Killed)?;
        Ok(VmSchedulerRun {
            pid: Some(pid),
            tick: self.tick,
            reductions_charged: 0,
            outcome: VmSchedulerOutcome::Cancelled(cleanup),
        })
    }

    fn apply_decision(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        decision: VmSchedulerDecision,
    ) -> VmSchedulerRun {
        let reductions_charged = decision.reductions();
        match decision {
            VmSchedulerDecision::Yield { .. } => {
                if processes
                    .get(pid)
                    .is_some_and(|process| process.state == VmProcessState::Runnable)
                {
                    let _ = self.enqueue_unchecked(pid);
                }
                VmSchedulerRun {
                    pid: Some(pid),
                    tick: self.tick,
                    reductions_charged,
                    outcome: VmSchedulerOutcome::Ran,
                }
            }
            VmSchedulerDecision::Block { .. } => {
                if let Some(process) = processes.get_mut(pid) {
                    process.block();
                }
                VmSchedulerRun {
                    pid: Some(pid),
                    tick: self.tick,
                    reductions_charged,
                    outcome: VmSchedulerOutcome::Blocked,
                }
            }
            VmSchedulerDecision::Exit { reason, .. } => {
                let cleanup = processes.exit_process(pid, reason).unwrap_or_default();
                VmSchedulerRun {
                    pid: Some(pid),
                    tick: self.tick,
                    reductions_charged,
                    outcome: VmSchedulerOutcome::Exited(cleanup),
                }
            }
        }
    }
}

impl Default for VmScheduler {
    fn default() -> Self {
        Self::new(VmSchedulerConfig::default())
    }
}

#[cfg(test)]
#[path = "scheduler_test.rs"]
mod scheduler_test;
