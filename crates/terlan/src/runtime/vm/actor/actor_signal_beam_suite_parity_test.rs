use super::super::super::process::{
    VmExitReason, VmMessagePriority, VmProcessSource, VmProcessState,
};
use super::super::super::process_alias::VmProcessAliasOptions;
use super::super::super::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::super::super::ReplValue;
use super::super::actor_exit::VmActorExitSignalOutcome;
use super::super::{VmActorDemonitorOptions, VmActorReceive, VmActorRuntime};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.SignalParity", name, 0)
}

fn receive_payload(
    runtime: &mut VmActorRuntime,
    recipient: super::super::super::process::VmProcessId,
) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(recipient)
        .expect("receive queued signal-suite message")
    else {
        panic!("signal-suite message must be queued");
    };
    message.payload
}

#[test]
fn signal_suite_priority_link_monitor_order_contract() {
    let mut runtime = VmActorRuntime::default();
    let receiver = runtime.spawn_root(source("receiver"));
    let sender = runtime.spawn_root(source("sender"));
    let ordinary_link = runtime.spawn_root(source("ordinary-link"));
    let priority_link = runtime.spawn_root(source("priority-link"));
    let ordinary_monitor = runtime.spawn_root(source("ordinary-monitor"));
    let priority_monitor = runtime.spawn_root(source("priority-monitor"));

    runtime
        .set_actor_trap_exits(receiver, true)
        .expect("receiver traps linked exits");
    runtime
        .link_actors(receiver, ordinary_link)
        .expect("ordinary link");
    runtime
        .link_actors_with_priority(receiver, priority_link, true)
        .expect("priority link");
    let ordinary_ref = runtime
        .monitor_actor(receiver, ordinary_monitor)
        .expect("ordinary monitor");
    let priority_ref = runtime
        .monitor_actor_with_priority(receiver, priority_monitor, true)
        .expect("priority monitor");
    assert!(runtime
        .actor_has_priority_messages(receiver)
        .expect("inspect active priority relationships"));

    runtime
        .send(sender, receiver, ReplValue::Atom("ordinary-1".to_string()))
        .expect("first ordinary message");
    runtime
        .exit_actor(
            ordinary_link,
            VmExitReason::Error("ordinary-link".to_string()),
        )
        .expect("ordinary linked exit");
    runtime
        .exit_actor(
            priority_link,
            VmExitReason::Error("priority-link".to_string()),
        )
        .expect("priority linked exit");
    runtime
        .exit_actor(ordinary_monitor, VmExitReason::Normal)
        .expect("ordinary monitor completion");
    runtime
        .exit_actor(priority_monitor, VmExitReason::Normal)
        .expect("priority monitor completion");
    runtime
        .send_priority(
            sender,
            receiver,
            ReplValue::Atom("priority-message".to_string()),
        )
        .expect("explicit priority message");
    runtime
        .send(sender, receiver, ReplValue::Atom("ordinary-2".to_string()))
        .expect("second ordinary message");

    let expected = vec![
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(priority_link.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("priority-link".to_string()),
            ]),
        ]),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(priority_ref.as_u64() as i64),
            ReplValue::Int(priority_monitor.as_u64() as i64),
            ReplValue::Atom("normal".to_string()),
        ]),
        ReplValue::Atom("priority-message".to_string()),
        ReplValue::Atom("ordinary-1".to_string()),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(ordinary_link.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("ordinary-link".to_string()),
            ]),
        ]),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(ordinary_ref.as_u64() as i64),
            ReplValue::Int(ordinary_monitor.as_u64() as i64),
            ReplValue::Atom("normal".to_string()),
        ]),
        ReplValue::Atom("ordinary-2".to_string()),
    ];
    for expected_payload in expected {
        assert_eq!(receive_payload(&mut runtime, receiver), expected_payload);
    }

    assert!(!runtime
        .actor_has_priority_messages(receiver)
        .expect("completed relationships disable priority messages"));
    assert_eq!(
        runtime
            .receive_next_or_block(receiver)
            .expect("mailbox should now block"),
        VmActorReceive::Blocked
    );
}

#[test]
fn signal_suite_priority_relationship_enable_disable_contract() {
    let mut runtime = VmActorRuntime::default();
    let receiver = runtime.spawn_root(source("receiver"));
    let peer = runtime.spawn_root(source("peer"));

    runtime
        .link_actors_with_priority(receiver, peer, true)
        .expect("enable priority link");
    assert!(runtime
        .actor_has_priority_messages(receiver)
        .expect("priority link is active"));
    assert!(!runtime
        .actor_has_priority_messages(peer)
        .expect("reverse endpoint did not request priority"));

    assert!(!runtime
        .link_actors_with_priority(peer, receiver, false)
        .expect("reverse ordinary relink remains idempotent"));
    assert!(runtime
        .actor_has_priority_messages(receiver)
        .expect("reverse relink cannot disable receiver priority"));
    assert!(!runtime
        .actor_has_priority_messages(peer)
        .expect("reverse ordinary relink cannot inherit priority"));

    assert!(!runtime
        .link_actors_with_priority(receiver, peer, false)
        .expect("duplicate link updates its priority lane"));
    assert!(!runtime
        .actor_has_priority_messages(receiver)
        .expect("ordinary relink disables priority lane"));
    runtime
        .unlink_actors(receiver, peer)
        .expect("unlink updated relationship");

    let sender = runtime.spawn_root(source("sender"));
    runtime
        .send_priority(sender, receiver, ReplValue::Int(1))
        .expect("priority send remains independently available");
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(receiver)
        .expect("receive explicit priority message")
    else {
        panic!("priority message must be queued");
    };
    assert_eq!(message.priority, VmMessagePriority::Priority);
}

#[test]
fn signal_suite_priority_alias_enable_disable_contract() {
    let mut runtime = VmActorRuntime::default();
    let receiver = runtime.spawn_root(source("alias-receiver"));
    let sender = runtime.spawn_root(source("alias-sender"));
    let alias = runtime
        .create_alias_with_options(receiver, VmProcessAliasOptions::default().priority())
        .expect("priority alias");

    assert!(runtime
        .actor_has_priority_messages(receiver)
        .expect("priority alias enables priority messages"));
    runtime
        .send(sender, receiver, ReplValue::Atom("ordinary".to_string()))
        .expect("ordinary traffic");
    runtime
        .send_alias_exit_signal(
            sender,
            alias,
            ReplValue::Atom("priority-exit".to_string()),
            true,
        )
        .expect("priority alias exit signal");
    assert_eq!(runtime.resolve_alias(alias), Some(receiver));
    runtime
        .send_alias_priority(sender, alias, ReplValue::Atom("priority-reply".to_string()))
        .expect("priority alias message");
    assert_eq!(runtime.resolve_alias(alias), Some(receiver));

    assert_eq!(
        receive_payload(&mut runtime, receiver),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(sender.as_u64() as i64),
            ReplValue::Atom("priority-exit".to_string()),
        ])
    );
    assert_eq!(
        receive_payload(&mut runtime, receiver),
        ReplValue::Atom("priority-reply".to_string())
    );
    assert_eq!(
        receive_payload(&mut runtime, receiver),
        ReplValue::Atom("ordinary".to_string())
    );

    assert_eq!(
        runtime.remove_alias(alias).expect("explicit unalias"),
        receiver
    );
    assert!(!runtime
        .actor_has_priority_messages(receiver)
        .expect("unalias disables priority messages"));
}

#[test]
fn signal_suite_priority_reply_alias_is_consumed_only_by_successful_reply() {
    let mut runtime = VmActorRuntime::default();
    let receiver = runtime.spawn_root(source("reply-receiver"));
    let sender = runtime.spawn_root(source("reply-sender"));
    let reply_alias = runtime
        .create_alias_with_options(
            receiver,
            VmProcessAliasOptions::default().priority().reply(),
        )
        .expect("priority reply alias");

    runtime
        .send_alias_exit_signal(
            sender,
            reply_alias,
            ReplValue::Atom("functional-exit".to_string()),
            true,
        )
        .expect("exit signal must not consume reply alias");
    assert_eq!(runtime.resolve_alias(reply_alias), Some(receiver));
    runtime
        .send_alias_priority(sender, reply_alias, ReplValue::Int(42))
        .expect("successful reply consumes alias");
    assert_eq!(runtime.resolve_alias(reply_alias), None);
    assert!(!runtime
        .actor_has_priority_messages(receiver)
        .expect("consumed reply alias disables priority messages"));

    assert_eq!(
        runtime
            .send_alias_priority(sender, reply_alias, ReplValue::Int(99))
            .expect_err("consumed alias rejects a second reply"),
        format!("process alias {} is not registered", reply_alias.as_u64())
    );
    let ordinary_alias = runtime.create_alias(receiver).expect("ordinary alias");
    assert_eq!(
        runtime
            .send_alias_priority(sender, ordinary_alias, ReplValue::Unit)
            .expect_err("ordinary alias rejects priority delivery"),
        format!(
            "process alias {} is not priority-enabled",
            ordinary_alias.as_u64()
        )
    );

    assert_eq!(
        receive_payload(&mut runtime, receiver),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(sender.as_u64() as i64),
            ReplValue::Atom("functional-exit".to_string()),
        ])
    );
    assert_eq!(receive_payload(&mut runtime, receiver), ReplValue::Int(42));
    assert_eq!(
        runtime
            .receive_next_or_block(receiver)
            .expect("failed sends leave mailbox empty"),
        VmActorReceive::Blocked
    );
}

#[test]
fn signal_suite_direct_exit_trap_normal_kill_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("exit-sender"));
    let target = runtime.spawn_root(source("exit-target"));
    let observer = runtime.spawn_root(source("exit-observer"));
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("monitor target before direct signals");
    runtime
        .set_actor_trap_exits(target, true)
        .expect("target traps exit signals");

    assert_eq!(
        runtime
            .send_exit_signal(sender, target, VmExitReason::Normal)
            .expect("trapped normal signal"),
        VmActorExitSignalOutcome::DeliveredMessage { message_id: 1 }
    );
    assert_eq!(
        receive_payload(&mut runtime, target),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(sender.as_u64() as i64),
            ReplValue::Atom("normal".to_string()),
        ])
    );
    assert_eq!(
        runtime
            .send_exit_signal(sender, target, VmExitReason::Error("trapped".to_string()),)
            .expect("trapped abnormal signal"),
        VmActorExitSignalOutcome::DeliveredMessage { message_id: 2 }
    );
    assert_eq!(
        receive_payload(&mut runtime, target),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(sender.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("trapped".to_string()),
            ]),
        ])
    );
    assert_eq!(
        runtime
            .send_exit_signal(target, target, VmExitReason::Normal)
            .expect("trapped self normal signal"),
        VmActorExitSignalOutcome::DeliveredMessage { message_id: 3 }
    );
    assert_eq!(
        receive_payload(&mut runtime, target),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Atom("normal".to_string()),
        ])
    );

    assert_eq!(
        runtime
            .send_exit_signal(sender, target, VmExitReason::Killed)
            .expect("kill is untrappable"),
        VmActorExitSignalOutcome::Exited
    );
    assert_eq!(
        runtime
            .processes()
            .snapshot(target)
            .expect("exited target")
            .state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(
        receive_payload(&mut runtime, observer),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Atom("killed".to_string()),
        ])
    );

    let ignored = runtime.spawn_root(source("normal-ignored"));
    assert_eq!(
        runtime
            .send_exit_signal(sender, ignored, VmExitReason::Normal)
            .expect("untrapped remote normal signal"),
        VmActorExitSignalOutcome::IgnoredNormal
    );
    assert!(runtime.is_alive(ignored));
    assert_eq!(
        runtime
            .send_exit_signal(ignored, ignored, VmExitReason::Normal)
            .expect("self normal signal exits"),
        VmActorExitSignalOutcome::Exited
    );
}

#[test]
fn signal_suite_message_before_down_order_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("order-sender"));
    let target = runtime.spawn_root(source("order-target"));
    let observer = runtime.spawn_root(source("order-observer"));
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("monitor ordered target");

    runtime
        .send(target, observer, ReplValue::Int(1))
        .expect("first message before exit");
    runtime
        .send(target, observer, ReplValue::Int(2))
        .expect("second message before exit");
    assert_eq!(
        runtime
            .send_exit_signal(sender, target, VmExitReason::Error("ordered".to_string()),)
            .expect("ordered target exit"),
        VmActorExitSignalOutcome::Exited
    );
    assert_eq!(
        runtime
            .send(sender, target, ReplValue::Int(3))
            .expect_err("messages after exit are rejected"),
        format!("recipient process {} has exited", target.as_u64())
    );

    assert_eq!(receive_payload(&mut runtime, observer), ReplValue::Int(1));
    assert_eq!(receive_payload(&mut runtime, observer), ReplValue::Int(2));
    assert_eq!(
        receive_payload(&mut runtime, observer),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("ordered".to_string()),
            ]),
        ])
    );
}

#[test]
fn signal_suite_kill_chain_translates_to_killed_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("kill-sender"));
    let first = runtime.spawn_root(source("kill-first"));
    let middle = runtime.spawn_root(source("kill-middle"));
    let trapper = runtime.spawn_root(source("kill-trapper"));
    runtime
        .set_actor_trap_exits(trapper, true)
        .expect("end of chain traps exits");
    runtime
        .link_actors(first, middle)
        .expect("first chain link");
    runtime
        .link_actors(middle, trapper)
        .expect("second chain link");

    assert_eq!(
        runtime
            .send_exit_signal(sender, first, VmExitReason::Killed)
            .expect("kill chain head"),
        VmActorExitSignalOutcome::Exited
    );
    for exited in [first, middle] {
        assert_eq!(
            runtime
                .processes()
                .snapshot(exited)
                .expect("chain exit")
                .state,
            VmProcessState::Exited(VmExitReason::Killed)
        );
    }
    assert!(runtime.is_alive(trapper));
    assert_eq!(
        receive_payload(&mut runtime, trapper),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(middle.as_u64() as i64),
            ReplValue::Atom("killed".to_string()),
        ])
    );
}

#[test]
fn signal_suite_unlink_exit_serialization_contract() {
    let mut runtime = VmActorRuntime::default();

    for round in 0..128 {
        let parent = runtime.spawn_root(source(&format!("unlink-parent-{round}")));
        let child = runtime.spawn_root(source(&format!("unlink-child-{round}")));
        runtime
            .set_actor_trap_exits(child, true)
            .expect("child traps link exits");
        runtime.link_actors(parent, child).expect("initial link");
        assert!(runtime
            .unlink_actors(parent, child)
            .expect("unlink completes synchronously"));
        runtime
            .exit_actor(parent, VmExitReason::Error("bye".to_string()))
            .expect("parent exits after unlink");

        assert!(runtime.is_alive(child));
        assert_eq!(
            runtime
                .link_actors(child, parent)
                .expect_err("relink after observed exit fails without stale signal"),
            format!("cannot link exited process {}", parent.as_u64())
        );
        assert_eq!(
            runtime
                .processes()
                .get(child)
                .expect("child remains live")
                .mailbox_len(),
            0
        );
    }

    let parent = runtime.spawn_root(source("relink-parent"));
    let child = runtime.spawn_root(source("relink-child"));
    runtime
        .set_actor_trap_exits(child, true)
        .expect("relinked child traps exits");
    runtime.link_actors(parent, child).expect("initial link");
    runtime
        .unlink_actors(parent, child)
        .expect("remove initial generation");
    runtime
        .link_actors(child, parent)
        .expect("create new link generation");
    runtime
        .exit_actor(parent, VmExitReason::Error("bye".to_string()))
        .expect("exit after relink");
    assert_eq!(
        receive_payload(&mut runtime, child),
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(parent.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("bye".to_string()),
            ]),
        ])
    );
}

#[test]
fn signal_suite_contended_enqueue_inspection_and_single_wakeup_contract() {
    let mut runtime = VmActorRuntime::default();
    let receiver = runtime.spawn_root(source("enqueue-receiver"));
    assert_eq!(
        runtime
            .receive_next_or_block(receiver)
            .expect("empty receiver blocks"),
        VmActorReceive::Blocked
    );
    let sender = runtime.spawn_root(source("enqueue-sender"));

    for value in 0..256 {
        runtime
            .link_actors(sender, receiver)
            .expect("transient link");
        runtime
            .unlink_actors(sender, receiver)
            .expect("transient unlink");
        let monitor_ref = runtime
            .monitor_actor(sender, receiver)
            .expect("transient monitor");
        assert!(
            runtime
                .demonitor_actor(sender, monitor_ref, VmActorDemonitorOptions::default(),)
                .expect("transient demonitor")
                .removed
        );
        runtime
            .send(sender, receiver, ReplValue::Int(value))
            .expect("ordinary contended send");
        runtime
            .send_priority(sender, receiver, ReplValue::Int(-value - 1))
            .expect("priority contended send");
        assert_eq!(
            runtime
                .processes()
                .snapshot(receiver)
                .expect("interleaved mailbox inspection")
                .mailbox_messages,
            (value as usize + 1) * 2
        );
    }

    assert!(runtime
        .failure_snapshot(receiver)
        .expect("all transient relationships cleaned")
        .links
        .is_empty());
    let receiver_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, receiver);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("receiver wakes exactly once");
    assert_eq!(receiver_run.pid, Some(receiver));
    assert_eq!(receiver_run.outcome, VmSchedulerOutcome::Blocked);
    let sender_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, sender);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("sender retains its original queue slot");
    assert_eq!(sender_run.pid, Some(sender));
    let idle = runtime
        .run_next(|_, _| panic!("duplicate receiver wake must not remain queued"))
        .expect("queue drains after one wake per process");
    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);

    for expected in 0..256 {
        assert_eq!(
            receive_payload(&mut runtime, receiver),
            ReplValue::Int(-expected - 1)
        );
    }
    for expected in 0..256 {
        assert_eq!(
            receive_payload(&mut runtime, receiver),
            ReplValue::Int(expected)
        );
    }
}

#[test]
fn signal_suite_pid_name_alias_messages_precede_down_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("route-sender"));
    let receiver = runtime.spawn_root(source("route-receiver"));
    runtime
        .register_name("signal.route.receiver", receiver)
        .expect("register receiver route");
    let alias = runtime
        .create_alias(receiver)
        .expect("receiver alias route");
    let monitor_ref = runtime
        .monitor_actor(receiver, sender)
        .expect("receiver monitors sender");

    for sequence in 0..300 {
        let payload = ReplValue::Int(sequence);
        match sequence % 3 {
            0 => runtime.send(sender, receiver, payload).expect("pid route"),
            1 => runtime
                .send_named(sender, "signal.route.receiver", payload)
                .expect("name route"),
            _ => runtime
                .send_alias(sender, alias, payload)
                .expect("alias route"),
        };
    }
    runtime
        .exit_actor(sender, VmExitReason::Normal)
        .expect("sender exits after all route messages");

    for expected in 0..300 {
        assert_eq!(
            receive_payload(&mut runtime, receiver),
            ReplValue::Int(expected)
        );
    }
    assert_eq!(
        receive_payload(&mut runtime, receiver),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(sender.as_u64() as i64),
            ReplValue::Atom("normal".to_string()),
        ])
    );
}
