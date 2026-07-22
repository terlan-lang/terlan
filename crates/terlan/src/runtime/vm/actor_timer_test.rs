use super::{
    actor_timer::VmActorTimerDelivery, VmActorReceive, VmActorRuntime, VmExitReason,
    VmProcessSource,
};
use crate::runtime::vm::{timer::VmTimerEvent, ReplValue};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Timer", name, 0)
}

#[test]
fn delayed_actor_sends_deliver_typed_payloads_through_every_route_in_timer_order() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .register_name("recipient", recipient)
        .expect("register recipient");
    let alias = runtime.create_alias(recipient).expect("create alias");
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("block recipient"),
        VmActorReceive::Blocked
    );

    let direct = runtime
        .send_after(
            sender,
            recipient,
            ReplValue::Tuple(vec![
                ReplValue::Atom("direct".to_string()),
                ReplValue::Int(1),
            ]),
            10,
            5,
        )
        .expect("direct delayed send");
    let named = runtime
        .send_named_after(
            sender,
            "recipient",
            ReplValue::List(vec![ReplValue::String("named".to_string())]),
            10,
            5,
        )
        .expect("named delayed send");
    let aliased = runtime
        .send_alias_after(
            sender,
            alias,
            ReplValue::Record {
                name: "TimerPayload".to_string(),
                fields: vec![("route".to_string(), ReplValue::Atom("alias".to_string()))],
            },
            10,
            5,
        )
        .expect("alias delayed send");
    let correlated = runtime
        .start_named_message_timer(
            sender,
            "recipient",
            ReplValue::Atom("correlated".to_string()),
            10,
            5,
        )
        .expect("named message timer");
    let inspection = format!("{runtime:?}");
    assert!(inspection.contains("VmDelayedActorMessage"));
    assert!(inspection.contains("TimerMessage"));

    assert_eq!(runtime.read_delayed_send(direct, 14), Ok(1));
    assert_eq!(runtime.read_delayed_send(named, 15), Ok(0));
    assert!(runtime.advance_actor_timers(14).deliveries.is_empty());
    let advanced = runtime.advance_actor_timers(15);
    assert_eq!(advanced.timer_events.len(), 4);
    assert_eq!(
        advanced.deliveries,
        vec![
            VmActorTimerDelivery::Delivered {
                timer_id: direct,
                message_id: 1,
            },
            VmActorTimerDelivery::Delivered {
                timer_id: named,
                message_id: 2,
            },
            VmActorTimerDelivery::Delivered {
                timer_id: aliased,
                message_id: 3,
            },
            VmActorTimerDelivery::Delivered {
                timer_id: correlated,
                message_id: 4,
            },
        ]
    );
    assert_eq!(runtime.delayed_send_count(), 0);

    let first = runtime
        .receive_next_or_block(recipient)
        .expect("receive direct");
    let second = runtime
        .receive_next_or_block(recipient)
        .expect("receive named");
    let third = runtime
        .receive_next_or_block(recipient)
        .expect("receive alias");
    let fourth = runtime
        .receive_next_or_block(recipient)
        .expect("receive correlated timer");
    assert!(matches!(
        first,
        VmActorReceive::Message(message)
            if message.sender == sender
                && message.payload
                    == ReplValue::Tuple(vec![
                        ReplValue::Atom("direct".to_string()),
                        ReplValue::Int(1),
                    ])
    ));
    assert!(matches!(
        second,
        VmActorReceive::Message(message)
            if message.payload
                == ReplValue::List(vec![ReplValue::String("named".to_string())])
    ));
    assert!(matches!(
        third,
        VmActorReceive::Message(message)
            if message.payload
                == ReplValue::Record {
                    name: "TimerPayload".to_string(),
                    fields: vec![(
                        "route".to_string(),
                        ReplValue::Atom("alias".to_string()),
                    )],
                }
    ));
    assert!(matches!(
        fourth,
        VmActorReceive::Message(message)
            if message.payload
                == ReplValue::Record {
                    name: "TimerMessage".to_string(),
                    fields: vec![
                        (
                            "timer_id".to_string(),
                            ReplValue::String(correlated.as_u64().to_string()),
                        ),
                        (
                            "payload".to_string(),
                            ReplValue::Atom("correlated".to_string()),
                        ),
                    ],
                }
    ));
}

#[test]
fn delayed_actor_send_cancellation_is_atomic_and_stale_identity_is_rejected() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let timer = runtime
        .send_after(sender, recipient, ReplValue::Int(7), 20, 11)
        .expect("schedule delayed send");

    let cancellation = runtime
        .cancel_delayed_send(timer, 25)
        .expect("cancel delayed send");
    assert_eq!(cancellation.remaining_ticks, 6);
    assert!(matches!(
        cancellation.timer_event,
        VmTimerEvent::Cancelled { timer_id, owner, .. }
            if timer_id == timer && owner == sender
    ));
    assert_eq!(runtime.delayed_send_count(), 0);
    assert_eq!(
        runtime.read_delayed_send(timer, 25),
        Err("missing timer 1".to_string())
    );
    assert_eq!(
        runtime
            .cancel_delayed_send(timer, 25)
            .expect_err("cancelled timer must be stale"),
        "missing timer 1"
    );
    assert!(runtime.advance_actor_timers(31).deliveries.is_empty());
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("recipient remains empty"),
        VmActorReceive::Blocked
    );
}

#[test]
fn delayed_actor_send_freezes_name_resolution_and_handles_lifecycle_races() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let original = runtime.spawn_root(source("original"));
    let replacement = runtime.spawn_root(source("replacement"));
    runtime
        .register_name("worker", original)
        .expect("register original");
    let timer = runtime
        .send_named_after(sender, "worker", ReplValue::Int(1), 0, 5)
        .expect("schedule named send");
    runtime
        .unregister_name("worker")
        .expect("unregister original");
    runtime
        .register_name("worker", replacement)
        .expect("register replacement");

    let advanced = runtime.advance_actor_timers(5);
    assert_eq!(
        advanced.deliveries,
        vec![VmActorTimerDelivery::Delivered {
            timer_id: timer,
            message_id: 1,
        }]
    );
    assert!(matches!(
        runtime
            .receive_next_or_block(original)
            .expect("original receives frozen route"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(1)
    ));
    assert_eq!(
        runtime
            .receive_next_or_block(replacement)
            .expect("replacement receives nothing"),
        VmActorReceive::Blocked
    );

    let recipient_exit = runtime
        .send_after(sender, original, ReplValue::Int(2), 5, 5)
        .expect("schedule recipient exit race");
    runtime
        .exit_actor(original, VmExitReason::Normal)
        .expect("exit recipient");
    let rejected = runtime.advance_actor_timers(10);
    assert_eq!(
        rejected.deliveries,
        vec![VmActorTimerDelivery::Rejected {
            timer_id: recipient_exit,
            diagnostic: format!("recipient process {} has exited", original.as_u64()),
        }]
    );

    let owner_exit = runtime
        .send_after(sender, replacement, ReplValue::Int(3), 10, 5)
        .expect("schedule owner exit cleanup");
    runtime
        .exit_actor(sender, VmExitReason::Normal)
        .expect("exit sender");
    assert_eq!(runtime.delayed_send_count(), 0);
    assert_eq!(
        runtime.read_delayed_send(owner_exit, 10),
        Err(format!("missing timer {}", owner_exit.as_u64()))
    );
    assert!(runtime.advance_actor_timers(15).deliveries.is_empty());
}

#[test]
fn delayed_actor_send_rejections_do_not_allocate_timer_identity_or_payload_state() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let missing = super::VmProcessId::from_raw_for_test(999);

    assert_eq!(
        runtime.send_named_after(missing, "missing", ReplValue::Unit, 0, 1),
        Err("missing sender process 999".to_string())
    );
    assert_eq!(
        runtime.send_named_after(sender, "missing", ReplValue::Unit, 0, 1),
        Err("actor name `missing` is not registered".to_string())
    );
    assert_eq!(
        runtime.start_named_message_timer(sender, "missing", ReplValue::Unit, 0, 1),
        Err("actor name `missing` is not registered".to_string())
    );
    let removed_alias = runtime.create_alias(recipient).expect("create alias");
    runtime.remove_alias(removed_alias).expect("remove alias");
    assert_eq!(
        runtime.send_alias_after(sender, removed_alias, ReplValue::Unit, 0, 1),
        Err(format!(
            "process alias {} is not registered",
            removed_alias.as_u64()
        ))
    );
    assert_eq!(
        runtime.send_after(sender, missing, ReplValue::Unit, 0, 1),
        Err("missing recipient process 999".to_string())
    );
    assert_eq!(
        runtime.send_after(sender, recipient, ReplValue::Unit, u64::MAX, 1),
        Err(format!(
            "delayed actor send deadline overflow at tick {} with delay 1",
            u64::MAX
        ))
    );
    assert_eq!(
        runtime.start_message_timer(sender, recipient, ReplValue::Unit, u64::MAX, 1),
        Err(format!(
            "actor message timer deadline overflow at tick {} with delay 1",
            u64::MAX
        ))
    );
    assert_eq!(runtime.delayed_send_count(), 0);

    let first = runtime
        .send_after(sender, recipient, ReplValue::Unit, 0, 1)
        .expect("first valid timer");
    assert_eq!(first.as_u64(), 1);
}

#[test]
fn delayed_actor_send_delivers_late_and_reports_owner_exit_from_runtime_state() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let late = runtime
        .send_after(sender, recipient, ReplValue::Int(1), 0, 5)
        .expect("schedule late delivery");
    let late_advance = runtime.advance_actor_timers(8);
    assert!(matches!(
        late_advance.timer_events.as_slice(),
        [VmTimerEvent::DeadlineMissed { timer_id, late_by_ticks: 3, .. }]
            if *timer_id == late
    ));
    assert!(matches!(
        late_advance.deliveries.as_slice(),
        [VmActorTimerDelivery::Delivered { timer_id, message_id: 1 }]
            if *timer_id == late
    ));

    let orphaned = runtime
        .send_after(sender, recipient, ReplValue::Int(2), 8, 2)
        .expect("schedule owner exit race");
    runtime
        .processes
        .exit_process(sender, VmExitReason::Normal)
        .expect("transition owner outside actor facade");
    let owner_exit = runtime.advance_actor_timers(10);
    assert!(matches!(
        owner_exit.timer_events.as_slice(),
        [VmTimerEvent::OwnerExited { timer_id, .. }] if *timer_id == orphaned
    ));
    assert_eq!(
        owner_exit.deliveries,
        vec![VmActorTimerDelivery::OwnerExited { timer_id: orphaned }]
    );
    assert_eq!(runtime.delayed_send_count(), 0);
}
