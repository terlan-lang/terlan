//! Bounded child futures polled under one existing VM task owner.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};

type LocalTask = Pin<Box<dyn Future<Output = ()> + 'static>>;
type RootTask<E> = Pin<Box<dyn Future<Output = Result<(), E>> + 'static>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpawnError {
    CapacityExceeded,
    Closed,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TaskGroupError<E> {
    Root(E),
    CapacityExceeded,
}

struct State {
    queued: VecDeque<LocalTask>,
    active: usize,
    capacity: NonZeroUsize,
    overflowed: bool,
    closed: bool,
    waker: Option<Waker>,
}

/// A weak admission handle cannot keep cancelled tasks or their owner alive.
#[derive(Clone)]
pub(crate) struct LocalTaskSpawner(Weak<RefCell<State>>);

impl LocalTaskSpawner {
    pub(crate) fn spawn(&self, task: LocalTask) -> Result<(), SpawnError> {
        let state = self.0.upgrade().ok_or(SpawnError::Closed)?;
        let mut state = state.borrow_mut();
        if state.closed {
            return Err(SpawnError::Closed);
        }
        // Count the currently polled child as well as queued children. A child
        // may submit more work while it is temporarily outside the queue.
        let result = if state.overflowed || state.active == state.capacity.get() {
            state.overflowed = true;
            Err(SpawnError::CapacityExceeded)
        } else {
            state.active += 1;
            state.queued.push_back(task);
            Ok(())
        };
        let waker = state.waker.clone();
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
        result
    }
}

/// Runs a root and its children without creating a runtime or leaving its owner.
/// Dropping the driver closes admission before cancelling retained futures.
pub(crate) struct LocalTaskGroup<E> {
    root: Option<RootTask<E>>,
    state: Rc<RefCell<State>>,
}

impl<E> LocalTaskGroup<E> {
    pub(crate) fn new<F>(capacity: NonZeroUsize, root: impl FnOnce(LocalTaskSpawner) -> F) -> Self
    where
        F: Future<Output = Result<(), E>> + 'static,
    {
        let state = Rc::new(RefCell::new(State {
            queued: VecDeque::new(),
            active: 0,
            capacity,
            overflowed: false,
            closed: false,
            waker: None,
        }));
        let root = root(LocalTaskSpawner(Rc::downgrade(&state)));
        Self {
            root: Some(Box::pin(root)),
            state,
        }
    }

    fn close(&mut self) {
        let queued = {
            let mut state = self.state.borrow_mut();
            state.closed = true;
            state.active = 0;
            state.waker = None;
            std::mem::take(&mut state.queued)
        };
        // Destructors may try to submit cleanup work; reject it without a
        // RefCell borrow spanning user code.
        drop(queued);
        self.root = None;
    }

    fn overflowed(&self) -> bool {
        self.state.borrow().overflowed
    }
}

impl<E> Future for LocalTaskGroup<E> {
    type Output = Result<(), TaskGroupError<E>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.state.borrow_mut().waker = Some(context.waker().clone());
        if self.overflowed() {
            self.close();
            return Poll::Ready(Err(TaskGroupError::CapacityExceeded));
        }
        if let Some(root) = self.root.as_mut() {
            match root.as_mut().poll(context) {
                Poll::Ready(Ok(())) => self.root = None,
                Poll::Ready(Err(error)) => {
                    self.close();
                    return Poll::Ready(Err(TaskGroupError::Root(error)));
                }
                Poll::Pending => {}
            }
        }

        // Poll each admitted child at most once per owner turn. New submissions
        // wake this task and are deferred instead of extending the turn forever.
        let budget = self.state.borrow().queued.len();
        for _ in 0..budget {
            if self.overflowed() {
                break;
            }
            let mut task = self
                .state
                .borrow_mut()
                .queued
                .pop_front()
                .expect("queued child");
            if task.as_mut().poll(context).is_pending() {
                self.state.borrow_mut().queued.push_back(task);
            } else {
                self.state.borrow_mut().active -= 1;
            }
        }
        if self.overflowed() {
            self.close();
            Poll::Ready(Err(TaskGroupError::CapacityExceeded))
        } else if self.root.is_none() && self.state.borrow().active == 0 {
            self.close();
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

impl<E> Drop for LocalTaskGroup<E> {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
#[path = "task_group_test.rs"]
mod tests;
