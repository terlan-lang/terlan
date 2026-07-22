#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::process::{
    VmExitReason, VmProcessId, VmProcessInspectionError, VmProcessState, VmProcessTable,
};
use super::reference::{VmReferenceAllocator, VmReferenceId};
use super::ReplValue;

/// Stable monitor reference allocated by the VM failure layer.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmMonitorRef(VmReferenceId);

impl VmMonitorRef {
    /// Returns the numeric monitor reference.
    pub(crate) fn as_u64(&self) -> u64 {
        self.0.as_u64()
    }

    /// Returns the complete distribution-safe reference identity.
    pub(crate) fn reference(&self) -> &VmReferenceId {
        &self.0
    }
}

/// Result of exiting a process through the failure layer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmFailureReport {
    pub(crate) exited: Vec<VmProcessId>,
    pub(crate) cleanup_handles: Vec<String>,
    pub(crate) delivered_exit_signals: usize,
    pub(crate) delivered_down_messages: usize,
    pub(crate) message_recipients: Vec<VmProcessId>,
}

/// One monitor relationship viewed from an inspected process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmFailureMonitorSnapshot {
    pub(crate) monitor_ref: VmMonitorRef,
    pub(crate) peer: VmProcessId,
}

/// Result of establishing or immediately completing one process monitor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmMonitorRegistration {
    pub(crate) monitor_ref: VmMonitorRef,
    pub(crate) completed: bool,
}

/// Read-only link, monitor, and trapped-exit state for one VM process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmFailureProcessSnapshot {
    pub(crate) pid: VmProcessId,
    pub(crate) trap_exits: bool,
    pub(crate) links: Vec<VmProcessId>,
    pub(crate) monitoring: Vec<VmFailureMonitorSnapshot>,
    pub(crate) monitored_by: Vec<VmFailureMonitorSnapshot>,
}

/// Typed failure returned by trap-exit state operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTrapExitError {
    MissingProcess(VmProcessId),
    ExitedProcess(VmProcessId),
}

impl fmt::Display for VmTrapExitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProcess(pid) => {
                write!(
                    formatter,
                    "cannot inspect trap exits for missing process {}",
                    pid.as_u64()
                )
            }
            Self::ExitedProcess(pid) => {
                write!(
                    formatter,
                    "cannot inspect trap exits for exited process {}",
                    pid.as_u64()
                )
            }
        }
    }
}

/// Observable result of changing a process trap-exit setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmTrapExitUpdate {
    pub(crate) previous: bool,
    pub(crate) current: bool,
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
    priority_links: BTreeSet<(VmProcessId, VmProcessId)>,
    monitors: BTreeMap<VmMonitorRef, (VmProcessId, VmProcessId, bool)>,
    trap_exits: BTreeSet<VmProcessId>,
}

impl VmFailureRuntime {
    /// Links two live processes.
    pub(crate) fn link(
        &mut self,
        processes: &VmProcessTable,
        left: VmProcessId,
        right: VmProcessId,
    ) -> Result<(), String> {
        self.link_with_priority(processes, left, right, false)
    }

    /// Links two live processes and controls whether exit messages use the
    /// priority mailbox lane.
    pub(crate) fn link_with_priority(
        &mut self,
        processes: &VmProcessTable,
        left: VmProcessId,
        right: VmProcessId,
        priority: bool,
    ) -> Result<(), String> {
        if left == right {
            return Err(format!("cannot link process {} to itself", left.as_u64()));
        }
        ensure_live_process(processes, left, "link")?;
        ensure_live_process(processes, right, "link")?;
        let link = canonical_link(left, right);
        self.links.insert(link);
        if priority {
            self.priority_links.insert((left, right));
        } else {
            self.priority_links.remove(&(left, right));
        }
        Ok(())
    }

    /// Removes a link if it exists.
    pub(crate) fn unlink(&mut self, left: VmProcessId, right: VmProcessId) {
        let link = canonical_link(left, right);
        self.links.remove(&link);
        self.priority_links.remove(&(left, right));
        self.priority_links.remove(&(right, left));
    }

    /// Returns whether two process ids are linked.
    pub(crate) fn is_linked(&self, left: VmProcessId, right: VmProcessId) -> bool {
        self.links.contains(&canonical_link(left, right))
    }

    /// Returns whether the active link delivers priority exit messages.
    pub(crate) fn is_priority_link(&self, left: VmProcessId, right: VmProcessId) -> bool {
        self.priority_links.contains(&(left, right))
    }

    /// Returns whether a process owns any active priority signal relationship.
    pub(crate) fn has_priority_relationship(&self, pid: VmProcessId) -> bool {
        self.priority_links
            .iter()
            .any(|(receiver, _)| *receiver == pid)
            || self
                .monitors
                .values()
                .any(|(watcher, _, priority)| *watcher == pid && *priority)
    }

    /// Enables or disables trap-exit behavior for a live process.
    pub(crate) fn set_trap_exits(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
        enabled: bool,
    ) -> Result<VmTrapExitUpdate, VmTrapExitError> {
        ensure_trap_exit_process(processes, pid)?;
        let previous = self.trap_exits.contains(&pid);
        if enabled {
            self.trap_exits.insert(pid);
        } else {
            self.trap_exits.remove(&pid);
        }
        Ok(VmTrapExitUpdate {
            previous,
            current: enabled,
        })
    }

    /// Returns whether a live process converts linked exits into messages.
    pub(crate) fn trap_exits(
        &self,
        processes: &VmProcessTable,
        pid: VmProcessId,
    ) -> Result<bool, VmTrapExitError> {
        ensure_trap_exit_process(processes, pid)?;
        Ok(self.trap_exits.contains(&pid))
    }

    /// Returns the number of live trap-exit settings retained by the runtime.
    pub(crate) fn trap_exit_process_count(&self) -> usize {
        self.trap_exits.len()
    }

    /// Monitors a target process from a watcher process.
    pub(crate) fn monitor(
        &mut self,
        references: &mut VmReferenceAllocator,
        processes: &VmProcessTable,
        watcher: VmProcessId,
        target: VmProcessId,
    ) -> Result<VmMonitorRef, String> {
        self.monitor_with_priority(references, processes, watcher, target, false)
    }

    /// Monitors a live target and controls the mailbox lane used by `DOWN`.
    pub(crate) fn monitor_with_priority(
        &mut self,
        references: &mut VmReferenceAllocator,
        processes: &VmProcessTable,
        watcher: VmProcessId,
        target: VmProcessId,
        priority: bool,
    ) -> Result<VmMonitorRef, String> {
        ensure_live_process(processes, watcher, "monitor from")?;
        ensure_live_process(processes, target, "monitor")?;
        let monitor_ref = VmMonitorRef(
            references
                .allocate_reference()
                .map_err(|error| error.to_string())?,
        );
        self.monitors
            .insert(monitor_ref.clone(), (watcher, target, priority));
        Ok(monitor_ref)
    }

    /// Monitors a known process identity, completing immediately when it has exited.
    pub(crate) fn monitor_or_complete(
        &mut self,
        references: &mut VmReferenceAllocator,
        processes: &mut VmProcessTable,
        watcher: VmProcessId,
        target: VmProcessId,
    ) -> Result<VmMonitorRegistration, String> {
        self.monitor_or_complete_with_priority(references, processes, watcher, target, false)
    }

    /// Monitors a known identity with priority `DOWN` delivery.
    pub(crate) fn monitor_or_complete_with_priority(
        &mut self,
        references: &mut VmReferenceAllocator,
        processes: &mut VmProcessTable,
        watcher: VmProcessId,
        target: VmProcessId,
        priority: bool,
    ) -> Result<VmMonitorRegistration, String> {
        ensure_live_process(processes, watcher, "monitor from")?;
        let target_process = processes
            .get(target)
            .ok_or_else(|| format!("cannot monitor missing process {}", target.as_u64()))?;
        let target_exited = matches!(target_process.state, VmProcessState::Exited(_));
        let monitor_ref = VmMonitorRef(
            references
                .allocate_reference()
                .map_err(|error| error.to_string())?,
        );
        if target_exited {
            let delivered = send_down_payload(
                processes,
                target,
                watcher,
                monitor_ref.clone(),
                ReplValue::Atom("noproc".to_string()),
                priority,
            );
            debug_assert!(
                delivered,
                "validated watcher must accept monitor completion"
            );
            return Ok(VmMonitorRegistration {
                monitor_ref,
                completed: true,
            });
        }
        self.monitors
            .insert(monitor_ref.clone(), (watcher, target, priority));
        Ok(VmMonitorRegistration {
            monitor_ref,
            completed: false,
        })
    }

    /// Removes a monitor reference.
    pub(crate) fn demonitor(&mut self, monitor_ref: VmMonitorRef) -> bool {
        self.monitors.remove(&monitor_ref).is_some()
    }

    /// Removes a monitor only when it belongs to the requesting watcher.
    pub(crate) fn demonitor_for(
        &mut self,
        watcher: VmProcessId,
        monitor_ref: VmMonitorRef,
    ) -> Result<bool, String> {
        let Some((owner, _, _)) = self.monitors.get(&monitor_ref) else {
            return Ok(false);
        };
        if *owner != watcher {
            return Err(format!(
                "monitor reference {} belongs to process {}, not process {}",
                monitor_ref.as_u64(),
                owner.as_u64(),
                watcher.as_u64()
            ));
        }
        Ok(self.monitors.remove(&monitor_ref).is_some())
    }

    /// Returns the number of active monitor references.
    pub(crate) fn monitor_count(&self) -> usize {
        self.monitors.len()
    }

    /// Returns deterministic active failure relationships for one process.
    pub(crate) fn snapshot(
        &self,
        processes: &VmProcessTable,
        pid: VmProcessId,
    ) -> Result<VmFailureProcessSnapshot, VmProcessInspectionError> {
        let process = processes.snapshot(pid)?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Ok(empty_failure_snapshot(pid));
        }

        let mut links = self
            .links
            .iter()
            .filter_map(|(left, right)| {
                if *left == pid && is_live_process(processes, *right) {
                    Some(*right)
                } else if *right == pid && is_live_process(processes, *left) {
                    Some(*left)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        links.sort_unstable();

        let monitoring = self
            .monitors
            .iter()
            .filter_map(|(monitor_ref, (watcher, target, _))| {
                (*watcher == pid && is_live_process(processes, *target)).then_some(
                    VmFailureMonitorSnapshot {
                        monitor_ref: monitor_ref.clone(),
                        peer: *target,
                    },
                )
            })
            .collect();
        let monitored_by = self
            .monitors
            .iter()
            .filter_map(|(monitor_ref, (watcher, target, _))| {
                (*target == pid && is_live_process(processes, *watcher)).then_some(
                    VmFailureMonitorSnapshot {
                        monitor_ref: monitor_ref.clone(),
                        peer: *watcher,
                    },
                )
            })
            .collect();

        Ok(VmFailureProcessSnapshot {
            pid,
            trap_exits: self.trap_exits.contains(&pid),
            links,
            monitoring,
            monitored_by,
        })
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
            .expect("process was checked before failure-layer exit");
        self.trap_exits.remove(&pid);
        self.remove_watcher_monitors(pid);
        let mut report = VmFailureReport {
            exited: vec![pid],
            cleanup_handles,
            delivered_exit_signals: 0,
            delivered_down_messages: 0,
            message_recipients: Vec::new(),
        };

        for (monitor_ref, watcher, priority) in self.take_target_monitors(pid) {
            if send_down_message(processes, pid, watcher, monitor_ref, &reason, priority) {
                report.delivered_down_messages += 1;
                report.message_recipients.push(watcher);
            }
        }

        for (linked, priority) in self.take_links(pid) {
            if !is_live_process(processes, linked) {
                continue;
            }
            if self.trap_exits.contains(&linked) {
                let send = if priority {
                    VmProcessTable::send_priority_system_message
                } else {
                    VmProcessTable::send_system_message
                };
                send(processes, pid, linked, exit_signal_message(pid, &reason))
                    .expect("linked process was checked before trapped exit delivery");
                report.delivered_exit_signals += 1;
                report.message_recipients.push(linked);
                continue;
            }
            if reason != VmExitReason::Normal {
                let child_report = self
                    .exit_process_inner(processes, linked, reason.clone())
                    .expect("linked process was checked before recursive exit");
                report.exited.extend(child_report.exited);
                report.cleanup_handles.extend(child_report.cleanup_handles);
                report.delivered_exit_signals += child_report.delivered_exit_signals;
                report.delivered_down_messages += child_report.delivered_down_messages;
                report
                    .message_recipients
                    .extend(child_report.message_recipients);
            }
        }

        Ok(report)
    }

    fn take_links(&mut self, pid: VmProcessId) -> Vec<(VmProcessId, bool)> {
        let peers = self
            .links
            .iter()
            .filter_map(|(left, right)| {
                if *left == pid {
                    Some((*right, self.priority_links.contains(&(*right, *left))))
                } else if *right == pid {
                    Some((*left, self.priority_links.contains(&(*left, *right))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        self.links
            .retain(|(left, right)| *left != pid && *right != pid);
        self.priority_links
            .retain(|(left, right)| *left != pid && *right != pid);
        peers
    }

    fn take_target_monitors(&mut self, pid: VmProcessId) -> Vec<(VmMonitorRef, VmProcessId, bool)> {
        let matching = self
            .monitors
            .iter()
            .filter_map(|(monitor_ref, (watcher, target, priority))| {
                (*target == pid).then_some((monitor_ref.clone(), *watcher, *priority))
            })
            .collect::<Vec<_>>();
        for (monitor_ref, _, _) in &matching {
            self.monitors.remove(monitor_ref);
        }
        matching
    }

    fn remove_watcher_monitors(&mut self, pid: VmProcessId) {
        self.monitors.retain(|_, (watcher, _, _)| *watcher != pid);
    }
}

fn empty_failure_snapshot(pid: VmProcessId) -> VmFailureProcessSnapshot {
    VmFailureProcessSnapshot {
        pid,
        trap_exits: false,
        links: Vec::new(),
        monitoring: Vec::new(),
        monitored_by: Vec::new(),
    }
}

fn ensure_trap_exit_process(
    processes: &VmProcessTable,
    pid: VmProcessId,
) -> Result<(), VmTrapExitError> {
    let Some(process) = processes.get(pid) else {
        return Err(VmTrapExitError::MissingProcess(pid));
    };
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(VmTrapExitError::ExitedProcess(pid));
    }
    Ok(())
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
    priority: bool,
) -> bool {
    send_down_payload(
        processes,
        target,
        watcher,
        monitor_ref,
        reason_value(reason),
        priority,
    )
}

fn send_down_payload(
    processes: &mut VmProcessTable,
    target: VmProcessId,
    watcher: VmProcessId,
    monitor_ref: VmMonitorRef,
    reason: ReplValue,
    priority: bool,
) -> bool {
    if !is_live_process(processes, watcher) {
        return false;
    }
    let send = if priority {
        VmProcessTable::send_priority_system_message
    } else {
        VmProcessTable::send_system_message
    };
    send(
        processes,
        target,
        watcher,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(target.as_u64() as i64),
            reason,
        ]),
    )
    .is_ok()
}

/// Returns whether one mailbox value is the completion for a monitor.
pub(crate) fn is_monitor_down_message(payload: &ReplValue, monitor_ref: &VmMonitorRef) -> bool {
    matches!(
        payload,
        ReplValue::Tuple(fields)
            if matches!(fields.as_slice(), [
                ReplValue::Atom(kind),
                ReplValue::Int(reference),
                ReplValue::Int(_),
                _,
            ] if kind == "down" && *reference == monitor_ref.as_u64() as i64)
    )
}

pub(crate) fn exit_signal_message(from: VmProcessId, reason: &VmExitReason) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("exit".to_string()),
        ReplValue::Int(from.as_u64() as i64),
        reason_value(reason),
    ])
}

pub(crate) fn reason_value(reason: &VmExitReason) -> ReplValue {
    match reason {
        VmExitReason::Normal => ReplValue::Atom("normal".to_string()),
        VmExitReason::Killed => ReplValue::Atom("killed".to_string()),
        VmExitReason::Error(message) => ReplValue::Tuple(vec![
            ReplValue::Atom("error".to_string()),
            ReplValue::String(message.clone()),
        ]),
        VmExitReason::ShutdownTimeout { timeout_ms } => ReplValue::Tuple(vec![
            ReplValue::Atom("shutdown_timeout".to_string()),
            ReplValue::Int((*timeout_ms).try_into().unwrap_or(i64::MAX)),
        ]),
        VmExitReason::MemoryLimitExceeded {
            requested_bytes,
            previous_bytes,
            projected_bytes,
        } => ReplValue::Tuple(vec![
            ReplValue::Atom("memory_limit_exceeded".to_string()),
            ReplValue::Int((*requested_bytes).try_into().unwrap_or(i64::MAX)),
            ReplValue::Int((*previous_bytes).try_into().unwrap_or(i64::MAX)),
            ReplValue::Int((*projected_bytes).try_into().unwrap_or(i64::MAX)),
        ]),
    }
}

#[cfg(test)]
#[path = "failure_test.rs"]
mod failure_test;

#[cfg(test)]
#[path = "failure_inspection_test.rs"]
mod failure_inspection_test;

#[cfg(test)]
#[path = "failure_erl_link_parity_test.rs"]
mod failure_erl_link_parity_test;

#[cfg(test)]
#[path = "failure_reason_test.rs"]
mod failure_reason_test;
