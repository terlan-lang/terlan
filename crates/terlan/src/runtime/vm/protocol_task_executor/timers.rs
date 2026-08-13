//! Owner-local deadlines for VM protocol task futures.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

thread_local! {
    static PROTOCOL_TIMERS: RefCell<ProtocolTimerRegistry> =
        RefCell::new(ProtocolTimerRegistry::default());
}

#[derive(Default)]
struct ProtocolTimerRegistry {
    next_id: u64,
    deadlines: BTreeMap<(Instant, u64), Waker>,
}

impl ProtocolTimerRegistry {
    fn register(
        &mut self,
        deadline: Instant,
        prior: Option<(Instant, u64)>,
        waker: &Waker,
    ) -> (Instant, u64) {
        if let Some(key) = prior {
            if key.0 == deadline {
                match self.deadlines.get_mut(&key) {
                    Some(registered) if !registered.will_wake(waker) => {
                        *registered = waker.clone();
                    }
                    Some(_) => {}
                    None => {
                        self.deadlines.insert(key, waker.clone());
                    }
                }
                return key;
            }
            self.deadlines.remove(&key);
        }
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let key = (deadline, self.next_id);
        self.deadlines.insert(key, waker.clone());
        key
    }

    fn remove(&mut self, key: (Instant, u64)) {
        self.deadlines.remove(&key);
    }

    fn take_due(&mut self, now: Instant) -> Vec<Waker> {
        let split = (now, u64::MAX);
        let pending = self.deadlines.split_off(&split);
        let due = std::mem::replace(&mut self.deadlines, pending);
        due.into_values().collect()
    }

    fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.deadlines
            .first_key_value()
            .map(|((deadline, _), _)| deadline.saturating_duration_since(now))
    }
}

/// Future resolved by the same VM protocol owner that polls the connection.
pub(crate) struct VmProtocolSleep {
    deadline: Instant,
    registration: Option<(Instant, u64)>,
}

impl Future for VmProtocolSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if Instant::now() >= self.deadline {
            if let Some(key) = self.registration.take() {
                PROTOCOL_TIMERS.with(|timers| timers.borrow_mut().remove(key));
            }
            return Poll::Ready(());
        }
        let registration = PROTOCOL_TIMERS.with(|timers| {
            timers
                .borrow_mut()
                .register(self.deadline, self.registration, context.waker())
        });
        self.registration = Some(registration);
        Poll::Pending
    }
}

impl Drop for VmProtocolSleep {
    fn drop(&mut self) {
        if let Some(key) = self.registration.take() {
            PROTOCOL_TIMERS.with(|timers| timers.borrow_mut().remove(key));
        }
    }
}

/// Registers one deadline without introducing a foreign timer runtime.
pub(crate) fn protocol_sleep_until(deadline: Instant) -> VmProtocolSleep {
    VmProtocolSleep {
        deadline,
        registration: None,
    }
}

pub(super) fn wake_due_protocol_timers() {
    let due = PROTOCOL_TIMERS.with(|timers| timers.borrow_mut().take_due(Instant::now()));
    for waker in due {
        waker.wake();
    }
}

pub(super) fn next_protocol_timer_timeout(now: Instant) -> Option<Duration> {
    PROTOCOL_TIMERS.with(|timers| timers.borrow().next_timeout(now))
}

#[cfg(test)]
#[path = "timers_test.rs"]
#[cfg(test)]
mod timers_test;
