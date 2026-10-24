//! Shared driver for source callbacks that retain protocol task authority.

use crate::runtime::vm::protocol_task_executor::{
    current_protocol_task_route, next_protocol_task_route, with_protocol_task_for_test,
    VmProtocolTaskRoute,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

fn owner() -> VmProtocolTaskRoute {
    current_protocol_task_route().unwrap_or_else(|| {
        let scheduler = VmSchedulerTopology::new(1)
            .unwrap()
            .schedulers()
            .next()
            .unwrap();
        next_protocol_task_route(scheduler).unwrap()
    })
}

pub(super) fn with_owner<T>(operation: impl FnOnce() -> T) -> T {
    with_protocol_task_for_test(owner(), operation)
}

pub(super) fn cancel_after_pending<T: std::fmt::Debug>(future: impl Future<Output = T>) {
    let route = owner();
    let mut future = Box::pin(future);
    with_protocol_task_for_test(route, || {
        let state = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        assert!(
            state.is_pending(),
            "expected a retained source wait, got {state:?}"
        );
        drop(future);
    });
}

pub(super) fn complete<T>(future: impl Future<Output = T>) -> T {
    let route = owner();
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match with_protocol_task_for_test(route, || future.as_mut().poll(&mut context)) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                if Instant::now() >= deadline {
                    with_protocol_task_for_test(route, || drop(future));
                    panic!("source protocol callback did not finish");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}
