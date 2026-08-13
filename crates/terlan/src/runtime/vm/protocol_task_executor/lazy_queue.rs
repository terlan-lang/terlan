//! Lazily allocated bounded MPSC storage for protocol-owner ingress.

use std::sync::atomic::{AtomicUsize, Ordering};

use concurrent_queue::ConcurrentQueue;

/// An unsegmented hard capacity paired with storage allocated only on demand.
pub(super) struct VmLazyBoundedQueue<T> {
    queue: ConcurrentQueue<T>,
    capacity: usize,
    length: AtomicUsize,
}

impl<T> VmLazyBoundedQueue<T> {
    pub(super) fn bounded(capacity: usize) -> Self {
        assert!(capacity > 0, "lazy bounded queue capacity is nonzero");
        Self {
            queue: ConcurrentQueue::unbounded(),
            capacity,
            length: AtomicUsize::new(0),
        }
    }

    pub(super) fn push(&self, value: T) -> Result<(), VmLazyQueuePushError<T>> {
        let mut length = self.length.load(Ordering::Relaxed);
        loop {
            if length >= self.capacity {
                return Err(VmLazyQueuePushError(value));
            }
            match self.length.compare_exchange_weak(
                length,
                length + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => length = observed,
            }
        }
        if let Err(error) = self.queue.push(value) {
            self.length.fetch_sub(1, Ordering::Release);
            return Err(VmLazyQueuePushError(error.into_inner()));
        }
        Ok(())
    }

    pub(super) fn pop(&self) -> Result<T, VmLazyQueuePopError> {
        self.queue
            .pop()
            .map_err(|_| VmLazyQueuePopError)
            .inspect(|_value| {
                let previous = self.length.fetch_sub(1, Ordering::AcqRel);
                debug_assert!(previous > 0, "lazy bounded queue length underflow");
            })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.length.load(Ordering::Acquire) == 0
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.length.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub(super) struct VmLazyQueuePushError<T>(T);

impl<T> VmLazyQueuePushError<T> {
    pub(super) fn into_inner(self) -> T {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VmLazyQueuePopError;
