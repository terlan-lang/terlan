use super::super::process::{VmExitReason, VmProcessResumeState, VmProcessSource, VmProcessState};
use super::super::ReplValue;
use super::{VmActorReceive, VmActorRuntime};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn native_failure_uses_vm_exit_propagation_monitoring_and_cleanup() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-failure"));
    let linked = runtime.spawn_root(source("linked-peer"));
    let watcher = runtime.spawn_root(source("watcher"));

    runtime
        .park_native_continuation(owner.as_u64(), 239, 241)
        .expect("resource continuation should park");
    runtime
        .service_native_resource(owner.as_u64(), 239, 241, 7)
        .expect("owner resource should register");
    runtime.link_actors(owner, linked).expect("link actors");
    let monitor_ref = runtime
        .monitor_actor(watcher, owner)
        .expect("monitor native failure owner");
    runtime
        .park_native_continuation(owner.as_u64(), 251, 257)
        .expect("failure continuation should park");

    assert_eq!(
        runtime
            .service_native_failure(owner.as_u64(), 251, 257, 7)
            .expect("native failure should terminate through failure runtime"),
        ["resource:1"]
    );
    let expected_reason = VmExitReason::Error("native_failure:7".to_string());
    assert_eq!(
        runtime.processes().get(owner).expect("failed owner").state,
        VmProcessState::Exited(expected_reason.clone())
    );
    assert_eq!(
        runtime.processes().get(linked).expect("linked peer").state,
        VmProcessState::Exited(expected_reason.clone())
    );
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert!(runtime.resource_snapshots().is_empty());

    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(watcher)
        .expect("watcher should receive DOWN")
    else {
        panic!("native failure must deliver a monitor DOWN message");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(owner.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("native_failure:7".to_string()),
            ]),
        ])
    );
}

#[test]
fn native_failure_rejects_invalid_authority_and_code_before_exit() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-failure"));
    let foreign = runtime.spawn_root(source("foreign-failure"));
    runtime
        .park_native_continuation(owner.as_u64(), 263, 269)
        .expect("failure continuation should park");

    assert!(runtime
        .service_native_failure(foreign.as_u64(), 263, 269, 7)
        .expect_err("foreign failure owner must fail")
        .contains("is owned by process"));
    assert_eq!(
        runtime
            .service_native_failure(owner.as_u64(), 263, 269, 0)
            .expect_err("zero failure code must fail"),
        "native failure code must be positive"
    );
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("live owner").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(
        runtime
            .processes()
            .get(foreign)
            .expect("live foreign")
            .state,
        VmProcessState::Runnable
    );
}
