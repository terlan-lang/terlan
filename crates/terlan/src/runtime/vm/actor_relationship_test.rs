use super::super::failure::is_monitor_down_message;
use super::super::process::{VmExitReason, VmProcessId, VmProcessSource};
use super::super::ReplValue;
use super::{VmActorDemonitorOptions, VmActorReceive, VmActorRuntime, VmActorSpawnOptions};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn actor_links_are_idempotent_symmetric_and_validate_before_mutation() {
    let mut runtime = VmActorRuntime::default();
    let left = runtime.spawn_root(source("left"));
    let right = runtime.spawn_root(source("right"));

    assert!(runtime.link_actors(left, right).expect("first link"));
    assert!(!runtime.link_actors(right, left).expect("duplicate link"));
    assert_eq!(
        runtime
            .failure_snapshot(left)
            .expect("left relationships")
            .links,
        [right]
    );
    assert_eq!(
        runtime
            .failure_snapshot(right)
            .expect("right relationships")
            .links,
        [left]
    );
    assert!(runtime.unlink_actors(right, left).expect("remove link"));
    assert!(!runtime.unlink_actors(left, right).expect("missing link"));

    assert_eq!(
        runtime
            .link_actors(left, left)
            .expect_err("self-link must fail"),
        "cannot link process 1 to itself"
    );
    assert_eq!(
        runtime
            .unlink_actors(left, left)
            .expect_err("self-unlink must fail"),
        "cannot unlink process 1 from itself"
    );
    let missing = VmProcessId::from_raw_for_test(99);
    assert_eq!(
        runtime
            .unlink_actors(left, missing)
            .expect_err("missing peer must fail"),
        "cannot unlink missing process 99"
    );
    runtime
        .exit_actor(right, VmExitReason::Normal)
        .expect("right exit");
    assert_eq!(
        runtime
            .link_actors(left, right)
            .expect_err("exited peer must fail"),
        "cannot link exited process 2"
    );
}

/// Replaces OTP's `naughty_child.erl` supervisor regression with VM-owned
/// behavior: a linked child may unlink before termination without leaving its
/// former parent unable to make mailbox progress.
#[test]
fn actor_unlinked_child_termination_preserves_parent_mailbox_progress() {
    let mut runtime = VmActorRuntime::default();
    let supervisor = runtime.spawn_root(source("supervisor"));
    let observer = runtime.spawn_root(source("observer"));
    let child = runtime
        .spawn_child_with_options(
            supervisor,
            source("naughty-child"),
            VmActorSpawnOptions::default().linked(),
        )
        .expect("linked child spawn")
        .pid;

    assert_eq!(
        runtime
            .failure_snapshot(supervisor)
            .expect("supervisor relationships")
            .links,
        [child]
    );
    assert!(runtime
        .unlink_actors(child, supervisor)
        .expect("child unlinks from supervisor"));
    assert!(runtime
        .failure_snapshot(supervisor)
        .expect("supervisor relationships after unlink")
        .links
        .is_empty());

    runtime
        .exit_actor(child, VmExitReason::Killed)
        .expect("terminate unlinked child");
    assert!(runtime.is_alive(supervisor));
    runtime
        .send(
            observer,
            supervisor,
            ReplValue::Atom("supervisor_progress_probe".to_string()),
        )
        .expect("queue progress probe");

    let VmActorReceive::Message(probe) = runtime
        .receive_next_or_block(supervisor)
        .expect("supervisor receives after unlinked child termination")
    else {
        panic!("supervisor must remain runnable after unlinked child termination");
    };
    assert_eq!(
        probe.payload,
        ReplValue::Atom("supervisor_progress_probe".to_string())
    );
}

#[test]
fn actor_monitors_enforce_observer_ownership_and_allocate_after_validation() {
    let mut runtime = VmActorRuntime::default();
    let first_observer = runtime.spawn_root(source("first-observer"));
    let second_observer = runtime.spawn_root(source("second-observer"));
    let first_target = runtime.spawn_root(source("first-target"));
    let second_target = runtime.spawn_root(source("second-target"));
    let monitor_ref = runtime
        .monitor_actor(first_observer, first_target)
        .expect("first monitor");

    assert_eq!(monitor_ref.as_u64(), 1);
    assert_eq!(
        runtime
            .demonitor_actor(
                second_observer,
                monitor_ref.clone(),
                VmActorDemonitorOptions::default(),
            )
            .expect_err("another observer cannot remove monitor"),
        "monitor reference 1 belongs to process 1, not process 2"
    );
    assert_eq!(
        runtime
            .failure_snapshot(first_observer)
            .expect("relationships")
            .monitoring
            .len(),
        1
    );
    let removed = runtime
        .demonitor_actor(
            first_observer,
            monitor_ref.clone(),
            VmActorDemonitorOptions::default(),
        )
        .expect("owner demonitor");
    assert!(removed.removed);
    assert!(!removed.flushed_down);
    assert!(
        !runtime
            .demonitor_actor(
                first_observer,
                monitor_ref,
                VmActorDemonitorOptions::default(),
            )
            .expect("idempotent demonitor")
            .removed
    );

    let missing = VmProcessId::from_raw_for_test(99);
    assert_eq!(
        runtime
            .monitor_actor(first_observer, missing)
            .expect_err("missing target"),
        "cannot monitor missing process 99"
    );
    let next_ref = runtime
        .monitor_actor(first_observer, second_target)
        .expect("validated monitor");
    assert_eq!(next_ref.as_u64(), 2);
    assert_eq!(
        runtime
            .demonitor_actor(missing, next_ref, VmActorDemonitorOptions::default(),)
            .expect_err("missing observer"),
        "cannot demonitor from missing process 99"
    );
}

#[test]
fn actor_demonitor_flushes_only_its_stale_down_message() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let first_target = runtime.spawn_root(source("first-target"));
    let second_target = runtime.spawn_root(source("second-target"));
    let first_ref = runtime
        .monitor_actor(observer, first_target)
        .expect("first monitor");
    let second_ref = runtime
        .monitor_actor(observer, second_target)
        .expect("second monitor");
    runtime
        .send(
            first_target,
            observer,
            ReplValue::String("ordinary".to_string()),
        )
        .expect("ordinary message");
    runtime
        .exit_actor(first_target, VmExitReason::Killed)
        .expect("first target exit");
    runtime
        .exit_actor(second_target, VmExitReason::Normal)
        .expect("second target exit");

    let result = runtime
        .demonitor_actor(
            observer,
            second_ref,
            VmActorDemonitorOptions::default().flush_down(),
        )
        .expect("flush stale completion");
    assert!(!result.removed);
    assert!(result.flushed_down);

    let VmActorReceive::Message(ordinary) = runtime
        .receive_next_or_block(observer)
        .expect("ordinary receive")
    else {
        panic!("ordinary message must be preserved");
    };
    assert_eq!(ordinary.payload, ReplValue::String("ordinary".to_string()));
    let VmActorReceive::Message(first_down) = runtime
        .receive_next_or_block(observer)
        .expect("first completion receive")
    else {
        panic!("other monitor completion must be preserved");
    };
    assert!(is_monitor_down_message(&first_down.payload, &first_ref));
    assert!(!is_monitor_down_message(
        &ReplValue::Atom("down".to_string()),
        &first_ref
    ));
}

#[test]
fn actor_demonitor_can_remove_active_monitor_without_flushing_other_work() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let target = runtime.spawn_root(source("target"));
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("active monitor");
    runtime
        .send(target, observer, ReplValue::Int(42))
        .expect("ordinary message");

    let result = runtime
        .demonitor_actor(
            observer,
            monitor_ref,
            VmActorDemonitorOptions::default().flush_down(),
        )
        .expect("active demonitor");
    assert!(result.removed);
    assert!(!result.flushed_down);
    runtime
        .exit_actor(target, VmExitReason::Killed)
        .expect("unmonitored target exit");
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(observer)
        .expect("ordinary receive")
    else {
        panic!("ordinary message must remain");
    };
    assert_eq!(message.payload, ReplValue::Int(42));
}

#[test]
fn actor_trap_exit_setting_controls_linked_failure_delivery() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let linked = runtime.spawn_root(source("linked"));

    let enabled = runtime
        .set_actor_trap_exits(observer, true)
        .expect("enable trap exits");
    assert!(!enabled.previous);
    assert!(enabled.current);
    runtime.link_actors(observer, linked).expect("link actors");
    runtime
        .exit_actor(linked, VmExitReason::Killed)
        .expect("linked exit");

    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(observer)
        .expect("exit signal receive")
    else {
        panic!("trapped exit must be delivered");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(linked.as_u64() as i64),
            ReplValue::Atom("killed".to_string()),
        ])
    );
    let disabled = runtime
        .set_actor_trap_exits(observer, false)
        .expect("disable trap exits");
    assert!(disabled.previous);
    assert!(!disabled.current);
    let missing = VmProcessId::from_raw_for_test(99);
    assert_eq!(
        runtime
            .set_actor_trap_exits(missing, true)
            .expect_err("missing actor"),
        "cannot inspect trap exits for missing process 99"
    );
}
