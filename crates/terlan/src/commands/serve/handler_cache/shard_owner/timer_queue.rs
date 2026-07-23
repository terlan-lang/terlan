//! Bounded scheduler-local deadlines for generated Timer continuations.

use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{PureNativeSuspension, PureNativeTimerWait};
use crate::runtime::vm::scheduler_topology::VmFixedActorRoute;

use super::OwnedInvocationStep;

/// Maximum parked timers retained by one fixed scheduler owner.
const TIMER_QUEUE_CAPACITY: usize = super::SHARD_INBOX_CAPACITY;

/// Complete generated invocation retained until its absolute VM deadline.
pub(super) struct PendingTimerInvocation {
    /// Stable fixed-scheduler route.
    pub(super) route: VmFixedActorRoute,
    /// Exact actor owning the generated continuation.
    pub(super) owner: VmProcessId,
    /// Generated continuation parked on Timer.
    pub(super) suspension: PureNativeSuspension,
    /// Generation-qualified timer delivery authority.
    pub(super) wait: PureNativeTimerWait,
    /// Original invocation reply retained across parking.
    pub(super) reply: SyncSender<Result<OwnedInvocationStep, String>>,
    /// Host deadline derived from the scheduler's monotonic origin.
    due: Instant,
}

/// Queue rejection retaining the invocation reply that must be settled.
pub(super) struct PendingTimerRejection {
    /// Stable capacity or deadline failure.
    pub(super) reason: String,
    /// Original invocation reply not admitted by the timer queue.
    pub(super) reply: SyncSender<Result<OwnedInvocationStep, String>>,
}

/// One monotonic clock and bounded timer queue owned by a scheduler thread.
pub(super) struct GeneratedTimerQueue {
    origin: Instant,
    pending: Vec<PendingTimerInvocation>,
}

impl GeneratedTimerQueue {
    /// Starts one scheduler clock with no retained timers.
    pub(super) fn new() -> Self {
        Self {
            origin: Instant::now(),
            pending: Vec::new(),
        }
    }

    /// Returns the scheduler's current monotonic millisecond tick.
    pub(super) fn observed_tick(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Retains one parked timer until its absolute logical deadline.
    pub(super) fn push(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeTimerWait,
        reply: SyncSender<Result<OwnedInvocationStep, String>>,
    ) -> Result<(), PendingTimerRejection> {
        if self.pending.len() == TIMER_QUEUE_CAPACITY {
            return Err(PendingTimerRejection {
                reason: format!(
                    "error[vm.timer_capacity]: scheduler timer capacity {TIMER_QUEUE_CAPACITY} exhausted"
                ),
                reply,
            });
        }
        let Some(due) = self
            .origin
            .checked_add(Duration::from_millis(wait.deadline_tick()))
        else {
            return Err(PendingTimerRejection {
                reason: "error[vm.timer_deadline]: host deadline overflow".to_string(),
                reply,
            });
        };
        self.pending.push(PendingTimerInvocation {
            route,
            owner,
            suspension,
            wait,
            reply,
            due,
        });
        Ok(())
    }

    /// Returns how long command ingress may block before the next timer is due.
    pub(super) fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.pending
            .iter()
            .map(|pending| pending.due.saturating_duration_since(now))
            .min()
    }

    /// Removes every timer whose host deadline has passed.
    pub(super) fn take_due(&mut self, now: Instant) -> Vec<PendingTimerInvocation> {
        let mut due = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            if self.pending[index].due <= now {
                due.push(self.pending.swap_remove(index));
            } else {
                index += 1;
            }
        }
        due.sort_by_key(|pending| pending.wait.deadline_tick());
        due
    }

    /// Removes every timer retained for one terminal actor route.
    pub(super) fn remove_route(&mut self, route: VmFixedActorRoute) -> Vec<PendingTimerInvocation> {
        let mut removed = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            if self.pending[index].route == route {
                removed.push(self.pending.swap_remove(index));
            } else {
                index += 1;
            }
        }
        removed
    }

    /// Returns every actor route currently parked on a timer.
    pub(super) fn routes(&self) -> Vec<VmFixedActorRoute> {
        self.pending.iter().map(|pending| pending.route).collect()
    }
}
