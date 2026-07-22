use super::actor_timer::VmActorTimerDelivery;
use super::actor_timer_options::{
    VmActorTimerCancelMode, VmActorTimerDeadline, VmActorTimerInformation,
    VmActorTimerOptionResult, VmActorTimerReadMode,
};
use super::{VmActorReceive, VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::{timer::VmTimerEvent, ReplValue};

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimerBifSuiteParity", function, 0)
}

#[test]
fn timer_bif_suite_huge_near_read_cancel_and_order_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let huge_deadline = u64::MAX - 2;

    let huge = runtime
        .send_with_deadline(
            sender,
            recipient,
            ReplValue::Atom("huge".to_string()),
            7,
            VmActorTimerDeadline::Absolute(huge_deadline),
        )
        .expect("schedule huge absolute timer");
    let near = runtime
        .start_message_timer(
            sender,
            recipient,
            ReplValue::Tuple(vec![
                ReplValue::Atom("near".to_string()),
                ReplValue::Int(42),
            ]),
            7,
            2,
        )
        .expect("schedule near correlated timer");

    let read = runtime
        .read_delayed_send_with_mode(sender, huge, 7, VmActorTimerReadMode::Synchronous)
        .expect("read huge timer");
    assert_eq!(
        read.result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Remaining(
            huge_deadline - 7,
        ))
    );
    assert!(runtime.advance_actor_timers(8).deliveries.is_empty());

    let near_advance = runtime.advance_actor_timers(9);
    assert!(matches!(
        near_advance.timer_events.as_slice(),
        [VmTimerEvent::Fired { timer_id, .. }] if *timer_id == near
    ));
    assert_eq!(
        near_advance.deliveries,
        vec![VmActorTimerDelivery::Delivered {
            timer_id: near,
            message_id: 1,
        }]
    );
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(recipient)
        .expect("receive correlated near timer")
    else {
        panic!("near timer must deliver exactly once");
    };
    assert_eq!(
        message.payload,
        ReplValue::Record {
            name: "TimerMessage".to_string(),
            fields: vec![
                (
                    "timer_id".to_string(),
                    ReplValue::String(near.as_u64().to_string()),
                ),
                (
                    "payload".to_string(),
                    ReplValue::Tuple(vec![
                        ReplValue::Atom("near".to_string()),
                        ReplValue::Int(42),
                    ]),
                ),
            ],
        }
    );

    let cancelled = runtime
        .cancel_delayed_send_with_mode(
            sender,
            huge,
            10,
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            },
        )
        .expect("cancel huge timer");
    assert_eq!(
        cancelled.result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Remaining(
            huge_deadline - 10,
        ))
    );
    assert!(matches!(
        cancelled.timer_event,
        Some(VmTimerEvent::Cancelled { timer_id, .. }) if timer_id == huge
    ));

    let stale = runtime
        .read_delayed_send_with_mode(sender, huge, 10, VmActorTimerReadMode::Synchronous)
        .expect("read stale timer");
    assert_eq!(
        stale.result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Missing)
    );
    assert_eq!(runtime.delayed_send_count(), 0);
    assert!(runtime
        .advance_actor_timers(huge_deadline)
        .deliveries
        .is_empty());

    assert_eq!(
        runtime.send_after(sender, recipient, ReplValue::Unit, u64::MAX, 1),
        Err(format!(
            "delayed actor send deadline overflow at tick {} with delay 1",
            u64::MAX
        ))
    );
    let next = runtime
        .send_after(sender, recipient, ReplValue::Unit, huge_deadline, 0)
        .expect("overflow rejection must not allocate timer identity");
    assert_eq!(next.as_u64(), 3);
}

#[test]
fn timer_bif_suite_equal_deadline_batch_cancellation_and_cleanup_contract() {
    const TIMER_COUNT: i64 = 128;

    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("batch-sender"));
    let recipient = runtime.spawn_root(source("batch-recipient"));
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("block batch recipient"),
        VmActorReceive::Blocked
    );

    let mut active = Vec::new();
    for sequence in 0..TIMER_COUNT {
        let payload = ReplValue::Tuple(vec![
            ReplValue::Atom("batch".to_string()),
            ReplValue::Int(sequence),
            ReplValue::List(vec![
                ReplValue::String(format!("payload-{sequence}")),
                ReplValue::Record {
                    name: "NestedTimerPayload".to_string(),
                    fields: vec![("sequence".to_string(), ReplValue::Int(sequence))],
                },
            ]),
        ]);
        let timer = runtime
            .send_after(sender, recipient, payload, 0, 50)
            .expect("schedule equal-deadline timer");
        if sequence % 4 == 1 {
            runtime
                .cancel_delayed_send(timer, 25)
                .expect("cancel selected timer before deadline");
        } else {
            active.push((timer, sequence));
        }
    }

    assert_eq!(runtime.delayed_send_count(), active.len());
    assert!(runtime.advance_actor_timers(49).deliveries.is_empty());
    let advanced = runtime.advance_actor_timers(50);
    assert_eq!(advanced.timer_events.len(), active.len());
    assert_eq!(advanced.deliveries.len(), active.len());
    for ((expected_timer, _), delivery) in active.iter().zip(&advanced.deliveries) {
        assert!(matches!(
            delivery,
            VmActorTimerDelivery::Delivered { timer_id, .. } if timer_id == expected_timer
        ));
    }

    for (_, expected_sequence) in &active {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(recipient)
            .expect("drain equal-deadline timer batch")
        else {
            panic!("each active timer must deliver once");
        };
        assert_eq!(
            message.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("batch".to_string()),
                ReplValue::Int(*expected_sequence),
                ReplValue::List(vec![
                    ReplValue::String(format!("payload-{expected_sequence}")),
                    ReplValue::Record {
                        name: "NestedTimerPayload".to_string(),
                        fields: vec![("sequence".to_string(), ReplValue::Int(*expected_sequence),)],
                    },
                ]),
            ])
        );
    }
    assert_eq!(runtime.delayed_send_count(), 0);
    assert!(runtime.advance_actor_timers(50).deliveries.is_empty());

    let exiting_owner = runtime.spawn_root(source("exiting-owner"));
    for sequence in 0..32 {
        runtime
            .send_after(
                exiting_owner,
                recipient,
                ReplValue::Int(sequence),
                50,
                1_000_000,
            )
            .expect("schedule owner-bound cleanup timer");
    }
    assert_eq!(runtime.delayed_send_count(), 32);
    runtime
        .exit_actor(exiting_owner, VmExitReason::Normal)
        .expect("owner exit cancels every timer");
    assert_eq!(runtime.delayed_send_count(), 0);
    assert!(runtime
        .advance_actor_timers(1_000_050)
        .deliveries
        .is_empty());
}
