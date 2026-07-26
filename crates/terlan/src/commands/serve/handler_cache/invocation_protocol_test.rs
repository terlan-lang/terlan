//! Full-cycle protocol-reactor completion evidence for generated actors.

use std::fs;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::vm::protocol_task_executor::{
    next_protocol_task_route, with_protocol_task_for_test,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::runtime::vm::ReplValue;

use super::invocation_test::{runtime_with_shards, waiting};
use super::AotHandlerInvocationStep;

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

struct SignalWake(AtomicBool);

impl Wake for SignalWake {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Release);
    }
}

/// Proves entry and typed completion mutate the protocol owner's local shard.
#[test]
fn protocol_reactor_completion_stays_on_protocol_owner() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let scheduler = VmSchedulerTopology::new(2)
        .expect("two-scheduler topology")
        .schedulers()
        .nth(1)
        .expect("secondary scheduler");
    let protocol = next_protocol_task_route(scheduler).expect("protocol task route");
    let invocation = with_protocol_task_for_test(protocol, || waiting(&runtime));

    assert_eq!(protocol.scheduler(), scheduler);
    assert_eq!(invocation.route.scheduler(), scheduler);

    let wake = invocation
        .wait()
        .expect("protocol typed wait")
        .wake(ReplValue::String("reactor-ready".to_string()));
    let result = with_protocol_task_for_test(protocol, || invocation.resume(wake))
        .expect("publish protocol completion");
    assert!(matches!(
        result,
        AotHandlerInvocationStep::Complete(ReplValue::String(value)) if value == "reactor-ready"
    ));
    assert!(
        runtime.generation.shards[scheduler.index()]
            .initialized()
            .is_none(),
        "suspended protocol work must not start a duplicate AOT owner thread"
    );

    fs::remove_dir_all(root).expect("cleanup protocol completion fixture");
}

/// Proves timer parking uses the protocol task deadline rather than the
/// dedicated shard thread or an immediate synthetic resume.
#[test]
fn protocol_reactor_timer_resumes_after_its_owner_deadline() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let scheduler = VmSchedulerTopology::new(1)
        .expect("one-scheduler topology")
        .schedulers()
        .next()
        .expect("primary scheduler");
    let protocol = next_protocol_task_route(scheduler).expect("protocol task route");
    let timer = with_protocol_task_for_test(protocol, || {
        match runtime
            .begin_request_invocation("app.AsyncHandler", "timer_then_true", Vec::new())
            .expect("park protocol timer")
        {
            AotHandlerInvocationStep::TimerWaiting(timer) => timer,
            step => panic!("expected protocol timer, found {step:?}"),
        }
    });
    let mut future = Box::pin(timer.resume_at_deadline());
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(
        with_protocol_task_for_test(protocol, || future.as_mut().poll(&mut context)),
        Poll::Pending
    ));
    thread::sleep(Duration::from_millis(25));
    let completed = with_protocol_task_for_test(protocol, || future.as_mut().poll(&mut context));
    assert!(matches!(
        completed,
        Poll::Ready(Ok(AotHandlerInvocationStep::Complete(ReplValue::Bool(
            true
        ))))
    ));
    fs::remove_dir_all(root).expect("cleanup protocol timer fixture");
}

/// Exercises the packaged worker through the protocol-local pump and requires
/// its background transport to wake the exact parked task.
#[test]
#[ignore = "requires a built terlan-native-worker executable"]
fn protocol_reactor_capability_worker_wakes_and_resumes_exact_actor() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let scheduler = VmSchedulerTopology::new(1)
        .expect("one-scheduler topology")
        .schedulers()
        .next()
        .expect("primary scheduler");
    let protocol = next_protocol_task_route(scheduler).expect("protocol task route");
    let invocation = with_protocol_task_for_test(protocol, || {
        match runtime
            .begin_request_invocation(
                "app.AsyncHandler",
                "file_exists",
                vec![ReplValue::String(
                    "/tmp/terlan-protocol-capability".to_string(),
                )],
            )
            .expect("park protocol capability")
        {
            AotHandlerInvocationStep::CapabilityWaiting(invocation) => invocation,
            step => panic!("expected protocol capability, found {step:?}"),
        }
    });
    let mut future = Box::pin(invocation.resume_from_worker());
    let signal = Arc::new(SignalWake(AtomicBool::new(false)));
    let waker = Waker::from(Arc::clone(&signal));
    let mut context = Context::from_waker(&waker);
    let mut outcome = with_protocol_task_for_test(protocol, || future.as_mut().poll(&mut context));
    if matches!(outcome, Poll::Pending) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !signal.0.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            signal.0.load(Ordering::Acquire),
            "worker transport did not wake the protocol task"
        );
        outcome = with_protocol_task_for_test(protocol, || future.as_mut().poll(&mut context));
    }
    assert!(matches!(
        outcome,
        Poll::Ready(Ok(AotHandlerInvocationStep::Complete(ReplValue::Bool(_))))
    ));
    fs::remove_dir_all(root).expect("cleanup protocol capability fixture");
}

/// Proves another connection on the same reactor cannot reuse completion data.
#[test]
fn protocol_reactor_rejects_same_scheduler_foreign_connection() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let scheduler = VmSchedulerTopology::new(1)
        .expect("one-scheduler topology")
        .schedulers()
        .next()
        .expect("primary scheduler");
    let owner = next_protocol_task_route(scheduler).expect("owner protocol route");
    let foreign = next_protocol_task_route(scheduler).expect("foreign protocol route");
    let invocation = with_protocol_task_for_test(owner, || waiting(&runtime));
    let wake = invocation
        .wait()
        .expect("protocol typed wait")
        .wake(ReplValue::String("foreign".to_string()));

    let error = with_protocol_task_for_test(foreign, || invocation.resume(wake))
        .expect_err("foreign connection completion must fail");
    assert!(
        error.contains("error[vm.protocol_completion_owner]") && error.contains("cannot complete"),
        "{error}"
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    while runtime.generation.active_actors[0].load(Ordering::Acquire) != 0
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        runtime.generation.active_actors[0].load(Ordering::Acquire),
        0,
        "rejected completion must cancel its parked actor"
    );
    fs::remove_dir_all(root).expect("cleanup foreign protocol fixture");
}
