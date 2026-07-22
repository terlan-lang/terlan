use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::actor::{VmActorReceive, VmActorRuntime};
use crate::runtime::vm::process::VmProcessSource;
use crate::runtime::vm::pure_native::{
    dispatch_transition_operation, validate_transition_arguments,
};
use crate::runtime::vm::ReplValue;

#[test]
fn native_send_transition_dispatches_through_vm_mailbox_ownership() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "sender", 0));
    let recipient = actors.spawn_root(VmProcessSource::new("native.Test", "recipient", 0));
    actors
        .park_native_continuation(owner.as_u64(), 67, 71)
        .expect("native continuation should park");

    dispatch_transition_operation(
        &mut actors,
        owner.as_u64(),
        67,
        71,
        &TvmTransitionOperation::Send,
        &[recipient.as_u64() as i64, 73],
    )
    .expect("Send transition should dispatch");

    assert_eq!(actors.pending_native_continuation_count(), 0);
    assert!(matches!(
        actors
            .receive_next_or_block(recipient)
            .expect("recipient should receive"),
        VmActorReceive::Message(message)
            if message.sender == owner && message.payload == ReplValue::Int(73)
    ));
}

#[test]
fn native_receive_transition_dispatches_typed_mailbox_result() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "receiver", 0));
    actors
        .send(owner, owner, ReplValue::Int(79))
        .expect("queue native receive payload");
    actors
        .park_native_continuation(owner.as_u64(), 83, 89)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            83,
            89,
            &TvmTransitionOperation::Receive,
            &[],
        )
        .expect("Receive transition should dispatch"),
        Some(vec![79])
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
}

#[test]
fn native_spawn_transition_dispatches_vm_owned_child_identity() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "spawner", 0));
    actors
        .park_native_continuation(owner.as_u64(), 97, 101)
        .expect("native continuation should park");

    let resume = dispatch_transition_operation(
        &mut actors,
        owner.as_u64(),
        97,
        101,
        &TvmTransitionOperation::Spawn,
        &[103],
    )
    .expect("Spawn transition should dispatch")
    .expect("Spawn resumes immediately");
    assert_eq!(resume.len(), 1);
    let child = crate::runtime::vm::process::VmProcessId::from_raw_for_test(resume[0] as u64);
    assert!(actors.is_alive(child));
    assert_eq!(
        actors.processes().get(child).expect("spawned child").parent,
        Some(owner)
    );
}

#[test]
fn native_timer_transition_dispatches_vm_owned_deadline() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "sleeper", 0));
    actors
        .park_native_continuation(owner.as_u64(), 107, 109)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            107,
            109,
            &TvmTransitionOperation::Timer,
            &[3],
        )
        .expect("Timer transition should dispatch"),
        Some(Vec::new())
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
    assert_eq!(
        actors.processes().get(owner).expect("timer owner").state,
        crate::runtime::vm::process::VmProcessState::Runnable
    );
}

#[test]
fn native_link_transition_dispatches_vm_owned_failure_relationship() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "linker", 0));
    let peer = actors.spawn_root(VmProcessSource::new("native.Test", "peer", 0));
    actors
        .park_native_continuation(owner.as_u64(), 113, 127)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            113,
            127,
            &TvmTransitionOperation::Link,
            &[peer.as_u64() as i64],
        )
        .expect("Link transition should dispatch"),
        Some(Vec::new())
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
    assert_eq!(
        actors
            .failure_snapshot(owner)
            .expect("link relationship")
            .links,
        [peer]
    );
}

#[test]
fn native_monitor_transition_dispatches_vm_owned_reference() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "monitor", 0));
    let target = actors.spawn_root(VmProcessSource::new("native.Test", "target", 0));
    actors
        .park_native_continuation(owner.as_u64(), 131, 137)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            131,
            137,
            &TvmTransitionOperation::Monitor,
            &[target.as_u64() as i64],
        )
        .expect("Monitor transition should dispatch"),
        Some(vec![1])
    );
    let snapshot = actors
        .failure_snapshot(owner)
        .expect("monitor relationship");
    assert_eq!(snapshot.monitoring.len(), 1);
    assert_eq!(snapshot.monitoring[0].peer, target);
}

#[test]
fn native_resource_transition_dispatches_vm_owned_identity() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "resource", 0));
    actors
        .park_native_continuation(owner.as_u64(), 139, 149)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            139,
            149,
            &TvmTransitionOperation::Resource,
            &[7],
        )
        .expect("Resource transition should dispatch"),
        Some(vec![1])
    );
    let resources = actors.resource_snapshots();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].owner, owner);
    assert_eq!(resources[0].label, "tag_7");
}

#[test]
fn native_cancellation_transition_dispatches_scheduler_owned_request() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "canceller", 0));
    let target = actors.spawn_root(VmProcessSource::new("native.Test", "target", 0));
    actors
        .park_native_continuation(owner.as_u64(), 151, 157)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            151,
            157,
            &TvmTransitionOperation::Cancellation,
            &[target.as_u64() as i64],
        )
        .expect("Cancellation transition should dispatch"),
        Some(Vec::new())
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
    assert!(
        actors
            .processes()
            .get(target)
            .expect("cancellation target")
            .cancellation_requested
    );
}

#[test]
fn native_failure_transition_dispatches_vm_owned_abnormal_exit() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "failure", 0));
    actors
        .park_native_continuation(owner.as_u64(), 163, 167)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            163,
            167,
            &TvmTransitionOperation::Failure,
            &[7],
        )
        .expect("Failure transition should dispatch"),
        Some(Vec::new())
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
    assert_eq!(
        actors.processes().get(owner).expect("failure owner").state,
        crate::runtime::vm::process::VmProcessState::Exited(
            crate::runtime::vm::process::VmExitReason::Error("native_failure:7".to_string())
        )
    );
}

#[test]
fn native_scheduling_transition_dispatches_vm_owned_reclassification() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("native.Test", "scheduling", 0));
    actors
        .park_native_continuation(owner.as_u64(), 173, 179)
        .expect("native continuation should park");

    assert_eq!(
        dispatch_transition_operation(
            &mut actors,
            owner.as_u64(),
            173,
            179,
            &TvmTransitionOperation::Scheduling,
            &[1],
        )
        .expect("Scheduling transition should dispatch"),
        Some(Vec::new())
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
    assert_eq!(
        actors
            .processes()
            .get(owner)
            .expect("scheduled owner")
            .state,
        crate::runtime::vm::process::VmProcessState::Runnable
    );
}

#[test]
fn native_transition_argument_contract_accepts_active_typed_operations() {
    validate_transition_arguments(&TvmTransitionOperation::Yield, &[])
        .expect("Yield has no operation arguments");
    validate_transition_arguments(&TvmTransitionOperation::Send, &[7, -11])
        .expect("Send carries a positive recipient and one Int payload");
    validate_transition_arguments(&TvmTransitionOperation::Receive, &[])
        .expect("Receive resolves its owner without operation arguments");
    validate_transition_arguments(&TvmTransitionOperation::Spawn, &[7])
        .expect("Spawn carries one positive native entry identity");
    validate_transition_arguments(&TvmTransitionOperation::Timer, &[3])
        .expect("Timer carries one positive delay");
    validate_transition_arguments(&TvmTransitionOperation::Link, &[7])
        .expect("Link carries one positive peer identity");
    validate_transition_arguments(&TvmTransitionOperation::Monitor, &[7])
        .expect("Monitor carries one positive target identity");
    validate_transition_arguments(&TvmTransitionOperation::Resource, &[7])
        .expect("Resource carries one positive kind tag");
    validate_transition_arguments(&TvmTransitionOperation::Cancellation, &[7])
        .expect("Cancellation carries one positive target identity");
    validate_transition_arguments(&TvmTransitionOperation::Failure, &[7])
        .expect("Failure carries one positive scalar code");
    for class_tag in 1..=3 {
        validate_transition_arguments(&TvmTransitionOperation::Scheduling, &[class_tag])
            .expect("Scheduling accepts each bounded class tag");
    }
}

#[test]
fn native_transition_argument_contract_rejects_malformed_send_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Send, &[7])
            .expect_err("missing payload must fail"),
        "error[pure_native_transition_arguments]: Send transition requires 2 scalar or 5 typed arguments, received 1 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Send, &[0, 11])
            .expect_err("zero recipient must fail"),
        "error[pure_native_transition_arguments]: Send recipient must be a positive process identity"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Send, &[-1, 11])
            .expect_err("negative recipient must fail"),
        "error[pure_native_transition_arguments]: Send recipient must be a positive process identity"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_receive_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Receive, &[1])
            .expect_err("Receive arguments must fail"),
        "error[pure_native_transition_arguments]: Receive transition requires 0 scalar or 3 typed arguments, received 1 arguments"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_spawn_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Spawn, &[])
            .expect_err("missing Spawn entry must fail"),
        "error[pure_native_transition_arguments]: Spawn transition requires one native entry identity, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Spawn, &[0])
            .expect_err("zero Spawn entry must fail"),
        "error[pure_native_transition_arguments]: Spawn entry must be a positive native identity"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_timer_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Timer, &[])
            .expect_err("missing Timer delay must fail"),
        "error[pure_native_transition_arguments]: Timer transition requires one positive delay, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Timer, &[0])
            .expect_err("zero Timer delay must fail"),
        "error[pure_native_transition_arguments]: Timer delay must be positive"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_link_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Link, &[])
            .expect_err("missing Link peer must fail"),
        "error[pure_native_transition_arguments]: Link transition requires one positive peer identity, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Link, &[0])
            .expect_err("zero Link peer must fail"),
        "error[pure_native_transition_arguments]: Link peer must be a positive process identity"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_monitor_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Monitor, &[])
            .expect_err("missing Monitor target must fail"),
        "error[pure_native_transition_arguments]: Monitor transition requires one positive target identity, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Monitor, &[0])
            .expect_err("zero Monitor target must fail"),
        "error[pure_native_transition_arguments]: Monitor target must be a positive process identity"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_resource_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Resource, &[])
            .expect_err("missing Resource tag must fail"),
        "error[pure_native_transition_arguments]: Resource transition requires one positive kind tag, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Resource, &[0])
            .expect_err("zero Resource tag must fail"),
        "error[pure_native_transition_arguments]: Resource kind tag must be positive"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_cancellation_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Cancellation, &[])
            .expect_err("missing Cancellation target must fail"),
        "error[pure_native_transition_arguments]: Cancellation transition requires one positive target identity, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Cancellation, &[0])
            .expect_err("zero Cancellation target must fail"),
        "error[pure_native_transition_arguments]: Cancellation target must be a positive process identity"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_failure_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Failure, &[])
            .expect_err("missing Failure code must fail"),
        "error[pure_native_transition_arguments]: Failure transition requires one positive failure code, received 0 arguments"
    );
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Failure, &[0])
            .expect_err("zero Failure code must fail"),
        "error[pure_native_transition_arguments]: Failure code must be positive"
    );
}

#[test]
fn native_transition_argument_contract_rejects_malformed_scheduling_before_parking() {
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Scheduling, &[])
            .expect_err("missing Scheduling class must fail"),
        "error[pure_native_transition_arguments]: Scheduling transition requires one class tag, received 0 arguments"
    );
    for class_tag in [0, 4, -1] {
        assert_eq!(
            validate_transition_arguments(&TvmTransitionOperation::Scheduling, &[class_tag])
                .expect_err("out-of-range Scheduling class must fail"),
            "error[pure_native_transition_arguments]: Scheduling class tag must be 1, 2, or 3"
        );
    }
    assert_eq!(
        validate_transition_arguments(&TvmTransitionOperation::Scheduling, &[1, 2])
            .expect_err("extra Scheduling class must fail"),
        "error[pure_native_transition_arguments]: Scheduling transition requires one class tag, received 2 arguments"
    );
}
