use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;

use serde::Serialize;

use super::{VmScheduler, VmSchedulerClass};
use crate::runtime::vm::process::VmProcessId;

/// One deterministic scheduler queue transition retained for replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
pub(crate) struct VmSchedulerQueueTransition {
    pub(crate) tick: u64,
    pub(crate) pid: u64,
    pub(crate) action: &'static str,
    pub(crate) class: VmSchedulerClass,
    pub(crate) queue_len: usize,
}

/// Cumulative scheduler accounting for one VM process.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmSchedulerProcessMetrics {
    pub(crate) pid: u64,
    pub(crate) reductions: u64,
    pub(crate) memory_reductions: u64,
    pub(crate) slices: u64,
    pub(crate) preemptions: u64,
    pub(crate) max_wait_ticks: u64,
    pub(crate) first_run_tick: Option<u64>,
    pub(crate) last_run_tick: Option<u64>,
}

/// Replay-stable scheduler telemetry exposed to diagnostics and release gates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmSchedulerMetrics {
    pub(crate) total_reductions: u64,
    pub(crate) total_memory_reductions: u64,
    pub(crate) total_slices: u64,
    pub(crate) preemptions: u64,
    pub(crate) max_queue_depth: usize,
    pub(crate) processes: BTreeMap<u64, VmSchedulerProcessMetrics>,
    #[cfg(test)]
    queue_transitions: Vec<VmSchedulerQueueTransition>,
    #[cfg(test)]
    omitted_queue_transitions: u64,
}

#[cfg(test)]
const MAX_QUEUE_TRANSITIONS: usize = 4096;

impl VmSchedulerMetrics {
    /// Refuses incomplete replay evidence after the test-only history fills.
    #[cfg(test)]
    pub(crate) fn queue_transitions(&self) -> &[VmSchedulerQueueTransition] {
        assert_eq!(
            self.omitted_queue_transitions, 0,
            "scheduler transition test history exceeded its event budget; trace is incomplete"
        );
        &self.queue_transitions
    }

    #[cfg(test)]
    fn record_queue_transition(&mut self, transition: VmSchedulerQueueTransition) {
        if self.queue_transitions.len() == MAX_QUEUE_TRANSITIONS {
            self.omitted_queue_transitions = self.omitted_queue_transitions.saturating_add(1);
        } else {
            self.queue_transitions.push(transition);
        }
    }

    /// Removing retained records never erases an incomplete-history marker.
    #[cfg(test)]
    pub(super) fn reap_queue_transitions(&mut self, pid: u64) {
        self.queue_transitions
            .retain(|transition| transition.pid != pid);
    }

    #[cfg(not(test))]
    #[inline]
    pub(super) fn reap_queue_transitions(&mut self, _: u64) {}
}

impl VmScheduler {
    #[cfg(test)]
    pub(super) fn record_queue_transition(
        &mut self,
        pid: VmProcessId,
        action: &'static str,
        class: VmSchedulerClass,
    ) {
        self.metrics
            .record_queue_transition(VmSchedulerQueueTransition {
                tick: self.tick,
                pid: pid.as_u64(),
                action,
                class,
                queue_len: self.queued_len(),
            });
    }

    /// Production scheduling retains cumulative counters, not per-event history.
    #[cfg(not(test))]
    #[inline]
    pub(super) fn record_queue_transition(
        &mut self,
        _: VmProcessId,
        _: &'static str,
        _: VmSchedulerClass,
    ) {
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
struct VmSchedulerStarvationWarning {
    pid: u64,
    max_wait_ticks: u64,
    starvation_bound_ticks: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
struct VmSchedulerProcessReport {
    #[serde(flatten)]
    metrics: VmSchedulerProcessMetrics,
    runnable_duration_ticks: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
struct VmSchedulerFairnessReport<'a> {
    schema: &'static str,
    correlation_id: Option<&'a str>,
    scheduler_tick: u64,
    total_reductions: u64,
    total_memory_reductions: u64,
    total_slices: u64,
    preemption_count: u64,
    max_queue_depth: usize,
    process_metrics: Vec<VmSchedulerProcessReport>,
    starvation_warnings: Vec<VmSchedulerStarvationWarning>,
    queue_transitions: &'a [VmSchedulerQueueTransition],
}

impl VmScheduler {
    /// Persists scheduler fairness evidence for release validation.
    #[cfg(test)]
    pub(crate) fn write_fairness_report(
        &self,
        path: &Path,
        starvation_bound_ticks: u64,
        correlation_id: Option<&str>,
    ) -> Result<(), String> {
        let process_metrics = self
            .metrics
            .processes
            .values()
            .cloned()
            .map(|metrics| VmSchedulerProcessReport {
                runnable_duration_ticks: metrics
                    .first_run_tick
                    .zip(metrics.last_run_tick)
                    .map_or(0, |(first, last)| {
                        last.saturating_sub(first).saturating_add(1)
                    }),
                metrics,
            })
            .collect::<Vec<_>>();
        let starvation_warnings = self
            .metrics
            .processes
            .values()
            .filter(|metrics| metrics.max_wait_ticks > starvation_bound_ticks)
            .map(|metrics| VmSchedulerStarvationWarning {
                pid: metrics.pid,
                max_wait_ticks: metrics.max_wait_ticks,
                starvation_bound_ticks,
            })
            .collect::<Vec<_>>();
        let report = VmSchedulerFairnessReport {
            schema: "terlan-vm-scheduler-fairness-report-v1",
            correlation_id,
            scheduler_tick: self.tick,
            total_reductions: self.metrics.total_reductions,
            total_memory_reductions: self.metrics.total_memory_reductions,
            total_slices: self.metrics.total_slices,
            preemption_count: self.metrics.preemptions,
            max_queue_depth: self.metrics.max_queue_depth,
            process_metrics,
            starvation_warnings,
            queue_transitions: self.metrics.queue_transitions(),
        };
        let json = serde_json::to_string_pretty(&report).map_err(|error| {
            format!("failed to serialize VM scheduler fairness report: {error}")
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("failed to create VM scheduler report directory: {error}")
            })?;
        }
        std::fs::write(path, format!("{json}\n"))
            .map_err(|error| format!("failed to write VM scheduler fairness report: {error}"))
    }
}

#[cfg(test)]
#[path = "telemetry_test.rs"]
mod telemetry_test;
