#![allow(dead_code)]

use super::process::VmProcessTable;
use super::scheduler::VmScheduler;
use super::timer::VmTimerTable;

/// Static capacity metadata for one Terlan VM runtime instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmRuntimeEnvironmentProfile {
    process_limit: usize,
    scheduler_count: usize,
}

impl VmRuntimeEnvironmentProfile {
    /// Creates a profile with explicit non-zero runtime capacities.
    pub(crate) fn new(process_limit: usize, scheduler_count: usize) -> Result<Self, String> {
        if process_limit == 0 {
            return Err("VM process limit must be non-zero".to_string());
        }
        if scheduler_count == 0 {
            return Err("VM scheduler count must be non-zero".to_string());
        }
        Ok(Self {
            process_limit,
            scheduler_count,
        })
    }
}

/// Immutable VM-owned runtime metrics exposed to diagnostics and Terlan code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmRuntimeEnvironmentSnapshot {
    pub(crate) process_limit: usize,
    pub(crate) scheduler_count: usize,
    pub(crate) word_size_bytes: usize,
    pub(crate) total_processes: usize,
    pub(crate) live_processes: usize,
    pub(crate) exited_processes: usize,
    pub(crate) run_queue: usize,
    pub(crate) mailbox_messages: usize,
    pub(crate) logical_heap_bytes: usize,
    pub(crate) resource_handles: usize,
    pub(crate) active_timers: usize,
    pub(crate) timers_started: u64,
    pub(crate) timers_fired: u64,
    pub(crate) timers_cancelled: u64,
    pub(crate) total_reductions: u64,
    pub(crate) memory_reductions: u64,
    pub(crate) scheduler_slices: u64,
    pub(crate) scheduler_preemptions: u64,
}

impl VmRuntimeEnvironmentSnapshot {
    /// Captures one deterministic runtime view without mutating its owners.
    pub(crate) fn capture(
        profile: VmRuntimeEnvironmentProfile,
        processes: &VmProcessTable,
        scheduler: &VmScheduler,
        timers: &VmTimerTable,
    ) -> Result<Self, String> {
        let process_metrics = processes.metrics();
        if process_metrics.live_processes > profile.process_limit {
            return Err(format!(
                "VM live process count {} exceeds configured limit {}",
                process_metrics.live_processes, profile.process_limit
            ));
        }
        let scheduler_metrics = scheduler.metrics();
        let timer_metrics = timers.metrics();
        Ok(Self {
            process_limit: profile.process_limit,
            scheduler_count: profile.scheduler_count,
            word_size_bytes: std::mem::size_of::<usize>(),
            total_processes: process_metrics.total_processes,
            live_processes: process_metrics.live_processes,
            exited_processes: process_metrics.exited_processes,
            run_queue: scheduler.queued_len(),
            mailbox_messages: process_metrics.mailbox_messages,
            logical_heap_bytes: process_metrics.heap_bytes,
            resource_handles: process_metrics.resource_handles,
            active_timers: timers.active_count(),
            timers_started: timer_metrics.started,
            timers_fired: timer_metrics.fired,
            timers_cancelled: timer_metrics.cancelled,
            total_reductions: scheduler_metrics.total_reductions,
            memory_reductions: scheduler_metrics.total_memory_reductions,
            scheduler_slices: scheduler_metrics.total_slices,
            scheduler_preemptions: scheduler_metrics.preemptions,
        })
    }
}

#[cfg(test)]
#[path = "process_environment_test.rs"]
mod process_environment_test;
