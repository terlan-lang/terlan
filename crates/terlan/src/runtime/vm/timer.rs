#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};
use super::scheduler::VmScheduler;

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTimerKind {
    OneShot,
    ReceiveTimeout,
}

/// Event emitted when a timer changes state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTimerEvent {
    Fired {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
    },
    Cancelled {
        timer_id: VmTimerId,
        owner: VmProcessId,
        kind: VmTimerKind,
    },
}

/// Read-only timer row for runtime inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmTimerSnapshot {
    pub(crate) id: VmTimerId,
    pub(crate) owner: VmProcessId,
    pub(crate) deadline_tick: u64,
    pub(crate) kind: VmTimerKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VmTimer {
    id: VmTimerId,
    owner: VmProcessId,
    deadline_tick: u64,
    kind: VmTimerKind,
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
}

impl VmTimerTable {
    /// Starts a one-shot timer for a live process.
    pub(crate) fn start_one_shot(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        deadline_tick: u64,
    ) -> Result<VmTimerId, String> {
        require_live_process(processes, owner)?;
        Ok(self.insert_timer(owner, deadline_tick, VmTimerKind::OneShot))
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
        processes
            .get_mut(owner)
            .expect("owner process was checked before blocking receive timeout")
            .block();
        Ok(self.insert_timer(
            owner,
            now_tick.saturating_add(timeout_ticks),
            VmTimerKind::ReceiveTimeout,
        ))
    }

    /// Cancels an active timer.
    pub(crate) fn cancel(&mut self, timer_id: VmTimerId) -> Result<VmTimerEvent, String> {
        let timer = self
            .timers
            .remove(&timer_id)
            .ok_or_else(|| format!("missing timer {}", timer_id.as_u64()))?;
        Ok(VmTimerEvent::Cancelled {
            timer_id,
            owner: timer.owner,
            kind: timer.kind,
        })
    }

    /// Fires all due timers and wakes receive-timeout owners.
    pub(crate) fn advance_clock(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        now_tick: u64,
    ) -> Vec<VmTimerEvent> {
        let due: Vec<VmTimer> = self
            .timers
            .values()
            .copied()
            .filter(|timer| timer.deadline_tick <= now_tick)
            .collect();
        let mut events = Vec::new();
        for timer in due {
            self.timers.remove(&timer.id);
            if timer.kind == VmTimerKind::ReceiveTimeout {
                let _ = scheduler.wake_process(processes, timer.owner);
            }
            events.push(VmTimerEvent::Fired {
                timer_id: timer.id,
                owner: timer.owner,
                kind: timer.kind,
            });
        }
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

    fn insert_timer(
        &mut self,
        owner: VmProcessId,
        deadline_tick: u64,
        kind: VmTimerKind,
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
            },
        );
        id
    }
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
