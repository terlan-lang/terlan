use std::collections::BTreeMap;

use super::{
    VmDistributedStorageAdapter, VmDistributedStorageOperation, VmDistributedStorageOutcome,
};
use crate::runtime::vm::{
    process::{VmProcessId, VmProcessTable},
    timer::{VmTimerEvent, VmTimerId, VmTimerKind, VmTimerTable},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VmPendingCheckpointFlush {
    owner: VmProcessId,
    sequence: u64,
}

/// A checkpoint flush protected by one VM-owned deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmScheduledCheckpointFlush {
    pub(crate) timer_id: VmTimerId,
    pub(crate) owner: VmProcessId,
    pub(crate) sequence: u64,
    pub(crate) deadline_tick: u64,
}

/// Terminal result of resolving a checkpoint flush deadline.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmCheckpointFlushCompletion {
    Completed {
        timer_id: VmTimerId,
        outcome: VmDistributedStorageOutcome,
    },
    TimedOut {
        timer_id: VmTimerId,
        outcome: VmDistributedStorageOutcome,
    },
    Cancelled {
        timer_id: VmTimerId,
        sequence: u64,
    },
    OwnerExited {
        timer_id: VmTimerId,
        sequence: u64,
    },
}

/// Coordinates checkpoint flush completion against VM-owned timer delivery.
///
/// A successful completion first proves that it won the deadline race by
/// cancelling the active timer, then flushes the adapter. A fired deadline
/// removes the intent and leaves the adapter's durable sequence unchanged.
#[derive(Debug, Default)]
pub(crate) struct VmCheckpointFlushDeadlineQueue {
    pending: BTreeMap<VmTimerId, VmPendingCheckpointFlush>,
    pending_by_owner: BTreeMap<VmProcessId, VmTimerId>,
}

impl VmCheckpointFlushDeadlineQueue {
    /// Starts one checkpoint flush deadline for a live process and open adapter.
    pub(crate) fn start(
        &mut self,
        adapter: &VmDistributedStorageAdapter,
        timers: &mut VmTimerTable,
        processes: &VmProcessTable,
        owner: VmProcessId,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmScheduledCheckpointFlush, String> {
        if timeout_ticks == 0 {
            return Err("checkpoint flush timeout must be positive".to_string());
        }
        let deadline_tick = now_tick
            .checked_add(timeout_ticks)
            .ok_or_else(|| "checkpoint flush deadline overflow".to_string())?;
        if let Some(timer_id) = self.pending_by_owner.get(&owner) {
            return Err(format!(
                "checkpoint flush for process {} is already pending on timer {}",
                owner.as_u64(),
                timer_id.as_u64()
            ));
        }
        if let Some(outcome) = adapter.guard(VmDistributedStorageOperation::Flush) {
            return Err(format!(
                "checkpoint flush unavailable: {}",
                outcome.reason()
            ));
        }

        let sequence = adapter.latest_sequence();
        let timer_id = timers.start_one_shot(processes, owner, deadline_tick)?;
        self.pending
            .insert(timer_id, VmPendingCheckpointFlush { owner, sequence });
        self.pending_by_owner.insert(owner, timer_id);
        Ok(VmScheduledCheckpointFlush {
            timer_id,
            owner,
            sequence,
            deadline_tick,
        })
    }

    /// Completes a flush only when cancellation proves its deadline is active.
    pub(crate) fn complete(
        &mut self,
        adapter: &mut VmDistributedStorageAdapter,
        timers: &mut VmTimerTable,
        timer_id: VmTimerId,
    ) -> Result<VmCheckpointFlushCompletion, String> {
        let pending = self.pending(timer_id)?;
        timers.cancel(timer_id).map_err(|error| {
            format!(
                "checkpoint flush timer {} no longer owns completion: {error}",
                timer_id.as_u64()
            )
        })?;
        self.remove_pending(timer_id, pending);
        let outcome = adapter.flush();
        Ok(VmCheckpointFlushCompletion::Completed { timer_id, outcome })
    }

    /// Cancels a pending flush without advancing the durable boundary.
    pub(crate) fn cancel(
        &mut self,
        timers: &mut VmTimerTable,
        timer_id: VmTimerId,
    ) -> Result<VmCheckpointFlushCompletion, String> {
        let pending = self.pending(timer_id)?;
        timers.cancel(timer_id)?;
        self.remove_pending(timer_id, pending);
        Ok(VmCheckpointFlushCompletion::Cancelled {
            timer_id,
            sequence: pending.sequence,
        })
    }

    /// Resolves a timer event if it belongs to a pending checkpoint flush.
    pub(crate) fn handle_timer_event(
        &mut self,
        event: &VmTimerEvent,
    ) -> Result<Option<VmCheckpointFlushCompletion>, String> {
        let timer_id = event.timer_id();
        let Some(pending) = self.pending.get(&timer_id).copied() else {
            return Ok(None);
        };
        let observed_owner = timer_event_owner(event);
        if observed_owner != pending.owner {
            return Err(format!(
                "checkpoint flush timer {} owner mismatch: expected {}, observed {}",
                timer_id.as_u64(),
                pending.owner.as_u64(),
                observed_owner.as_u64()
            ));
        }
        if timer_event_kind(event) != VmTimerKind::OneShot {
            return Err(format!(
                "checkpoint flush timer {} emitted non-one-shot outcome",
                timer_id.as_u64()
            ));
        }
        self.pending.remove(&timer_id);
        self.pending_by_owner.remove(&pending.owner);
        let completion = match event {
            VmTimerEvent::Fired { .. } | VmTimerEvent::DeadlineMissed { .. } => {
                VmCheckpointFlushCompletion::TimedOut {
                    timer_id,
                    outcome: VmDistributedStorageOutcome::FlushTimedOut {
                        operation: VmDistributedStorageOperation::Flush,
                        sequence: pending.sequence,
                    },
                }
            }
            VmTimerEvent::Cancelled { .. } => VmCheckpointFlushCompletion::Cancelled {
                timer_id,
                sequence: pending.sequence,
            },
            VmTimerEvent::OwnerExited { .. } => VmCheckpointFlushCompletion::OwnerExited {
                timer_id,
                sequence: pending.sequence,
            },
            VmTimerEvent::Coalesced { .. } | VmTimerEvent::Overflow { .. } => unreachable!(
                "coalesced and overflow events are rejected by the one-shot kind check"
            ),
        };
        Ok(Some(completion))
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn pending(&self, timer_id: VmTimerId) -> Result<VmPendingCheckpointFlush, String> {
        self.pending.get(&timer_id).copied().ok_or_else(|| {
            format!(
                "missing pending checkpoint flush for timer {}",
                timer_id.as_u64()
            )
        })
    }

    fn remove_pending(&mut self, timer_id: VmTimerId, pending: VmPendingCheckpointFlush) {
        self.pending.remove(&timer_id);
        self.pending_by_owner.remove(&pending.owner);
    }
}

fn timer_event_owner(event: &VmTimerEvent) -> VmProcessId {
    event.owner()
}

fn timer_event_kind(event: &VmTimerEvent) -> VmTimerKind {
    event.kind()
}

#[cfg(test)]
#[path = "deadline_test.rs"]
mod deadline_test;
