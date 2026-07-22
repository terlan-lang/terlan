use super::*;

fn valid_byte_producer(name: &str) -> VmExternalIoRuntimePlan {
    VmExternalIoRuntimePlan::new(
        name,
        VmExternalIoRuntimeRole::ByteProducer,
        VmExternalIoSchedulingPolicy::VmWakeProducerOnly,
    )
    .with_typed_vm_wakeups(true)
    .with_bounded_backpressure(true)
    .with_support_bundle_replay(true)
}

#[test]
fn external_io_runtime_boundary_accepts_vm_wakeup_only_byte_producer() {
    let boundary = VmExternalIoRuntimeBoundary::validate(valid_byte_producer("rustls-tcp-reader"))
        .expect("valid boundary");

    assert_eq!(boundary.name, "rustls-tcp-reader");
    assert_eq!(boundary.role, VmExternalIoRuntimeRole::ByteProducer);
}

#[test]
fn external_io_runtime_boundary_rejects_scheduling_and_hidden_continuations() {
    let scheduling_owner = VmExternalIoRuntimePlan::new(
        "foreign-event-loop",
        VmExternalIoRuntimeRole::ByteProducer,
        VmExternalIoSchedulingPolicy::OwnsActorScheduling,
    )
    .with_typed_vm_wakeups(true)
    .with_bounded_backpressure(true)
    .with_support_bundle_replay(true);
    let continuation_owner = VmExternalIoRuntimePlan::new(
        "foreign-continuation-store",
        VmExternalIoRuntimeRole::ByteConsumer,
        VmExternalIoSchedulingPolicy::OwnsProcessContinuations,
    )
    .with_typed_vm_wakeups(true)
    .with_bounded_backpressure(true)
    .with_support_bundle_replay(true);
    let direct_scheduler = VmExternalIoRuntimePlan::new(
        "foreign-scheduler-waker",
        VmExternalIoRuntimeRole::NameResolver,
        VmExternalIoSchedulingPolicy::DirectSchedulerAccess,
    )
    .with_typed_vm_wakeups(true)
    .with_bounded_backpressure(true)
    .with_support_bundle_replay(true);

    let scheduling_error =
        VmExternalIoRuntimeBoundary::validate(scheduling_owner).expect_err("schedule owner");
    let continuation_error =
        VmExternalIoRuntimeBoundary::validate(continuation_owner).expect_err("continuation owner");
    let scheduler_error =
        VmExternalIoRuntimeBoundary::validate(direct_scheduler).expect_err("direct scheduler");

    assert!(scheduling_error.contains("cannot own actor scheduling"));
    assert!(continuation_error.contains("cannot own process continuations"));
    assert!(scheduler_error.contains("cannot call VM scheduler wake APIs directly"));
}

#[test]
fn external_io_runtime_boundary_rejects_unreplayable_or_unbounded_helpers() {
    let missing_wakeups = VmExternalIoRuntimePlan::new(
        "silent-helper",
        VmExternalIoRuntimeRole::CryptoHandshake,
        VmExternalIoSchedulingPolicy::VmWakeProducerOnly,
    )
    .with_bounded_backpressure(true)
    .with_support_bundle_replay(true);
    let unbounded = valid_byte_producer("unbounded-helper").with_bounded_backpressure(false);
    let unreplayable = valid_byte_producer("unreplayable-helper").with_support_bundle_replay(false);

    let wakeup_error =
        VmExternalIoRuntimeBoundary::validate(missing_wakeups).expect_err("missing wakeups");
    let backpressure_error =
        VmExternalIoRuntimeBoundary::validate(unbounded).expect_err("unbounded helper");
    let replay_error =
        VmExternalIoRuntimeBoundary::validate(unreplayable).expect_err("unreplayable helper");

    assert!(wakeup_error.contains("must emit typed VM wakeups"));
    assert!(backpressure_error.contains("must enforce bounded backpressure"));
    assert!(replay_error.contains("must record support-bundle replay metadata"));
}
