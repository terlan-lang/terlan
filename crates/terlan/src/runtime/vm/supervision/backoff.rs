use std::collections::BTreeMap;

use super::{
    restart_delay_ms, VmRestartPolicy, VmSupervisionRestart, VmSupervisionSystem, VmSupervisorId,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    timer::{VmTimerEvent, VmTimerId, VmTimerTable},
};

const TIMER_OWNER_MODULE: &str = "runtime.SupervisionBackoff";

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmPendingSupervisionRestart {
    supervisor_id: VmSupervisorId,
    child_id: String,
    failed_pid: VmProcessId,
    reason: VmExitReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmScheduledSupervisionRestart {
    pub(crate) timer_id: VmTimerId,
    pub(crate) child_id: String,
    pub(crate) failed_pid: VmProcessId,
    pub(crate) deadline_tick: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmSupervisionRestartPlan {
    child_id: String,
    failed_pid: VmProcessId,
    should_restart: bool,
    restart_limit_reached: bool,
    delay_ticks: u64,
}

/// Result of requesting a restart through the VM-owned backoff scheduler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionBackoffStart {
    Immediate(VmSupervisionRestart),
    Deferred {
        restarted_immediately: Vec<VmSupervisionRestart>,
        scheduled: Vec<VmScheduledSupervisionRestart>,
    },
}

/// Terminal result for one scheduled supervision restart deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionBackoffCompletion {
    Restarted(VmSupervisionRestart),
    Cancelled {
        timer_id: VmTimerId,
        failed_pid: VmProcessId,
    },
    TimerOwnerExited {
        timer_id: VmTimerId,
        failed_pid: VmProcessId,
    },
    Stale {
        timer_id: VmTimerId,
        failed_pid: VmProcessId,
        current_pid: VmProcessId,
    },
}

/// Timer-backed restart intents for VM supervision backoff.
///
/// The queue owns only restart intent metadata. Deadline ordering, cancellation,
/// monotonic clock handling, and owner cleanup remain in `VmTimerTable`.
#[derive(Debug, Default)]
pub(crate) struct VmSupervisionBackoffQueue {
    timer_owner: Option<VmProcessId>,
    pending: BTreeMap<VmTimerId, VmPendingSupervisionRestart>,
    pending_children: BTreeMap<(VmSupervisorId, String), VmTimerId>,
}

impl VmSupervisionBackoffQueue {
    /// Defers a restart when the child's next policy delay is non-zero.
    pub(crate) fn schedule_restart(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        child_id: &str,
        reason: VmExitReason,
        now_tick: u64,
    ) -> Result<VmSupervisionBackoffStart, String> {
        let plans = restart_plan(supervision, supervisor_id, child_id, &reason)?;
        self.reject_pending_children(supervisor_id, &plans)?;
        if plans.iter().any(|plan| plan.restart_limit_reached)
            || plans.iter().all(|plan| plan.delay_ticks == 0)
        {
            return supervision
                .restart_child(processes, supervisor_id, child_id, reason)
                .map(VmSupervisionBackoffStart::Immediate);
        }

        let deadlines = restart_deadlines(&plans, now_tick)?;
        require_planned_processes(processes, &plans)?;
        let owner = self.live_timer_owner(processes);
        for plan in &plans {
            exit_failed_process(processes, plan.failed_pid, &reason)?;
        }

        let mut restarted_immediately = Vec::new();
        let mut scheduled = Vec::new();
        for (plan, deadline_tick) in plans.into_iter().zip(deadlines) {
            if !plan.should_restart {
                continue;
            }
            let Some(deadline_tick) = deadline_tick else {
                restarted_immediately.push(supervision.restart_child_after_backoff(
                    processes,
                    supervisor_id,
                    &plan.child_id,
                    reason.clone(),
                )?);
                continue;
            };
            let timer_id = timers.start_one_shot(processes, owner, deadline_tick)?;
            let key = (supervisor_id, plan.child_id.clone());
            self.pending.insert(
                timer_id,
                VmPendingSupervisionRestart {
                    supervisor_id,
                    child_id: plan.child_id.clone(),
                    failed_pid: plan.failed_pid,
                    reason: reason.clone(),
                },
            );
            self.pending_children.insert(key, timer_id);
            scheduled.push(VmScheduledSupervisionRestart {
                timer_id,
                child_id: plan.child_id,
                failed_pid: plan.failed_pid,
                deadline_tick,
            });
        }
        Ok(VmSupervisionBackoffStart::Deferred {
            restarted_immediately,
            scheduled,
        })
    }

    /// Applies one terminal timer event to its pending restart intent.
    pub(crate) fn handle_timer_event(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        processes: &mut VmProcessTable,
        event: &VmTimerEvent,
    ) -> Result<Option<VmSupervisionBackoffCompletion>, String> {
        let timer_id = event.timer_id();
        let Some(pending) = self.pending.remove(&timer_id) else {
            return Ok(None);
        };
        self.pending_children
            .remove(&(pending.supervisor_id, pending.child_id.clone()));

        match event {
            VmTimerEvent::Cancelled { .. } => Ok(Some(VmSupervisionBackoffCompletion::Cancelled {
                timer_id,
                failed_pid: pending.failed_pid,
            })),
            VmTimerEvent::OwnerExited { .. } => {
                Ok(Some(VmSupervisionBackoffCompletion::TimerOwnerExited {
                    timer_id,
                    failed_pid: pending.failed_pid,
                }))
            }
            VmTimerEvent::Fired { .. } | VmTimerEvent::DeadlineMissed { .. } => {
                let current_pid = child_pid(supervision, pending.supervisor_id, &pending.child_id)?;
                if current_pid != pending.failed_pid {
                    return Ok(Some(VmSupervisionBackoffCompletion::Stale {
                        timer_id,
                        failed_pid: pending.failed_pid,
                        current_pid,
                    }));
                }
                let restart = supervision.restart_child_after_backoff(
                    processes,
                    pending.supervisor_id,
                    &pending.child_id,
                    pending.reason,
                )?;
                Ok(Some(VmSupervisionBackoffCompletion::Restarted(restart)))
            }
            VmTimerEvent::Coalesced { .. } | VmTimerEvent::Overflow { .. } => Err(format!(
                "one-shot supervision backoff timer {} emitted an interval-only outcome",
                timer_id.as_u64()
            )),
        }
    }

    /// Cancels one pending restart through the timer table.
    pub(crate) fn cancel_restart(
        &mut self,
        supervision: &mut VmSupervisionSystem,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        timer_id: VmTimerId,
    ) -> Result<VmSupervisionBackoffCompletion, String> {
        let event = timers.cancel(timer_id)?;
        self.handle_timer_event(supervision, processes, &event)?
            .ok_or_else(|| {
                format!(
                    "missing pending supervision restart for timer {}",
                    timer_id.as_u64()
                )
            })
    }

    /// Returns the number of pending restart intents.
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn reject_pending_children(
        &self,
        supervisor_id: VmSupervisorId,
        plans: &[VmSupervisionRestartPlan],
    ) -> Result<(), String> {
        for plan in plans {
            if let Some(timer_id) = self
                .pending_children
                .get(&(supervisor_id, plan.child_id.clone()))
            {
                return Err(format!(
                    "supervision restart for child `{}` is already pending on timer {}",
                    plan.child_id,
                    timer_id.as_u64()
                ));
            }
        }
        Ok(())
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

fn restart_plan(
    supervision: &VmSupervisionSystem,
    supervisor_id: VmSupervisorId,
    child_id: &str,
    reason: &VmExitReason,
) -> Result<Vec<VmSupervisionRestartPlan>, String> {
    let supervisor = supervision
        .supervisors
        .get(&supervisor_id)
        .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
    if !supervisor.children.contains_key(child_id) {
        return Err(format!("missing child `{child_id}`"));
    }
    let child_ids = match supervisor.policy {
        VmRestartPolicy::OneForOne => vec![child_id.to_string()],
        VmRestartPolicy::OneForAll => supervisor.child_order.clone(),
        VmRestartPolicy::RestForOne => {
            let start = supervisor
                .child_order
                .iter()
                .position(|known| known == child_id)
                .expect("validated child belongs to supervisor order");
            supervisor.child_order[start..].to_vec()
        }
    };
    let mut plans = Vec::with_capacity(child_ids.len());
    for selected_id in child_ids {
        let child = supervisor
            .children
            .get(&selected_id)
            .expect("selected child belongs to supervisor");
        let should_restart = child.spec.restart_class.should_restart(reason);
        let restart_limit_reached =
            should_restart && child.restart_count >= child.spec.restart_limit;
        let delay_ticks = if should_restart && !restart_limit_reached {
            restart_delay_ms(&child.spec, child.restart_count.saturating_add(1))
        } else {
            0
        };
        plans.push(VmSupervisionRestartPlan {
            child_id: selected_id,
            failed_pid: child.pid,
            should_restart,
            restart_limit_reached,
            delay_ticks,
        });
    }
    Ok(plans)
}

fn restart_deadlines(
    plans: &[VmSupervisionRestartPlan],
    now_tick: u64,
) -> Result<Vec<Option<u64>>, String> {
    plans
        .iter()
        .map(|plan| {
            if !plan.should_restart || plan.delay_ticks == 0 {
                return Ok(None);
            }
            now_tick
                .checked_add(plan.delay_ticks)
                .map(Some)
                .ok_or_else(|| {
                    format!(
                        "supervision restart deadline overflow for child `{}` at tick {now_tick}",
                        plan.child_id
                    )
                })
        })
        .collect()
}

fn require_planned_processes(
    processes: &VmProcessTable,
    plans: &[VmSupervisionRestartPlan],
) -> Result<(), String> {
    for plan in plans {
        if processes.get(plan.failed_pid).is_none() {
            return Err(format!("missing process {}", plan.failed_pid.as_u64()));
        }
    }
    Ok(())
}

fn child_pid(
    supervision: &VmSupervisionSystem,
    supervisor_id: VmSupervisorId,
    child_id: &str,
) -> Result<VmProcessId, String> {
    supervision
        .supervisors
        .get(&supervisor_id)
        .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?
        .children
        .get(child_id)
        .map(|child| child.pid)
        .ok_or_else(|| format!("missing child `{child_id}`"))
}

fn exit_failed_process(
    processes: &mut VmProcessTable,
    failed_pid: VmProcessId,
    reason: &VmExitReason,
) -> Result<(), String> {
    match processes.get(failed_pid).map(|process| &process.state) {
        Some(VmProcessState::Exited(_)) => Ok(()),
        Some(_) => processes
            .exit_process(failed_pid, reason.clone())
            .map(|_| ()),
        None => Err(format!("missing process {}", failed_pid.as_u64())),
    }
}

#[cfg(test)]
#[path = "backoff_test.rs"]
mod backoff_test;

#[cfg(test)]
#[path = "backoff_group_test.rs"]
mod backoff_group_test;
