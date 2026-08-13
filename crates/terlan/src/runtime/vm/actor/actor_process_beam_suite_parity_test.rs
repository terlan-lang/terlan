use super::super::super::process::{
    VmExitReason, VmProcessId, VmProcessResumeState, VmProcessSource, VmProcessState,
};
use super::super::super::process_alias::VmProcessAliasOptions;
use super::super::super::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::super::super::ReplValue;
use super::super::actor_exit::VmActorExitSignalOutcome;
use super::super::{VmActorReceive, VmActorRuntime, VmActorSpawnOptions};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.ProcessParity", name, 0)
}

fn receive_payload(runtime: &mut VmActorRuntime, recipient: VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(recipient)
        .expect("process-suite message should be receivable")
    else {
        panic!("process-suite message must be queued");
    };
    message.payload
}

#[test]
fn process_suite_process_info_visibility_and_state_contract() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent"));
    let child = runtime
        .spawn_child(parent, source("child"))
        .expect("spawn child");
    runtime
        .register_name("process.parity.child", child)
        .expect("register child");

    let parent_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, parent);
            VmSchedulerDecision::Block { reductions: 2 }
        })
        .expect("run parent");
    assert_eq!(parent_run.outcome, VmSchedulerOutcome::Blocked);
    let child_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, child);
            VmSchedulerDecision::Block { reductions: 3 }
        })
        .expect("run child");
    assert_eq!(child_run.outcome, VmSchedulerOutcome::Blocked);

    let blocked = runtime
        .process_info_snapshot(child)
        .expect("live child has process info");
    assert_eq!(blocked.pid, child);
    assert_eq!(blocked.parent, Some(parent));
    assert_eq!(blocked.source, source("child"));
    assert_eq!(blocked.state, VmProcessState::Blocked);
    assert_eq!(
        blocked.reductions, 4,
        "registration and the executed slice are both scheduler-accounted"
    );
    assert_eq!(blocked.mailbox_messages, 0);
    assert_eq!(blocked.registered_names, ["process.parity.child"]);
    assert_eq!(blocked.current_location.source, source("child"));
    assert_eq!(blocked.current_stacktrace.len(), 1);

    runtime
        .send(parent, child, ReplValue::Int(17))
        .expect("send wakes child");
    let runnable = runtime
        .process_info_snapshot(child)
        .expect("woken child has process info");
    assert_eq!(runnable.state, VmProcessState::Runnable);
    assert_eq!(runnable.mailbox_messages, 1);

    runtime.suspend(child).expect("suspend child");
    assert_eq!(
        runtime
            .process_info_snapshot(child)
            .expect("suspended child remains visible")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    runtime.resume(child).expect("resume child");
    assert_eq!(receive_payload(&mut runtime, child), ReplValue::Int(17));

    runtime
        .exit_actor(child, VmExitReason::Normal)
        .expect("exit child");
    assert_eq!(runtime.process_info_snapshot(child), None);
    assert_eq!(
        runtime
            .processes()
            .snapshot(child)
            .expect("internal postmortem snapshot remains")
            .state,
        VmProcessState::Exited(VmExitReason::Normal)
    );
    assert_eq!(
        runtime.process_info_snapshot(VmProcessId::from_raw_for_test(404)),
        None
    );
}

#[test]
fn process_suite_live_enumeration_registry_and_pid_churn_contract() {
    let mut runtime = VmActorRuntime::default();
    let survivor = runtime.spawn_root(source("survivor"));
    runtime
        .register_name("process.parity.survivor", survivor)
        .expect("register survivor");
    let mut previous = survivor;

    for generation in 0..1_024 {
        let transient = runtime
            .spawn_child(survivor, source(&format!("transient-{generation}")))
            .expect("spawn transient");
        assert!(transient > previous, "process ids must be monotonic");
        runtime
            .register_name("process.parity.transient", transient)
            .expect("released name is reusable");
        runtime
            .exit_actor(transient, VmExitReason::Normal)
            .expect("exit transient");
        assert_eq!(runtime.lookup_name("process.parity.transient"), None);
        assert_eq!(runtime.process_info_snapshot(transient), None);
        previous = transient;
    }

    let final_child = runtime
        .spawn_child(survivor, source("final-child"))
        .expect("spawn after churn");
    assert!(final_child > previous);
    assert_eq!(runtime.live_process_ids(), [survivor, final_child]);
    assert_eq!(
        runtime.registered_names(),
        ["process.parity.survivor".to_string()]
    );
    assert_eq!(
        runtime
            .process_info_snapshot(final_child)
            .expect("final child is visible")
            .parent,
        Some(survivor)
    );
}

#[test]
fn process_suite_spawn_link_monitor_and_failure_atomicity_contract() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("spawn-parent"));
    runtime
        .set_actor_trap_exits(parent, true)
        .expect("parent traps child exits");
    let options = VmActorSpawnOptions::default().linked().monitored();

    assert_eq!(
        runtime
            .spawn_child_with_options(
                VmProcessId::from_raw_for_test(404),
                source("missing-parent-child"),
                options,
            )
            .expect_err("missing parent cannot spawn"),
        "cannot spawn child from missing process 404"
    );
    let spawned = runtime
        .spawn_child_with_options(parent, source("spawn-child"), options)
        .expect("atomic linked monitored spawn");
    assert_eq!(spawned.pid.as_u64(), 2);
    let monitor_ref = spawned.monitor_ref.expect("spawn monitor reference");
    assert_eq!(monitor_ref.as_u64(), 1);

    let relationships = runtime.failure_snapshot(parent).expect("relationships");
    assert_eq!(relationships.links, [spawned.pid]);
    assert_eq!(relationships.monitoring.len(), 1);
    assert_eq!(relationships.monitoring[0].monitor_ref, monitor_ref);
    assert_eq!(relationships.monitoring[0].peer, spawned.pid);

    runtime
        .exit_actor(spawned.pid, VmExitReason::Error("child-failed".to_string()))
        .expect("child exits");
    assert_eq!(
        receive_payload(&mut runtime, parent),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(spawned.pid.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("child-failed".to_string()),
            ]),
        ])
    );
    assert_eq!(
        receive_payload(&mut runtime, parent),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(spawned.pid.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("child-failed".to_string()),
            ]),
        ])
    );
    let cleaned = runtime
        .failure_snapshot(parent)
        .expect("cleaned relationships");
    assert!(cleaned.links.is_empty());
    assert!(cleaned.monitoring.is_empty());
    assert!(runtime.is_alive(parent));
}

#[test]
fn process_suite_suspended_timer_mailbox_and_resume_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("timer-sender"));
    let recipient = runtime.spawn_root(source("timer-recipient"));
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("empty recipient blocks"),
        VmActorReceive::Blocked
    );
    let before_suspend = runtime
        .send_after(sender, recipient, ReplValue::Int(1), 0, 5)
        .expect("timer created before suspension");
    runtime.suspend(recipient).expect("suspend recipient");
    let during_suspend = runtime
        .send_after(sender, recipient, ReplValue::Int(2), 0, 10)
        .expect("timer created during suspension");

    let first = runtime.advance_actor_timers(5);
    assert!(first.deliveries.iter().any(|delivery| matches!(
        delivery,
        super::super::actor_timer::VmActorTimerDelivery::Delivered { timer_id, .. }
            if *timer_id == before_suspend
    )));
    let second = runtime.advance_actor_timers(10);
    assert!(second.deliveries.iter().any(|delivery| matches!(
        delivery,
        super::super::actor_timer::VmActorTimerDelivery::Delivered { timer_id, .. }
            if *timer_id == during_suspend
    )));
    let suspended = runtime
        .process_info_snapshot(recipient)
        .expect("suspended recipient visible");
    assert_eq!(
        suspended.state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(suspended.mailbox_messages, 2);

    let sender_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, sender);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("sender remains runnable");
    assert_eq!(sender_run.outcome, VmSchedulerOutcome::Blocked);
    let idle = runtime
        .run_next(|_, _| panic!("suspended recipient must not run"))
        .expect("no runnable actors before resume");
    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);

    runtime.resume(recipient).expect("resume recipient");
    assert_eq!(receive_payload(&mut runtime, recipient), ReplValue::Int(1));
    assert_eq!(receive_payload(&mut runtime, recipient), ReplValue::Int(2));
}

#[test]
fn process_suite_exit_normal_trap_kill_and_duplicate_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("exit-sender"));
    let ignored = runtime.spawn_root(source("ignored-normal"));
    assert_eq!(
        runtime
            .send_exit_signal(sender, ignored, VmExitReason::Normal)
            .expect("remote normal signal"),
        VmActorExitSignalOutcome::IgnoredNormal
    );
    assert!(runtime.is_alive(ignored));

    let trapper = runtime.spawn_root(source("exit-trapper"));
    runtime
        .set_actor_trap_exits(trapper, true)
        .expect("enable trap exits");
    assert!(matches!(
        runtime
            .send_exit_signal(
                trapper,
                trapper,
                VmExitReason::Error("self-signal".to_string()),
            )
            .expect("trapped self signal"),
        VmActorExitSignalOutcome::DeliveredMessage { .. }
    ));
    assert_eq!(
        receive_payload(&mut runtime, trapper),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(trapper.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("self-signal".to_string()),
            ]),
        ])
    );
    assert!(runtime.is_alive(trapper));

    assert_eq!(
        runtime
            .send_exit_signal(sender, trapper, VmExitReason::Killed)
            .expect("kill bypasses trapping"),
        VmActorExitSignalOutcome::Exited
    );
    runtime
        .exit_actor(trapper, VmExitReason::Error("late".to_string()))
        .expect("duplicate exit is ignored");
    assert_eq!(
        runtime
            .processes()
            .snapshot(trapper)
            .expect("postmortem trapper")
            .state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
}

#[test]
fn process_suite_alias_explicit_and_reply_lifecycle_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("alias-sender"));
    let recipient = runtime.spawn_root(source("alias-recipient"));
    let explicit = runtime.create_alias(recipient).expect("explicit alias");
    let reply = runtime
        .create_alias_with_options(recipient, VmProcessAliasOptions::default().reply())
        .expect("reply alias");

    runtime
        .send_alias(sender, explicit, ReplValue::Int(1))
        .expect("first explicit delivery");
    runtime
        .send_alias(sender, explicit, ReplValue::Int(2))
        .expect("second explicit delivery");
    runtime
        .send_alias(sender, reply, ReplValue::Int(3))
        .expect("reply delivery consumes reply alias");
    assert_eq!(receive_payload(&mut runtime, recipient), ReplValue::Int(1));
    assert_eq!(receive_payload(&mut runtime, recipient), ReplValue::Int(2));
    assert_eq!(receive_payload(&mut runtime, recipient), ReplValue::Int(3));
    assert_eq!(
        runtime
            .send_alias(sender, reply, ReplValue::Int(4))
            .expect_err("reply alias is one shot"),
        format!("process alias {} is not registered", reply.as_u64())
    );

    assert_eq!(
        runtime
            .remove_alias(explicit)
            .expect("remove explicit alias"),
        recipient
    );
    assert_eq!(
        runtime
            .send_alias(sender, explicit, ReplValue::Int(5))
            .expect_err("removed alias rejects delivery"),
        format!("process alias {} is not registered", explicit.as_u64())
    );
}
