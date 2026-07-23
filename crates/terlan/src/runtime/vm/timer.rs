#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};
use super::scheduler::VmScheduler;
use super::ReplValue;

#[path = "timer/transfer.rs"]
mod transfer;

#[allow(unused_imports)] // Public to staged MC-5 tests before migration orchestration lands.
pub(crate) use transfer::{VmTimerImportFailure, VmTimerTransfer};

const TIMER_MAILBOX_DELIVERY_REDUCTIONS: u64 = 1;

/// VM-owned timer identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmTimerId(u64);

impl VmTimerId {
    /// Returns the numeric timer id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// VM timer kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VmTimerKind {
    OneShot,
    Interval,
    ReceiveTimeout,
}

impl VmTimerKind {
    /// Returns the stable runtime/inspection label for this timer kind.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::OneShot => "one_shot",
            Self::Interval => "interval",
            Self::ReceiveTimeout => "receive_timeout",
        }
    }
}

/// Event emitted when a timer changes state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTimerEvent {
    Fired {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
    },
    DeadlineMissed {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
        late_by_ticks: u64,
    },
    Coalesced {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
        skipped_intervals: u64,
        next_deadline_tick: u64,
    },
    Overflow {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
    },
    Cancelled {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
    },
    OwnerExited {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
    },
}

impl VmTimerEvent {
    /// Returns the stable timer identity carried by this event.
    pub(crate) const fn timer_id(self) -> VmTimerId {
        match self {
            Self::Fired { timer_id, .. }
            | Self::DeadlineMissed { timer_id, .. }
            | Self::Coalesced { timer_id, .. }
            | Self::Overflow { timer_id, .. }
            | Self::Cancelled { timer_id, .. }
            | Self::OwnerExited { timer_id, .. } => timer_id,
        }
    }

    /// Returns the process that owns this timer event.
    pub(crate) const fn owner(self) -> VmProcessId {
        match self {
            Self::Fired { owner, .. }
            | Self::DeadlineMissed { owner, .. }
            | Self::Coalesced { owner, .. }
            | Self::Overflow { owner, .. }
            | Self::Cancelled { owner, .. }
            | Self::OwnerExited { owner, .. } => owner,
        }
    }

    /// Returns the timer kind carried by this event.
    pub(crate) const fn kind(self) -> VmTimerKind {
        match self {
            Self::Fired { kind, .. }
            | Self::DeadlineMissed { kind, .. }
            | Self::Coalesced { kind, .. }
            | Self::Overflow { kind, .. }
            | Self::Cancelled { kind, .. }
            | Self::OwnerExited { kind, .. } => kind,
        }
    }
}

/// Read-only timer row for runtime inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmTimerSnapshot {
    pub(crate) id: VmTimerId,
    pub(crate) owner: VmProcessId,
    pub(crate) deadline_tick: u64,
    pub(crate) kind: VmTimerKind,
}

/// Owner-bound authority to cancel one active timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmTimerCancellationToken {
    timer_id: VmTimerId,
    owner: VmProcessId,
}

/// Rejected non-monotonic clock observation retained for diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmTimerClockDrift {
    pub(crate) previous_tick: u64,
    pub(crate) observed_tick: u64,
    pub(crate) diagnostic: String,
}

/// Runtime accounting captured by the VM-owned timer table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmTimerMetrics {
    pub(crate) started: u64,
    pub(crate) fired: u64,
    pub(crate) deadline_missed: u64,
    pub(crate) coalesced: u64,
    pub(crate) overflow: u64,
    pub(crate) cancelled: u64,
    pub(crate) owner_exited: u64,
    pub(crate) max_active: usize,
    pub(crate) late_by_ticks_total: u64,
    pub(crate) ordering_trace: Vec<u64>,
    pub(crate) cancellation_decisions: Vec<VmTimerCancellationDecision>,
    pub(crate) clock_drift_rejections: Vec<VmTimerClockDrift>,
    pub(crate) mailbox_deliveries: u64,
}

/// Typed timer cancellation evidence retained for observability and replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct VmTimerCancellationDecision {
    pub(crate) timer_id: u64,
    pub(crate) owner: u64,
    pub(crate) kind: VmTimerKind,
    pub(crate) outcome: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmTimerCounts {
    started: u64,
    fired: u64,
    deadline_missed: u64,
    coalesced: u64,
    overflow: u64,
    cancelled: u64,
    owner_exited: u64,
    active: usize,
    max_active: usize,
    mailbox_deliveries: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmTimerSchedulerPressure {
    max_consecutive_timer_wakeups: usize,
    fairness_interleaves: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmTimerDeadlineReport<'a> {
    schema: &'static str,
    timer_counts: VmTimerCounts,
    ordering_trace: &'a [u64],
    cancellation_decisions: &'a [VmTimerCancellationDecision],
    clock_drift_rejections: &'a [VmTimerClockDrift],
    late_fire_count: u64,
    late_by_ticks_total: u64,
    scheduler_pressure_deltas: VmTimerSchedulerPressure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VmTimer {
    id: VmTimerId,
    owner: VmProcessId,
    deadline_tick: u64,
    kind: VmTimerKind,
    interval_ticks: Option<u64>,
}

/// VM-owned timer table.
///
/// Inputs:
/// - Timer starts, cancellation requests, process table state, scheduler
///   wakeups, and clock ticks.
///
/// Output:
/// - Fired/cancelled timer events and inspection-visible timer rows.
///
/// Transformation:
/// - Owns timer behavior in Terlan VM terms rather than delegating one-shot
///   timers, receive timeouts, or wakeup inspection to a host runtime.
#[derive(Debug, Default)]
pub(crate) struct VmTimerTable {
    next_timer_id: u64,
    timers: BTreeMap<VmTimerId, VmTimer>,
    metrics: VmTimerMetrics,
    last_clock_tick: Option<u64>,
}

impl VmTimerTable {
    /// Returns the latest accepted clock tick, or zero before the first advance.
    pub(crate) fn current_tick(&self) -> u64 {
        self.last_clock_tick.unwrap_or(0)
    }

    /// Starts a one-shot timer for a live process.
    pub(crate) fn start_one_shot(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        deadline_tick: u64,
    ) -> Result<VmTimerId, String> {
        require_live_process(processes, owner)?;
        Ok(self.insert_timer(owner, deadline_tick, VmTimerKind::OneShot, None))
    }

    /// Starts an interval timer for a live process.
    pub(crate) fn start_interval(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        first_deadline_tick: u64,
        interval_ticks: u64,
    ) -> Result<VmTimerId, String> {
        require_live_process(processes, owner)?;
        if interval_ticks == 0 {
            return Err(format!(
                "interval timer for process {} must have a positive interval",
                owner.as_u64()
            ));
        }
        Ok(self.insert_timer(
            owner,
            first_deadline_tick,
            VmTimerKind::Interval,
            Some(interval_ticks),
        ))
    }

    /// Starts a receive-timeout timer and blocks the receiving process.
    pub(crate) fn start_receive_timeout(
        &mut self,
        processes: &mut VmProcessTable,
        _scheduler: &mut VmScheduler,
        owner: VmProcessId,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmTimerId, String> {
        require_live_process(processes, owner)?;
        let deadline_tick = now_tick
            .checked_add(timeout_ticks)
            .ok_or_else(|| format!("timer deadline overflow for process {}", owner.as_u64()))?;
        processes
            .with_process_control_mutator(owner, |process| process.block())
            .expect("owner process was checked before blocking receive timeout");
        Ok(self.insert_timer(owner, deadline_tick, VmTimerKind::ReceiveTimeout, None))
    }

    /// Cancels an active timer.
    pub(crate) fn cancel(&mut self, timer_id: VmTimerId) -> Result<VmTimerEvent, String> {
        let timer = self
            .timers
            .remove(&timer_id)
            .ok_or_else(|| format!("missing timer {}", timer_id.as_u64()))?;
        let event = VmTimerEvent::Cancelled {
            timer_id,
            owner: timer.owner,
            kind: timer.kind,
        };
        self.record_event(&event);
        Ok(event)
    }

    /// Returns an owner-bound cancellation token for an active timer.
    pub(crate) fn cancellation_token(
        &self,
        timer_id: VmTimerId,
    ) -> Result<VmTimerCancellationToken, String> {
        let timer = self
            .timers
            .get(&timer_id)
            .ok_or_else(|| format!("missing timer {}", timer_id.as_u64()))?;
        Ok(VmTimerCancellationToken {
            timer_id,
            owner: timer.owner,
        })
    }

    /// Returns the number of ticks before an active timer reaches its deadline.
    pub(crate) fn remaining_ticks(
        &self,
        timer_id: VmTimerId,
        now_tick: u64,
    ) -> Result<u64, String> {
        let timer = self
            .timers
            .get(&timer_id)
            .ok_or_else(|| format!("missing timer {}", timer_id.as_u64()))?;
        Ok(timer.deadline_tick.saturating_sub(now_tick))
    }

    /// Cancels a timer only when the token still names its current owner.
    pub(crate) fn cancel_with_token(
        &mut self,
        token: VmTimerCancellationToken,
    ) -> Result<VmTimerEvent, String> {
        let timer = self
            .timers
            .get(&token.timer_id)
            .ok_or_else(|| format!("missing timer {}", token.timer_id.as_u64()))?;
        if timer.owner != token.owner {
            return Err(format!(
                "timer {} cancellation token owner mismatch: expected {}, observed {}",
                token.timer_id.as_u64(),
                timer.owner.as_u64(),
                token.owner.as_u64()
            ));
        }
        self.cancel(token.timer_id)
    }

    /// Cleans up all active timers owned by an exited process.
    pub(crate) fn cancel_owner_timers(&mut self, owner: VmProcessId) -> Vec<VmTimerEvent> {
        let owned_timer_ids = self
            .timers
            .values()
            .filter_map(|timer| (timer.owner == owner).then_some(timer.id))
            .collect::<Vec<_>>();
        let mut events = Vec::with_capacity(owned_timer_ids.len());
        for timer_id in owned_timer_ids {
            let timer = self
                .timers
                .remove(&timer_id)
                .expect("timer id was collected from active timer table");
            events.push(VmTimerEvent::OwnerExited {
                timer_id,
                owner: timer.owner,
                kind: timer.kind,
            });
        }
        self.record_events(&events);
        events
    }

    /// Fires all due timers and wakes receive-timeout owners.
    pub(crate) fn advance_clock(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        now_tick: u64,
    ) -> Vec<VmTimerEvent> {
        if let Some(previous_tick) = self.last_clock_tick {
            if now_tick < previous_tick {
                self.metrics.clock_drift_rejections.push(VmTimerClockDrift {
                    previous_tick,
                    observed_tick: now_tick,
                    diagnostic: format!(
                        "timer clock moved backwards: previous tick {previous_tick}, observed tick {now_tick}"
                    ),
                });
                return Vec::new();
            }
        }
        self.last_clock_tick = Some(now_tick);
        let due: Vec<VmTimer> = self
            .timers
            .values()
            .copied()
            .filter(|timer| timer.deadline_tick <= now_tick)
            .collect();
        let mut events = Vec::new();
        for timer in due {
            self.timers.remove(&timer.id);
            if timer_owner_exited(processes, timer.owner) {
                events.push(VmTimerEvent::OwnerExited {
                    timer_id: timer.id,
                    owner: timer.owner,
                    kind: timer.kind,
                });
                continue;
            }
            if let Some(event) = self.coalesce_late_interval(timer, now_tick) {
                events.push(event);
                continue;
            }
            if now_tick > timer.deadline_tick {
                if timer.kind == VmTimerKind::ReceiveTimeout {
                    let _ = scheduler.wake_process(processes, timer.owner);
                }
                if let Some(event) = self.reschedule_interval(timer) {
                    events.push(event);
                    continue;
                }
                events.push(VmTimerEvent::DeadlineMissed {
                    timer_id: timer.id,
                    owner: timer.owner,
                    kind: timer.kind,
                    late_by_ticks: now_tick - timer.deadline_tick,
                });
                continue;
            }
            if timer.kind == VmTimerKind::ReceiveTimeout {
                let _ = scheduler.wake_process(processes, timer.owner);
            }
            if let Some(event) = self.reschedule_interval(timer) {
                events.push(event);
                continue;
            }
            events.push(VmTimerEvent::Fired {
                timer_id: timer.id,
                owner: timer.owner,
                kind: timer.kind,
            });
        }
        self.record_events(&events);
        events
    }

    /// Returns inspection rows for active timers.
    pub(crate) fn snapshots(&self) -> Vec<VmTimerSnapshot> {
        self.timers
            .values()
            .map(|timer| VmTimerSnapshot {
                id: timer.id,
                owner: timer.owner,
                deadline_tick: timer.deadline_tick,
                kind: timer.kind,
            })
            .collect()
    }

    /// Returns the number of active timer identities without allocating snapshots.
    pub(crate) fn active_count(&self) -> usize {
        self.timers.len()
    }

    /// Returns cumulative timer accounting for debugger and inspector surfaces.
    pub(crate) fn metrics(&self) -> &VmTimerMetrics {
        &self.metrics
    }

    /// Persists deterministic timer/deadline evidence for release validation.
    pub(crate) fn write_deadline_report(
        &self,
        path: &Path,
        max_consecutive_timer_wakeups: usize,
        fairness_interleaves: usize,
    ) -> Result<(), String> {
        let report = VmTimerDeadlineReport {
            schema: "terlan-vm-timer-deadline-report-v1",
            timer_counts: VmTimerCounts {
                started: self.metrics.started,
                fired: self.metrics.fired,
                deadline_missed: self.metrics.deadline_missed,
                coalesced: self.metrics.coalesced,
                overflow: self.metrics.overflow,
                cancelled: self.metrics.cancelled,
                owner_exited: self.metrics.owner_exited,
                active: self.timers.len(),
                max_active: self.metrics.max_active,
                mailbox_deliveries: self.metrics.mailbox_deliveries,
            },
            ordering_trace: &self.metrics.ordering_trace,
            cancellation_decisions: &self.metrics.cancellation_decisions,
            clock_drift_rejections: &self.metrics.clock_drift_rejections,
            late_fire_count: self.metrics.deadline_missed,
            late_by_ticks_total: self.metrics.late_by_ticks_total,
            scheduler_pressure_deltas: VmTimerSchedulerPressure {
                max_consecutive_timer_wakeups,
                fairness_interleaves,
            },
        };
        let json = serde_json::to_string_pretty(&report)
            .expect("VM timer report contains only JSON-serializable values");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create VM timer report directory: {error}"))?;
        }
        std::fs::write(path, format!("{json}\n"))
            .map_err(|error| format!("failed to write VM timer deadline report: {error}"))
    }

    /// Delivers one typed timer outcome to its live owner's mailbox.
    pub(crate) fn deliver_event_to_mailbox(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        event: &VmTimerEvent,
    ) -> Result<Option<u64>, String> {
        let owner = timer_event_owner(event);
        if matches!(event, VmTimerEvent::OwnerExited { .. }) {
            return Ok(None);
        }
        let message_id = processes.send(owner, owner, timer_event_mailbox_value(event))?;
        scheduler.charge_runtime_reductions(processes, owner, TIMER_MAILBOX_DELIVERY_REDUCTIONS)?;
        scheduler.wake_process(processes, owner)?;
        self.metrics.mailbox_deliveries = self.metrics.mailbox_deliveries.saturating_add(1);
        Ok(Some(message_id))
    }

    fn insert_timer(
        &mut self,
        owner: VmProcessId,
        deadline_tick: u64,
        kind: VmTimerKind,
        interval_ticks: Option<u64>,
    ) -> VmTimerId {
        self.next_timer_id = self.next_timer_id.saturating_add(1);
        let id = VmTimerId(self.next_timer_id);
        self.timers.insert(
            id,
            VmTimer {
                id,
                owner,
                deadline_tick,
                kind,
                interval_ticks,
            },
        );
        self.metrics.started = self.metrics.started.saturating_add(1);
        self.metrics.max_active = self.metrics.max_active.max(self.timers.len());
        id
    }

    fn record_events(&mut self, events: &[VmTimerEvent]) {
        for event in events {
            self.record_event(event);
        }
    }

    fn record_event(&mut self, event: &VmTimerEvent) {
        let (timer_id, owner, kind) = match *event {
            VmTimerEvent::Fired {
                timer_id,
                owner,
                kind,
            }
            | VmTimerEvent::DeadlineMissed {
                timer_id,
                owner,
                kind,
                ..
            }
            | VmTimerEvent::Coalesced {
                timer_id,
                owner,
                kind,
                ..
            }
            | VmTimerEvent::Overflow {
                timer_id,
                owner,
                kind,
            }
            | VmTimerEvent::Cancelled {
                timer_id,
                owner,
                kind,
            }
            | VmTimerEvent::OwnerExited {
                timer_id,
                owner,
                kind,
            } => (timer_id, owner, kind),
        };
        match *event {
            VmTimerEvent::Fired { .. } => self.metrics.fired += 1,
            VmTimerEvent::DeadlineMissed { late_by_ticks, .. } => {
                self.metrics.deadline_missed += 1;
                self.metrics.late_by_ticks_total = self
                    .metrics
                    .late_by_ticks_total
                    .saturating_add(late_by_ticks);
            }
            VmTimerEvent::Coalesced { .. } => self.metrics.coalesced += 1,
            VmTimerEvent::Overflow { .. } => self.metrics.overflow += 1,
            VmTimerEvent::Cancelled { .. } => self.metrics.cancelled += 1,
            VmTimerEvent::OwnerExited { .. } => self.metrics.owner_exited += 1,
        }
        self.metrics.ordering_trace.push(timer_id.as_u64());
        let outcome = match event {
            VmTimerEvent::Cancelled { .. } => Some("cancelled"),
            VmTimerEvent::OwnerExited { .. } => Some("owner_exited"),
            _ => None,
        };
        if let Some(outcome) = outcome {
            self.metrics
                .cancellation_decisions
                .push(VmTimerCancellationDecision {
                    timer_id: timer_id.as_u64(),
                    owner: owner.as_u64(),
                    kind,
                    outcome,
                });
        }
    }

    fn reschedule_interval(&mut self, timer: VmTimer) -> Option<VmTimerEvent> {
        let interval_ticks = timer.interval_ticks?;
        let Some(deadline_tick) = timer.deadline_tick.checked_add(interval_ticks) else {
            return Some(VmTimerEvent::Overflow {
                timer_id: timer.id,
                owner: timer.owner,
                kind: timer.kind,
            });
        };
        self.timers.insert(
            timer.id,
            VmTimer {
                deadline_tick,
                ..timer
            },
        );
        None
    }

    fn coalesce_late_interval(&mut self, timer: VmTimer, now_tick: u64) -> Option<VmTimerEvent> {
        if timer.kind != VmTimerKind::Interval || now_tick <= timer.deadline_tick {
            return None;
        }
        let interval_ticks = timer.interval_ticks?;
        let skipped_intervals = (now_tick - timer.deadline_tick) / interval_ticks;
        if skipped_intervals == 0 {
            return None;
        }
        let Some(next_step_count) = skipped_intervals.checked_add(1) else {
            return Some(interval_overflow_event(timer));
        };
        let Some(next_offset) = interval_ticks.checked_mul(next_step_count) else {
            return Some(interval_overflow_event(timer));
        };
        let Some(next_deadline_tick) = timer.deadline_tick.checked_add(next_offset) else {
            return Some(interval_overflow_event(timer));
        };
        self.timers.insert(
            timer.id,
            VmTimer {
                deadline_tick: next_deadline_tick,
                ..timer
            },
        );
        Some(VmTimerEvent::Coalesced {
            timer_id: timer.id,
            owner: timer.owner,
            kind: timer.kind,
            skipped_intervals,
            next_deadline_tick,
        })
    }
}

fn timer_event_owner(event: &VmTimerEvent) -> VmProcessId {
    event.owner()
}

fn timer_event_mailbox_value(event: &VmTimerEvent) -> ReplValue {
    let (timer_id, kind, outcome, detail) = match *event {
        VmTimerEvent::Fired { timer_id, kind, .. } => (timer_id, kind, "fired", ReplValue::Unit),
        VmTimerEvent::DeadlineMissed {
            timer_id,
            kind,
            late_by_ticks,
            ..
        } => (
            timer_id,
            kind,
            "deadline_missed",
            ReplValue::String(late_by_ticks.to_string()),
        ),
        VmTimerEvent::Coalesced {
            timer_id,
            kind,
            skipped_intervals,
            next_deadline_tick,
            ..
        } => (
            timer_id,
            kind,
            "coalesced",
            ReplValue::Tuple(vec![
                ReplValue::String(skipped_intervals.to_string()),
                ReplValue::String(next_deadline_tick.to_string()),
            ]),
        ),
        VmTimerEvent::Overflow { timer_id, kind, .. } => {
            (timer_id, kind, "overflow", ReplValue::Unit)
        }
        VmTimerEvent::Cancelled { timer_id, kind, .. } => {
            (timer_id, kind, "cancelled", ReplValue::Unit)
        }
        VmTimerEvent::OwnerExited { timer_id, kind, .. } => {
            (timer_id, kind, "owner_exited", ReplValue::Unit)
        }
    };
    ReplValue::Tuple(vec![
        ReplValue::Atom("timer_outcome".to_string()),
        ReplValue::String(timer_id.as_u64().to_string()),
        ReplValue::Atom(kind.as_str().to_string()),
        ReplValue::Atom(outcome.to_string()),
        detail,
    ])
}

fn interval_overflow_event(timer: VmTimer) -> VmTimerEvent {
    VmTimerEvent::Overflow {
        timer_id: timer.id,
        owner: timer.owner,
        kind: timer.kind,
    }
}

fn timer_owner_exited(processes: &VmProcessTable, pid: VmProcessId) -> bool {
    processes
        .get(pid)
        .map(|process| matches!(process.state, VmProcessState::Exited(_)))
        .unwrap_or(true)
}

fn require_live_process(processes: &VmProcessTable, pid: VmProcessId) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("missing process {}", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!("process {} has exited", pid.as_u64()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "timer_test.rs"]
mod timer_test;

#[cfg(test)]
#[path = "timer_accounting_test.rs"]
mod timer_accounting_test;

#[cfg(test)]
#[path = "timer_load_parity_test.rs"]
mod timer_load_parity_test;

#[cfg(test)]
#[path = "long_timer_parity_test.rs"]
mod long_timer_parity_test;

#[cfg(test)]
#[path = "timer_transfer_test.rs"]
mod timer_transfer_test;
