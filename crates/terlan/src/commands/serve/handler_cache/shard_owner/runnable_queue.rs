//! Class-aware runnable queues owned by one generated AOT scheduler thread.

use std::collections::VecDeque;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::PureNativeSuspension;
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};
use crate::runtime::vm::work_stealing::VmSchedulerWorkSnapshot;

use super::OwnedInvocationStep;

const CLASS_COUNT: usize = 3;
const SERVICE_CYCLE: [VmSchedulerClass; 6] = [
    VmSchedulerClass::Priority,
    VmSchedulerClass::Priority,
    VmSchedulerClass::Normal,
    VmSchedulerClass::Priority,
    VmSchedulerClass::Normal,
    VmSchedulerClass::Background,
];

/// One generated continuation waiting for another scheduler-owned slice.
pub(super) struct PendingRunnableInvocation {
    pub(super) route: VmFixedActorRoute,
    pub(super) owner: VmProcessId,
    pub(super) class: VmSchedulerClass,
    pub(super) suspension: PureNativeSuspension,
    pub(super) enqueued_at: Instant,
    pub(super) reply: SyncSender<Result<OwnedInvocationStep, String>>,
}

/// Three bounded scheduling-class queues with deterministic weighted service.
pub(super) struct GeneratedRunnableQueues {
    queues: [VecDeque<PendingRunnableInvocation>; CLASS_COUNT],
    service_cursor: usize,
}

impl GeneratedRunnableQueues {
    /// Creates one empty queue set at the start of its service cycle.
    pub(super) fn new() -> Self {
        Self {
            queues: std::array::from_fn(|_| VecDeque::new()),
            service_cursor: 0,
        }
    }

    /// Returns whether every scheduling class is empty.
    pub(super) fn is_empty(&self) -> bool {
        self.queues.iter().all(VecDeque::is_empty)
    }

    /// Returns the total number of retained runnable continuations.
    pub(super) fn len(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum()
    }

    /// Publishes one continuation at the tail of its exact scheduling class.
    pub(super) fn push(&mut self, pending: PendingRunnableInvocation) {
        self.queues[class_index(pending.class)].push_back(pending);
    }

    /// Selects local work through the canonical priority/normal/background cycle.
    pub(super) fn pop_weighted(&mut self) -> Option<PendingRunnableInvocation> {
        for _ in 0..SERVICE_CYCLE.len() {
            let class = SERVICE_CYCLE[self.service_cursor];
            self.service_cursor = (self.service_cursor + 1) % SERVICE_CYCLE.len();
            if let Some(pending) = self.queues[class_index(class)].pop_front() {
                return Some(pending);
            }
        }
        self.queues.iter_mut().find_map(VecDeque::pop_front)
    }

    /// Removes the newest candidate from one policy-selected scheduling class.
    pub(super) fn pop_for_steal(
        &mut self,
        class: VmSchedulerClass,
    ) -> Option<PendingRunnableInvocation> {
        self.queues[class_index(class)].pop_back()
    }

    /// Removes any retained continuation for cancellation during shutdown.
    pub(super) fn pop_any(&mut self) -> Option<PendingRunnableInvocation> {
        self.queues.iter_mut().find_map(VecDeque::pop_front)
    }

    /// Captures exact per-class load and oldest monotonic wait.
    pub(super) fn snapshot(&self, scheduler: VmSchedulerId) -> VmSchedulerWorkSnapshot {
        let now = Instant::now();
        let runnable = std::array::from_fn(|index| self.queues[index].len());
        let oldest_wait = std::array::from_fn(|index| {
            self.queues[index]
                .iter()
                .map(|pending| {
                    u64::try_from(
                        now.saturating_duration_since(pending.enqueued_at)
                            .as_millis(),
                    )
                    .unwrap_or(u64::MAX)
                })
                .max()
                .unwrap_or(0)
        });
        VmSchedulerWorkSnapshot::new(scheduler, runnable, oldest_wait)
    }
}

/// Maps one scheduling class to the stable snapshot and queue order.
const fn class_index(class: VmSchedulerClass) -> usize {
    match class {
        VmSchedulerClass::Priority => 0,
        VmSchedulerClass::Normal => 1,
        VmSchedulerClass::Background => 2,
    }
}
