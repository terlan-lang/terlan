//! Stable task wake ownership for one protocol scheduler.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Wake, Waker};

use concurrent_queue::ConcurrentQueue;
use mio::{Registry, Token, Waker as MioWaker};

use super::{push_owner_local_scheduled, VmSchedulerId, CURRENT_PROTOCOL_SCHEDULER};

/// Reusable future waker paired with its generation-qualified task token.
pub(super) struct VmProtocolTaskWakeSlot {
    /// Shared wake state retained by the task and its asynchronous operations.
    pub(super) wake: Arc<VmProtocolTaskWake>,
    /// Standard-library waker passed into the future polling context.
    pub(super) waker: Waker,
}

impl VmProtocolTaskWakeSlot {
    /// Creates one wake slot for an exact task token and protocol owner.
    pub(super) fn new(token: Token, owner: Arc<VmProtocolOwnerWake>) -> Self {
        let wake = Arc::new(VmProtocolTaskWake {
            token: AtomicUsize::new(token.0),
            scheduled: AtomicBool::new(false),
            owner,
        });
        let waker = Waker::from(Arc::clone(&wake));
        Self { wake, waker }
    }
}

/// Atomic task wake state shared by every clone of one future waker.
pub(super) struct VmProtocolTaskWake {
    /// Generation-qualified task token to enqueue.
    pub(super) token: AtomicUsize,
    /// Whether this wake is already represented in an owner queue.
    pub(super) scheduled: AtomicBool,
    /// Fixed protocol scheduler that owns future polling.
    pub(super) owner: Arc<VmProtocolOwnerWake>,
}

/// One shared readiness handle retained by all tasks on a fixed owner.
pub(super) struct VmProtocolOwnerWake {
    /// Fixed scheduler that owns this readiness handle.
    pub(super) scheduler: VmSchedulerId,
    /// Poll registry used to arm task-owned transports.
    pub(super) registry: Registry,
    /// Bounded cross-thread wake queue consumed by the owner.
    pub(super) queue: Arc<ConcurrentQueue<Token>>,
    /// Poll wake handle used for cross-thread completions.
    pub(super) poll_waker: Arc<MioWaker>,
}

impl Wake for VmProtocolTaskWake {
    /// Publishes an owned wake through the shared implementation.
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    /// Enqueues one task at most once until its owner drains the wake.
    fn wake_by_ref(self: &Arc<Self>) {
        if !self.scheduled.swap(true, Ordering::AcqRel) {
            let token = Token(self.token.load(Ordering::Acquire));
            // Owner-local self-wakes are drained before the poller sleeps.
            let is_owner_thread = CURRENT_PROTOCOL_SCHEDULER
                .with(|current| current.get() == Some(self.owner.scheduler));
            if is_owner_thread {
                push_owner_local_scheduled(token);
            } else {
                let _ = self.owner.queue.push(token);
                let _ = self.owner.poll_waker.wake();
            }
        }
    }
}
