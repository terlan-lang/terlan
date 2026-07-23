//! Full-cycle protocol-reactor completion evidence for generated actors.

use std::fs;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::vm::fixed_scheduler_telemetry::VmFixedSchedulerEventKind;
use crate::runtime::vm::protocol_task_executor::{
    next_protocol_task_route, with_protocol_task_for_test,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::runtime::vm::ReplValue;

use super::invocation_test::{runtime_with_shards, waiting};
use super::AotHandlerInvocationStep;

/// Proves a protocol owner publishes typed data before fixed-owner execution.
#[test]
fn protocol_reactor_completion_resumes_only_through_actor_owner() {
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
        AotHandlerInvocationStep::Complete(ReplValue::Tuple(_))
    ));

    let telemetry = &runtime.generation.shards[scheduler.index()];
    assert_eq!(telemetry.telemetry_snapshot().io_completions, 1);
    let trace = telemetry.telemetry_trace().expect("scheduler trace");
    let published = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::IoCompletionPublished)
        .expect("I/O completion publication");
    let dispatched = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::IoCompletionDispatched)
        .expect("I/O completion dispatch");
    let completed = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::Completed)
        .expect("generated actor completion");
    assert!(published < dispatched);
    assert!(dispatched < completed);

    fs::remove_dir_all(root).expect("cleanup protocol completion fixture");
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
