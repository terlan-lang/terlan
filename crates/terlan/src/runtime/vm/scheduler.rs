#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;

use super::process::{
    VmExitReason, VmProcess, VmProcessId, VmProcessResumeState, VmProcessState, VmProcessTable,
};
pub(crate) use telemetry::{VmSchedulerMetrics, VmSchedulerQueueTransition};

#[path = "scheduler/telemetry.rs"]
mod telemetry;

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

/// Scheduling class assigned to one VM process.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum VmSchedulerClass {
    Priority,
    Normal,
    Background,
}

impl VmSchedulerClass {
    fn queue_index(self) -> usize {
        match self {
            Self::Priority => 0,
            Self::Normal => 1,
            Self::Background => 2,
        }
    }
}

const VM_SCHEDULER_CLASS_CYCLE: [VmSchedulerClass; 6] = [
    VmSchedulerClass::Priority,
    VmSchedulerClass::Priority,
    VmSchedulerClass::Normal,
    VmSchedulerClass::Priority,
    VmSchedulerClass::Normal,
    VmSchedulerClass::Background,
];
const VM_MEMORY_BYTES_PER_REDUCTION: usize = 1024;
const VM_SCHEDULER_OPERATION_REDUCTIONS: u64 = 1;

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
/// - Owns runnable queue semantics without relying on external runtime
///   scheduling behavior.
#[derive(Debug)]
pub(crate) struct VmScheduler {
    config: VmSchedulerConfig,
    queues: [VecDeque<VmProcessId>; 3],
    queued: BTreeSet<VmProcessId>,
    classes: BTreeMap<VmProcessId, VmSchedulerClass>,
    enqueued_at: BTreeMap<VmProcessId, u64>,
    class_cycle_cursor: usize,
    tick: u64,
    metrics: VmSchedulerMetrics,
}

impl VmScheduler {
    /// Creates a scheduler from explicit configuration.
    pub(crate) fn new(config: VmSchedulerConfig) -> Self {
        Self {
            config,
            queues: std::array::from_fn(|_| VecDeque::new()),
            queued: BTreeSet::new(),
            classes: BTreeMap::new(),
            enqueued_at: BTreeMap::new(),
            class_cycle_cursor: 0,
            tick: 0,
            metrics: VmSchedulerMetrics::default(),
        }
    }

    /// Returns the number of queued process ids.
    pub(crate) fn queued_len(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum()
    }

    /// Returns the current logical scheduler tick for immutable diagnostics.
    pub(crate) fn diagnostic_tick(&self) -> u64 {
        self.tick
    }

    /// Returns queued process identities in deterministic class and queue order.
    pub(crate) fn diagnostic_queued_processes(&self) -> Vec<VmProcessId> {
        self.queues
            .iter()
            .flat_map(|queue| queue.iter().copied())
            .collect()
    }

    /// Removes all scheduler-owned state for a process that exited outside a
    /// scheduler slice, including runnable entries from cascaded actor exits.
    pub(crate) fn forget_process(&mut self, pid: VmProcessId) {
        self.remove_queued(pid, "exit");
        self.classes.remove(&pid);
        self.enqueued_at.remove(&pid);
    }

    /// Returns cumulative deterministic scheduler accounting.
    pub(crate) fn metrics(&self) -> &VmSchedulerMetrics {
        &self.metrics
    }

    pub(crate) fn memory_reductions(&self, pid: VmProcessId) -> u64 {
        self.metrics
            .processes
            .get(&pid.as_u64())
            .map_or(0, |metrics| metrics.memory_reductions)
    }

    pub(crate) fn total_memory_reductions(&self) -> u64 {
        self.metrics.total_memory_reductions
    }

    /// Charges VM runtime work performed outside an executing process slice.
    pub(crate) fn charge_runtime_reductions(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        reductions: u64,
    ) -> Result<u64, String> {
        self.charge_process_reductions(processes, pid, reductions, "reductions", false)
    }

    /// Charges completed terminal work after a process has exited.
    pub(crate) fn charge_terminal_reductions(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        reductions: u64,
    ) -> Result<u64, String> {
        if processes
            .get(pid)
            .is_some_and(|process| !matches!(process.state, VmProcessState::Exited(_)))
        {
            return Err(format!(
                "cannot charge terminal reductions for live process {}",
                pid.as_u64()
            ));
        }
        self.charge_process_reductions(processes, pid, reductions, "terminal reductions", true)
    }

    fn charge_process_reductions(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        reductions: u64,
        charge_label: &str,
        allow_exited: bool,
    ) -> Result<u64, String> {
        let process = match processes.get_mut(pid) {
            Some(process) => process,
            None => {
                return Err(format!(
                    "cannot charge {charge_label} for missing process {}",
                    pid.as_u64()
                ));
            }
        };
        if !allow_exited && matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!(
                "cannot charge {charge_label} for exited process {}",
                pid.as_u64()
            ));
        }
        process.charge_reductions(reductions);
        self.metrics.total_reductions = self.metrics.total_reductions.saturating_add(reductions);
        let metrics = self.metrics.processes.entry(pid.as_u64()).or_default();
        metrics.pid = pid.as_u64();
        metrics.reductions = metrics.reductions.saturating_add(reductions);
        Ok(reductions)
    }

    /// Charges VM memory work outside an executing process slice.
    pub(crate) fn charge_memory_reductions(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        logical_bytes: usize,
    ) -> Result<u64, String> {
        let byte_reductions = u64::try_from(logical_bytes.div_ceil(VM_MEMORY_BYTES_PER_REDUCTION))
            .unwrap_or(u64::MAX);
        let reductions = byte_reductions.saturating_add(1);
        self.charge_process_reductions(processes, pid, reductions, "memory", false)?;
        self.metrics.total_memory_reductions = self
            .metrics
            .total_memory_reductions
            .saturating_add(reductions);
        let metrics = self.metrics.processes.entry(pid.as_u64()).or_default();
        metrics.memory_reductions = metrics.memory_reductions.saturating_add(reductions);
        Ok(reductions)
    }

    /// Enqueues a runnable process if it is not already queued.
    pub(crate) fn enqueue_runnable(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let class = self
            .classes
            .get(&pid)
            .copied()
            .unwrap_or(VmSchedulerClass::Normal);
        self.enqueue_runnable_with_class(processes, pid, class)
    }

    /// Enqueues a runnable process in an explicit deterministic class.
    pub(crate) fn enqueue_runnable_with_class(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
        class: VmSchedulerClass,
    ) -> Result<(), String> {
        let process = processes
            .get(pid)
            .ok_or_else(|| format!("cannot enqueue missing process {}", pid.as_u64()))?;
        match &process.state {
            VmProcessState::Runnable => {
                if self.queued.contains(&pid)
                    && self
                        .classes
                        .get(&pid)
                        .is_some_and(|existing| *existing != class)
                {
                    return Err(format!("cannot reclassify queued process {}", pid.as_u64()));
                }
                self.classes.insert(pid, class);
                self.enqueue_unchecked(pid)
            }
            VmProcessState::Blocked => {
                Err(format!("cannot enqueue blocked process {}", pid.as_u64()))
            }
            VmProcessState::Suspended(_) => {
                Err(format!("cannot enqueue suspended process {}", pid.as_u64()))
            }
            VmProcessState::Exited(_) => {
                Err(format!("cannot enqueue exited process {}", pid.as_u64()))
            }
        }
    }

    /// Changes the scheduling class of a live process without changing its state.
    pub(crate) fn set_process_class(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        class: VmSchedulerClass,
    ) -> Result<(), String> {
        let process = processes
            .get(pid)
            .ok_or_else(|| format!("cannot reclassify missing process {}", pid.as_u64()))?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!("cannot reclassify exited process {}", pid.as_u64()));
        }
        if self.classes.get(&pid).copied() == Some(class) {
            self.charge_runtime_reductions(processes, pid, VM_SCHEDULER_OPERATION_REDUCTIONS)?;
            return Ok(());
        }

        let was_queued = self.queued.contains(&pid);
        if was_queued {
            self.remove_queued(pid, "reclassify");
        }
        self.classes.insert(pid, class);
        if was_queued {
            self.enqueue_unchecked(pid)?;
        }
        self.charge_runtime_reductions(processes, pid, VM_SCHEDULER_OPERATION_REDUCTIONS)?;
        Ok(())
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
        match process.state {
            VmProcessState::Exited(_) => {
                return Err(format!("cannot wake exited process {}", pid.as_u64()));
            }
            VmProcessState::Suspended(_) => {
                process.wake();
                return Ok(());
            }
            VmProcessState::Runnable | VmProcessState::Blocked => process.wake(),
        }
        self.enqueue_unchecked(pid)
    }

    /// Suspends a live process and removes any runnable queue entry.
    pub(crate) fn suspend_process(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let process = processes
            .get_mut(pid)
            .ok_or_else(|| format!("cannot suspend missing process {}", pid.as_u64()))?;
        process
            .suspend()
            .map_err(|_| format!("cannot suspend exited process {}", pid.as_u64()))?;
        self.remove_queued(pid, "suspend");
        Ok(())
    }

    /// Resumes a suspended process and queues it only when runnable work exists.
    pub(crate) fn resume_process(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(), String> {
        let process = processes
            .get_mut(pid)
            .ok_or_else(|| format!("cannot resume missing process {}", pid.as_u64()))?;
        let resume_state = process.resume().map_err(|reason| {
            if matches!(process.state, VmProcessState::Exited(_)) {
                format!("cannot resume exited process {}", pid.as_u64())
            } else {
                format!("cannot resume process {}: {reason}", pid.as_u64())
            }
        })?;
        if resume_state == VmProcessResumeState::Runnable {
            self.enqueue_unchecked(pid)?;
        }
        Ok(())
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
        self.charge_runtime_reductions(processes, pid, VM_SCHEDULER_OPERATION_REDUCTIONS)?;
        Ok(())
    }

    /// Runs the next runnable process slice.
    pub(crate) fn run_next(
        &mut self,
        processes: &mut VmProcessTable,
        mut run_slice: impl FnMut(&mut VmProcess, VmSchedulerSlice) -> VmSchedulerDecision,
    ) -> Result<VmSchedulerRun, String> {
        for _ in 0..self.config.max_empty_polls {
            let Some((pid, enqueued_tick)) = self.dequeue() else {
                return Ok(self.idle_run());
            };
            let Some(process) = processes.get(pid) else {
                return Err(format!("scheduled process {} is missing", pid.as_u64()));
            };
            if matches!(process.state, VmProcessState::Exited(_)) {
                self.classes.remove(&pid);
                continue;
            }
            if process.state != VmProcessState::Runnable {
                continue;
            }
            if process.cancellation_requested {
                self.tick = self.tick.saturating_add(1);
                let run = self.cancel_process(processes, pid, 0)?;
                self.record_run(pid, &run, enqueued_tick, false);
                return Ok(run);
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
            let preempted = matches!(&decision, VmSchedulerDecision::Yield { .. })
                && decision.reductions() >= slice.reduction_budget;
            let cancellation_at_boundary = process.cancellation_requested
                && !matches!(&decision, VmSchedulerDecision::Exit { .. });
            if cancellation_at_boundary {
                let run = self.cancel_process(processes, pid, decision.reductions())?;
                self.record_run(pid, &run, enqueued_tick, preempted);
                return Ok(run);
            }
            let run = self.apply_decision(processes, pid, decision);
            self.record_run(pid, &run, enqueued_tick, preempted);
            return Ok(run);
        }
        Ok(self.idle_run())
    }

    /// Injects a queued id for adversarial scheduler tests.
    #[cfg(test)]
    pub(crate) fn enqueue_for_test(&mut self, pid: VmProcessId) {
        let _ = self.enqueue_unchecked(pid);
    }

    fn enqueue_unchecked(&mut self, pid: VmProcessId) -> Result<(), String> {
        let class = self
            .classes
            .get(&pid)
            .copied()
            .unwrap_or(VmSchedulerClass::Normal);
        self.classes.entry(pid).or_insert(class);
        if self.queued.insert(pid) {
            self.queues[class.queue_index()].push_back(pid);
            self.enqueued_at.insert(pid, self.tick);
            self.metrics.max_queue_depth = self.metrics.max_queue_depth.max(self.queued_len());
            self.metrics
                .queue_transitions
                .push(VmSchedulerQueueTransition {
                    tick: self.tick,
                    pid: pid.as_u64(),
                    action: "enqueue",
                    class,
                    queue_len: self.queued_len(),
                });
        }
        Ok(())
    }

    fn remove_queued(&mut self, pid: VmProcessId, action: &'static str) {
        if !self.queued.remove(&pid) {
            return;
        }
        let class = self
            .classes
            .get(&pid)
            .copied()
            .unwrap_or(VmSchedulerClass::Normal);
        self.queues[class.queue_index()].retain(|queued_pid| *queued_pid != pid);
        self.enqueued_at.remove(&pid);
        self.metrics
            .queue_transitions
            .push(VmSchedulerQueueTransition {
                tick: self.tick,
                pid: pid.as_u64(),
                action,
                class,
                queue_len: self.queued_len(),
            });
    }

    fn dequeue(&mut self) -> Option<(VmProcessId, u64)> {
        for _ in 0..VM_SCHEDULER_CLASS_CYCLE.len() {
            let class = VM_SCHEDULER_CLASS_CYCLE[self.class_cycle_cursor];
            self.class_cycle_cursor =
                (self.class_cycle_cursor + 1) % VM_SCHEDULER_CLASS_CYCLE.len();
            let Some(pid) = self.queues[class.queue_index()].pop_front() else {
                continue;
            };
            self.queued.remove(&pid);
            let enqueued_tick = self.enqueued_at.remove(&pid).unwrap_or(self.tick);
            self.metrics
                .queue_transitions
                .push(VmSchedulerQueueTransition {
                    tick: self.tick,
                    pid: pid.as_u64(),
                    action: "dequeue",
                    class,
                    queue_len: self.queued_len(),
                });
            return Some((pid, enqueued_tick));
        }
        None
    }

    fn record_run(
        &mut self,
        pid: VmProcessId,
        run: &VmSchedulerRun,
        enqueued_tick: u64,
        preempted: bool,
    ) {
        let wait_ticks = run.tick.saturating_sub(enqueued_tick);
        self.metrics.total_slices = self.metrics.total_slices.saturating_add(1);
        self.metrics.total_reductions = self
            .metrics
            .total_reductions
            .saturating_add(run.reductions_charged);
        if preempted {
            self.metrics.preemptions = self.metrics.preemptions.saturating_add(1);
        }
        let process = self.metrics.processes.entry(pid.as_u64()).or_default();
        process.pid = pid.as_u64();
        process.reductions = process.reductions.saturating_add(run.reductions_charged);
        process.slices = process.slices.saturating_add(1);
        process.preemptions = process.preemptions.saturating_add(u64::from(preempted));
        process.max_wait_ticks = process.max_wait_ticks.max(wait_ticks);
        process.first_run_tick.get_or_insert(run.tick);
        process.last_run_tick = Some(run.tick);
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
        reductions_charged: u64,
    ) -> Result<VmSchedulerRun, String> {
        let cleanup = processes
            .exit_process(pid, VmExitReason::Killed)
            .expect("cancelled process was checked before scheduler exit");
        self.classes.remove(&pid);
        Ok(VmSchedulerRun {
            pid: Some(pid),
            tick: self.tick,
            reductions_charged,
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
                } else {
                    self.classes.remove(&pid);
                }
                VmSchedulerRun {
                    pid: Some(pid),
                    tick: self.tick,
                    reductions_charged,
                    outcome: VmSchedulerOutcome::Ran,
                }
            }
            VmSchedulerDecision::Block { .. } => {
                processes
                    .get_mut(pid)
                    .expect("process was checked before block decision")
                    .block();
                VmSchedulerRun {
                    pid: Some(pid),
                    tick: self.tick,
                    reductions_charged,
                    outcome: VmSchedulerOutcome::Blocked,
                }
            }
            VmSchedulerDecision::Exit { reason, .. } => {
                let cleanup = processes.exit_process(pid, reason).unwrap_or_default();
                self.classes.remove(&pid);
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

#[cfg(test)]
#[path = "scheduler_beam_suite_parity_test.rs"]
mod scheduler_beam_suite_parity_test;

#[cfg(test)]
#[path = "yielding_c_fun_beam_parity_test.rs"]
mod yielding_c_fun_beam_parity_test;

#[cfg(test)]
#[path = "scheduler_result_test.rs"]
mod scheduler_result_test;

#[cfg(test)]
#[path = "scheduler_terminal_accounting_test.rs"]
mod scheduler_terminal_accounting_test;

#[cfg(test)]
#[path = "scheduler_reclassification_accounting_test.rs"]
mod scheduler_reclassification_accounting_test;

#[cfg(test)]
#[path = "scheduler_cancellation_accounting_test.rs"]
mod scheduler_cancellation_accounting_test;
