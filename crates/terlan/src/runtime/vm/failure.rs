#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use super::process::{VmExitReason, VmProcessId, VmProcessState, VmProcessTable};
use super::ReplValue;

/// Stable monitor reference allocated by the VM failure layer.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmMonitorRef(u64);

impl VmMonitorRef {
    /// Returns the numeric monitor reference.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// Result of exiting a process through the failure layer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmFailureReport {
    pub(crate) exited: Vec<VmProcessId>,
    pub(crate) cleanup_handles: Vec<String>,
    pub(crate) delivered_exit_signals: usize,
    pub(crate) delivered_down_messages: usize,
}

/// VM-owned failure, link, and monitor state.
///
/// Inputs:
/// - Link operations, monitor operations, trap-exit flags, and process exits.
///
/// Output:
/// - Deterministic failure propagation and inspectable cleanup reports.
///
/// Transformation:
/// - Replaces implicit OTP failure behavior with explicit Terlan VM state while
///   preserving useful resiliency concepts: links, monitors, and trapped exits.
#[derive(Debug, Default)]
pub(crate) struct VmFailureRuntime {
    links: BTreeSet<(VmProcessId, VmProcessId)>,
    monitors: BTreeMap<VmMonitorRef, (VmProcessId, VmProcessId)>,
    trap_exits: BTreeSet<VmProcessId>,
    next_monitor_ref: u64,
}

impl VmFailureRuntime {
    /// Links two live processes.
    pub(crate) fn link(
        &mut self,
        processes: &VmProcessTable,
        left: VmProcessId,
        right: VmProcessId,
    ) -> Result<(), String> {
        if left == right {
            return Err(format!("cannot link process {} to itself", left.as_u64()));
        }
        ensure_live_process(processes, left, "link")?;
        ensure_live_process(processes, right, "link")?;
        self.links.insert(canonical_link(left, right));
        Ok(())
    }

    /// Removes a link if it exists.
    pub(crate) fn unlink(&mut self, left: VmProcessId, right: VmProcessId) {
        self.links.remove(&canonical_link(left, right));
    }

    /// Returns whether two process ids are linked.
    pub(crate) fn is_linked(&self, left: VmProcessId, right: VmProcessId) -> bool {
        self.links.contains(&canonical_link(left, right))
    }

    /// Enables or disables trap-exit behavior for a live process.
    pub(crate) fn set_trap_exits(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
        enabled: bool,
    ) -> Result<(), String> {
        ensure_live_process(processes, pid, "set trap exits for")?;
        if enabled {
            self.trap_exits.insert(pid);
        } else {
            self.trap_exits.remove(&pid);
        }
        Ok(())
    }

    /// Monitors a target process from a watcher process.
    pub(crate) fn monitor(
        &mut self,
        processes: &VmProcessTable,
        watcher: VmProcessId,
        target: VmProcessId,
    ) -> Result<VmMonitorRef, String> {
        ensure_live_process(processes, watcher, "monitor from")?;
        ensure_live_process(processes, target, "monitor")?;
        self.next_monitor_ref = self.next_monitor_ref.saturating_add(1);
        let monitor_ref = VmMonitorRef(self.next_monitor_ref);
        self.monitors.insert(monitor_ref, (watcher, target));
        Ok(monitor_ref)
    }

    /// Removes a monitor reference.
    pub(crate) fn demonitor(&mut self, monitor_ref: VmMonitorRef) -> bool {
        self.monitors.remove(&monitor_ref).is_some()
    }

    /// Exits a process and applies link/monitor semantics.
    pub(crate) fn exit_process(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        reason: VmExitReason,
    ) -> Result<VmFailureReport, String> {
        self.exit_process_inner(processes, pid, reason)
    }

    fn exit_process_inner(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        reason: VmExitReason,
    ) -> Result<VmFailureReport, String> {
        let Some(process) = processes.get(pid) else {
            return Err(format!("cannot exit missing process {}", pid.as_u64()));
        };
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Ok(VmFailureReport::default());
        }

        let cleanup_handles = processes
            .exit_process(pid, reason.clone())
            .expect("live process checked before failure-layer exit");
        self.trap_exits.remove(&pid);
        let mut report = VmFailureReport {
            exited: vec![pid],
            cleanup_handles,
            delivered_exit_signals: 0,
            delivered_down_messages: 0,
        };

        for (monitor_ref, watcher) in self.take_target_monitors(pid) {
            if send_down_message(processes, pid, watcher, monitor_ref, &reason) {
                report.delivered_down_messages += 1;
            }
        }

        for linked in self.take_links(pid) {
            if !is_live_process(processes, linked) {
                continue;
            }
            if self.trap_exits.contains(&linked) {
                processes
                    .send(pid, linked, exit_signal_message(pid, &reason))
                    .expect("live linked trap-exit process must accept exit signal");
                report.delivered_exit_signals += 1;
                continue;
            }
            if reason != VmExitReason::Normal {
                let child_report = self
                    .exit_process_inner(processes, linked, reason.clone())
                    .expect("live linked process checked before propagated exit");
                report.exited.extend(child_report.exited);
                report.cleanup_handles.extend(child_report.cleanup_handles);
                report.delivered_exit_signals += child_report.delivered_exit_signals;
                report.delivered_down_messages += child_report.delivered_down_messages;
            }
        }

        Ok(report)
    }

    fn take_links(&mut self, pid: VmProcessId) -> Vec<VmProcessId> {
        let peers = self
            .links
            .iter()
            .filter_map(|(left, right)| {
                if *left == pid {
                    Some(*right)
                } else if *right == pid {
                    Some(*left)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        self.links
            .retain(|(left, right)| *left != pid && *right != pid);
        peers
    }

    fn take_target_monitors(&mut self, pid: VmProcessId) -> Vec<(VmMonitorRef, VmProcessId)> {
        let matching = self
            .monitors
            .iter()
            .filter_map(|(monitor_ref, (watcher, target))| {
                (*target == pid).then_some((*monitor_ref, *watcher))
            })
            .collect::<Vec<_>>();
        for (monitor_ref, _) in &matching {
            self.monitors.remove(monitor_ref);
        }
        matching
    }
}

fn canonical_link(left: VmProcessId, right: VmProcessId) -> (VmProcessId, VmProcessId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn ensure_live_process(
    processes: &VmProcessTable,
    pid: VmProcessId,
    action: &str,
) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("cannot {action} missing process {}", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!("cannot {action} exited process {}", pid.as_u64()));
    }
    Ok(())
}

fn is_live_process(processes: &VmProcessTable, pid: VmProcessId) -> bool {
    processes
        .get(pid)
        .is_some_and(|process| !matches!(process.state, VmProcessState::Exited(_)))
}

fn send_down_message(
    processes: &mut VmProcessTable,
    target: VmProcessId,
    watcher: VmProcessId,
    monitor_ref: VmMonitorRef,
    reason: &VmExitReason,
) -> bool {
    if !is_live_process(processes, watcher) {
        return false;
    }
    processes
        .send(
            target,
            watcher,
            ReplValue::Tuple(vec![
                ReplValue::Atom("down".to_string()),
                ReplValue::Int(monitor_ref.as_u64() as i64),
                ReplValue::Int(target.as_u64() as i64),
                reason_value(reason),
            ]),
        )
        .is_ok()
}

fn exit_signal_message(from: VmProcessId, reason: &VmExitReason) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("exit".to_string()),
        ReplValue::Int(from.as_u64() as i64),
        reason_value(reason),
    ])
}

fn reason_value(reason: &VmExitReason) -> ReplValue {
    match reason {
        VmExitReason::Normal => ReplValue::Atom("normal".to_string()),
        VmExitReason::Killed => ReplValue::Atom("killed".to_string()),
        VmExitReason::Error(message) => ReplValue::Tuple(vec![
            ReplValue::Atom("error".to_string()),
            ReplValue::String(message.clone()),
        ]),
    }
}

#[cfg(test)]
#[path = "failure_test.rs"]
mod failure_test;
