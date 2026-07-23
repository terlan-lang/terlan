//! Actor-local MPSC publication and park/wake handshake.

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};

use concurrent_queue::ConcurrentQueue;

use super::{VmActorDirectoryError, VmActorHandle, VmActorPublication};

/// Maximum complete fragments admitted before producers receive backpressure.
pub(super) const ACTOR_MAILBOX_CAPACITY: usize = 1_024;

const ACTIVE: u8 = 0;
const PARKING: u8 = 1;
const PARKED: u8 = 2;
const NOTIFIED: u8 = 3;

/// Scheduler action required after publishing one complete fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmMailboxWake {
    /// The actor is active or already has a visible notification.
    Observed,
    /// The actor was parked and must receive one runnable queue entry.
    Enqueue,
}

/// One generation-qualified payload stored in the actor-local MPSC queue.
#[derive(Debug)]
pub(super) struct VmPublishedFragment<P> {
    /// Publication identity reserved before this payload became visible.
    pub(super) publication: VmActorPublication,
    /// Fully initialized payload transferred to the single consumer.
    pub(super) payload: P,
}

/// Established lock-free queue plus the authoritative no-lost-wakeup state.
#[derive(Debug)]
pub(super) struct VmActorMailbox<P> {
    queue: ConcurrentQueue<VmPublishedFragment<P>>,
    admitted: AtomicUsize,
    next_sequence: AtomicU64,
    wake_state: AtomicU8,
}

impl<P> Default for VmActorMailbox<P> {
    /// Creates an active bounded MPSC queue for one actor generation.
    fn default() -> Self {
        Self {
            queue: ConcurrentQueue::bounded(ACTOR_MAILBOX_CAPACITY),
            admitted: AtomicUsize::new(0),
            next_sequence: AtomicU64::new(0),
            wake_state: AtomicU8::new(ACTIVE),
        }
    }
}

impl<P> VmActorMailbox<P> {
    /// Publishes a complete payload with release ordering before notification.
    pub(super) fn publish(
        &self,
        handle: VmActorHandle,
        payload: P,
    ) -> Result<(VmActorPublication, VmMailboxWake), VmActorDirectoryError> {
        self.admitted
            .try_update(Ordering::AcqRel, Ordering::Acquire, |admitted| {
                (admitted < ACTOR_MAILBOX_CAPACITY).then_some(admitted + 1)
            })
            .map_err(|_| VmActorDirectoryError::MailboxFull(handle))?;
        let sequence = self
            .next_sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let publication = VmActorPublication { handle, sequence };
        let fragment = VmPublishedFragment {
            publication,
            payload,
        };
        if self.queue.push(fragment).is_err() {
            self.admitted.fetch_sub(1, Ordering::AcqRel);
            return Err(VmActorDirectoryError::MailboxFull(handle));
        }
        let previous = self.wake_state.swap(NOTIFIED, Ordering::AcqRel);
        let wake = if previous == PARKED {
            VmMailboxWake::Enqueue
        } else {
            VmMailboxWake::Observed
        };
        Ok((publication, wake))
    }

    /// Drains fragments in queue publication order for the single consumer.
    pub(super) fn drain(&self, mut consume: impl FnMut(VmPublishedFragment<P>)) -> usize {
        let mut drained = 0usize;
        loop {
            match self.queue.pop() {
                Ok(fragment) => {
                    self.admitted.fetch_sub(1, Ordering::AcqRel);
                    consume(fragment);
                    drained = drained.saturating_add(1);
                }
                Err(_) => break,
            }
        }
        if self.queue.is_empty() {
            let _ = self.wake_state.compare_exchange(
                NOTIFIED,
                ACTIVE,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        drained
    }

    /// Starts the receiver side of the park handshake and rechecks the queue.
    pub(super) fn prepare_park(&self) -> bool {
        if self
            .wake_state
            .compare_exchange(ACTIVE, PARKING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        if !self.queue.is_empty() {
            self.wake_state.store(NOTIFIED, Ordering::Release);
            return false;
        }
        self.wake_state
            .compare_exchange(PARKING, PARKED, Ordering::Release, Ordering::Acquire)
            .is_ok()
    }

    /// Cancels a park attempt or acknowledges a wake before actor execution.
    pub(super) fn activate(&self) {
        self.wake_state.store(ACTIVE, Ordering::Release);
    }

    /// Returns whether a producer published after the receiver prepared to park.
    pub(super) fn is_notified(&self) -> bool {
        self.wake_state.load(Ordering::Acquire) == NOTIFIED
    }

    /// Returns the number of complete fragments awaiting receiver integration.
    pub(super) fn len(&self) -> usize {
        self.queue.len()
    }
}

#[cfg(all(test, not(feature = "multicore-tsan-harness")))]
#[path = "mailbox_test.rs"]
mod mailbox_test;
