use std::collections::BTreeMap;

use super::{VmSupervisionRestart, VmSupervisionSystem, VmSupervisorId};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::VmScheduler,
    timer::{VmTimerEvent, VmTimerId, VmTimerTable},
    ReplValue,
};

const TIMER_OWNER_MODULE: &str = "runtime.SupervisionShutdown";

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmPendingSupervisionShutdown {
    supervisor_id: VmSupervisorId,
    child_id: String,
    pid: VmProcessId,
    timeout_ms: u64,
}

/// Inspection-visible graceful-shutdown deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmScheduledSupervisionShutdown {
    pub(crate) timer_id: VmTimerId,
    pub(crate) child_id: String,
    pub(crate) pid: VmProcessId,
    pub(crate) deadline_tick: u64,
    pub(crate) timeout_ms: u64,
}

/// Result of requesting supervised child shutdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionShutdownStart {
    Immediate(VmSupervisionRestart),
    Waiting(VmScheduledSupervisionShutdown),
}

/// Terminal result for one supervised child shutdown deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionShutdownCompletion {
    Exited {
        timer_id: VmTimerId,
        reason: VmExitReason,
        restart: VmSupervisionRestart,
    },
    TimedOut {
        timer_id: VmTimerId,
        pid: VmProcessId,
        timeout_ms: u64,
        restart: VmSupervisionRestart,
    },
    Cancelled {
        timer_id: VmTimerId,
        pid: VmProcessId,
    },
    TimerOwnerExited {
        timer_id: VmTimerId,
        pid: VmProcessId,
    },
    Stale {
        timer_id: VmTimerId,
        expected_pid: VmProcessId,
        current_pid: VmProcessId,
    },
}

/// Results from one scheduler-facing supervision shutdown clock advance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisionShutdownAdvance {
    pub(crate) timer_events: Vec<VmTimerEvent>,
    pub(crate) completions: Vec<VmSupervisionShutdownCompletion>,
    pub(crate) unhandled_timer_events: Vec<VmTimerEvent>,
}

/// VM-timer-backed graceful shutdown intents for supervised children.
#[derive(Debug, Default)]
pub(crate) struct VmSupervisionShutdownQueue {
    timer_owner: Option<VmProcessId>,
    pending: BTreeMap<VmTimerId, VmPendingSupervisionShutdown>,
    pending_children: BTreeMap<(VmSupervisorId, String), VmTimerId>,
}

impl VmSupervisionShutdownQueue {
    /// Requests child shutdown and installs its configured deadline when needed.
    pub(crate) fn begin_shutdown(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        child_id: &str,
        immediate_reason: VmExitReason,
        now_tick: u64,
    ) -> Result<VmSupervisionShutdownStart, String> {
        let (pid, timeout_ms) = child_shutdown_config(supervision, supervisor_id, child_id)?;
        let state = processes
            .get(pid)
            .map(|process| process.state.clone())
            .ok_or_else(|| format!("missing process {}", pid.as_u64()))?;
        if let VmProcessState::Exited(reason) = state {
            return supervision
                .restart_child(processes, supervisor_id, child_id, reason)
                .map(VmSupervisionShutdownStart::Immediate);
        }
        let Some(timeout_ms) = timeout_ms.filter(|timeout| *timeout > 0) else {
            return supervision
                .restart_child(processes, supervisor_id, child_id, immediate_reason)
                .map(VmSupervisionShutdownStart::Immediate);
        };
        let deadline_tick = now_tick.checked_add(timeout_ms).ok_or_else(|| {
            format!(
                "supervision shutdown deadline overflow for child `{child_id}` at tick {now_tick}"
            )
        })?;
        let key = (supervisor_id, child_id.to_string());
        if let Some(timer_id) = self.pending_children.get(&key) {
            return Err(format!(
                "supervision shutdown for child `{child_id}` is already pending on timer {}",
                timer_id.as_u64()
            ));
        }

        let owner = self.live_timer_owner(processes);
        let timer_id = timers.start_one_shot(processes, owner, deadline_tick)?;
        if let Err(error) = processes.send_system_message(owner, pid, shutdown_message(timeout_ms))
        {
            let _ = timers.cancel(timer_id);
            return Err(error);
        }
        self.pending.insert(
            timer_id,
            VmPendingSupervisionShutdown {
                supervisor_id,
                child_id: child_id.to_string(),
                pid,
                timeout_ms,
            },
        );
        self.pending_children.insert(key, timer_id);
        Ok(VmSupervisionShutdownStart::Waiting(
            VmScheduledSupervisionShutdown {
                timer_id,
                child_id: child_id.to_string(),
                pid,
                deadline_tick,
                timeout_ms,
            },
        ))
    }

    /// Completes a pending shutdown after the child exits cooperatively.
    pub(crate) fn complete_shutdown(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        child_id: &str,
    ) -> Result<VmSupervisionShutdownCompletion, String> {
        let key = (supervisor_id, child_id.to_string());
        let timer_id = *self
            .pending_children
            .get(&key)
            .ok_or_else(|| format!("no supervision shutdown is pending for child `{child_id}`"))?;
        let pending = self
            .pending
            .get(&timer_id)
            .expect("pending child index references a shutdown intent");
        let reason = match processes.get(pending.pid).map(|process| &process.state) {
            Some(VmProcessState::Exited(reason)) => reason.clone(),
            Some(_) => return Err(format!("child `{child_id}` has not exited")),
            None => return Err(format!("missing process {}", pending.pid.as_u64())),
        };
        let event = timers.cancel(timer_id)?;
        debug_assert!(matches!(event, VmTimerEvent::Cancelled { .. }));
        let pending = self.remove_pending(timer_id).expect("pending shutdown");
        let restart = supervision.restart_child(
            processes,
            pending.supervisor_id,
            &pending.child_id,
            reason.clone(),
        )?;
        Ok(VmSupervisionShutdownCompletion::Exited {
            timer_id,
            reason,
            restart,
        })
    }

    /// Advances the VM clock and enforces every due supervision shutdown.
    pub(crate) fn advance_clock(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        now_tick: u64,
    ) -> Result<VmSupervisionShutdownAdvance, String> {
        let timer_events = timers.advance_clock(processes, scheduler, now_tick);
        let mut completions = Vec::new();
        let mut unhandled_timer_events = Vec::new();
        for event in &timer_events {
            match self.handle_timer_event(supervision, processes, event)? {
                Some(completion) => completions.push(completion),
                None => unhandled_timer_events.push(*event),
            }
        }
        Ok(VmSupervisionShutdownAdvance {
            timer_events,
            completions,
            unhandled_timer_events,
        })
    }

    /// Applies one terminal timer event to its pending shutdown intent.
    pub(crate) fn handle_timer_event(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        processes: &mut VmProcessTable,
        event: &VmTimerEvent,
    ) -> Result<Option<VmSupervisionShutdownCompletion>, String> {
        let timer_id = event.timer_id();
        let Some(pending) = self.remove_pending(timer_id) else {
            return Ok(None);
        };
        match event {
            VmTimerEvent::Cancelled { .. } => {
                Ok(Some(VmSupervisionShutdownCompletion::Cancelled {
                    timer_id,
                    pid: pending.pid,
                }))
            }
            VmTimerEvent::OwnerExited { .. } => {
                Ok(Some(VmSupervisionShutdownCompletion::TimerOwnerExited {
                    timer_id,
                    pid: pending.pid,
                }))
            }
            VmTimerEvent::Fired { .. } | VmTimerEvent::DeadlineMissed { .. } => {
                let current_pid = child_pid(supervision, pending.supervisor_id, &pending.child_id)?;
                if current_pid != pending.pid {
                    return Ok(Some(VmSupervisionShutdownCompletion::Stale {
                        timer_id,
                        expected_pid: pending.pid,
                        current_pid,
                    }));
                }
                if let Some(VmProcessState::Exited(reason)) =
                    processes.get(pending.pid).map(|process| &process.state)
                {
                    let reason = reason.clone();
                    let restart = supervision.restart_child(
                        processes,
                        pending.supervisor_id,
                        &pending.child_id,
                        reason.clone(),
                    )?;
                    return Ok(Some(VmSupervisionShutdownCompletion::Exited {
                        timer_id,
                        reason,
                        restart,
                    }));
                }
                let reason = VmExitReason::ShutdownTimeout {
                    timeout_ms: pending.timeout_ms,
                };
                processes.exit_process(pending.pid, reason.clone())?;
                let restart = supervision.restart_child(
                    processes,
                    pending.supervisor_id,
                    &pending.child_id,
                    reason,
                )?;
                Ok(Some(VmSupervisionShutdownCompletion::TimedOut {
                    timer_id,
                    pid: pending.pid,
                    timeout_ms: pending.timeout_ms,
                    restart,
                }))
            }
            VmTimerEvent::Coalesced { .. } | VmTimerEvent::Overflow { .. } => Err(format!(
                "one-shot supervision shutdown timer {} emitted an interval-only outcome",
                timer_id.as_u64()
            )),
        }
    }

    /// Returns the number of child shutdowns waiting for completion.
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn remove_pending(&mut self, timer_id: VmTimerId) -> Option<VmPendingSupervisionShutdown> {
        let pending = self.pending.remove(&timer_id)?;
        self.pending_children
            .remove(&(pending.supervisor_id, pending.child_id.clone()));
        Some(pending)
    }

    fn live_timer_owner(&mut self, processes: &mut VmProcessTable) -> VmProcessId {
        if let Some(owner) = self.timer_owner {
            if matches!(
                processes.get(owner).map(|process| &process.state),
                Some(
                    VmProcessState::Runnable
                        | VmProcessState::Blocked
                        | VmProcessState::Hibernated
                        | VmProcessState::Suspended(_),
                )
            ) {
                return owner;
            }
        }
        let owner = processes.spawn_root(VmProcessSource::new(TIMER_OWNER_MODULE, "wait", 0));
        self.timer_owner = Some(owner);
        owner
    }
}

fn child_shutdown_config(
    supervision: &VmSupervisionSystem,
    supervisor_id: VmSupervisorId,
    child_id: &str,
) -> Result<(VmProcessId, Option<u64>), String> {
    let supervisor = supervision
        .supervisors
        .get(&supervisor_id)
        .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
    let child = supervisor
        .children
        .get(child_id)
        .ok_or_else(|| format!("missing child `{child_id}`"))?;
    Ok((child.pid, super::shutdown_timeout_ms(&child.spec)))
}

fn child_pid(
    supervision: &VmSupervisionSystem,
    supervisor_id: VmSupervisorId,
    child_id: &str,
) -> Result<VmProcessId, String> {
    child_shutdown_config(supervision, supervisor_id, child_id).map(|(pid, _)| pid)
}

fn shutdown_message(timeout_ms: u64) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("shutdown".to_string()),
        ReplValue::Int(timeout_ms.try_into().unwrap_or(i64::MAX)),
    ])
}

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod shutdown_test;
