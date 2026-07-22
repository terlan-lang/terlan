use super::{
    actor_timer::VmActorTimerDelivery, VmActorReceive, VmActorRuntime, VmExitReason,
    VmProcessSource, VmRuntimeEnvironmentProfile,
};
use crate::runtime::vm::{timer::VmTimerEvent, ReplValue};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimerParity", name, 0)
}

#[test]
fn timer_parity_cancellation_preserves_equal_deadline_order_and_exactly_once_delivery() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("block recipient"),
        VmActorReceive::Blocked
    );

    let first = runtime
        .send_after(sender, recipient, ReplValue::Int(1), 10, 5)
        .expect("first timer");
    let cancelled = runtime
        .send_after(sender, recipient, ReplValue::Int(2), 10, 5)
        .expect("cancelled timer");
    let third = runtime
        .send_after(sender, recipient, ReplValue::Int(3), 10, 5)
        .expect("third timer");
    let cancellation = runtime
        .cancel_delayed_send(cancelled, 12)
        .expect("cancel middle timer");
    assert_eq!(cancellation.remaining_ticks, 3);
    assert!(matches!(
        cancellation.timer_event,
        VmTimerEvent::Cancelled { timer_id, .. } if timer_id == cancelled
    ));

    let advanced = runtime.advance_actor_timers(15);
    assert!(matches!(
        advanced.timer_events.as_slice(),
        [
            VmTimerEvent::Fired { timer_id: first_id, .. },
            VmTimerEvent::Fired { timer_id: third_id, .. },
        ] if *first_id == first && *third_id == third
    ));
    assert_eq!(
        advanced.deliveries,
        vec![
            VmActorTimerDelivery::Delivered {
                timer_id: first,
                message_id: 1,
            },
            VmActorTimerDelivery::Delivered {
                timer_id: third,
                message_id: 2,
            },
        ]
    );

    for expected in [1, 3] {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(recipient)
            .expect("receive ordered timer payload")
        else {
            panic!("timer payload must be available");
        };
        assert_eq!(message.payload, ReplValue::Int(expected));
    }
    assert_eq!(runtime.delayed_send_count(), 0);
    assert!(runtime.advance_actor_timers(15).deliveries.is_empty());
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("delivery must not repeat"),
        VmActorReceive::Blocked
    );
}

#[test]
fn timer_parity_isolates_owner_exit_recipient_exit_and_surviving_delivery() {
    let mut runtime = VmActorRuntime::default();
    let exiting_owner = runtime.spawn_root(source("exiting-owner"));
    let sender = runtime.spawn_root(source("sender"));
    let exiting_recipient = runtime.spawn_root(source("exiting-recipient"));
    let survivor = runtime.spawn_root(source("survivor"));

    let owner_exit = runtime
        .send_after(exiting_owner, survivor, ReplValue::Int(1), 0, 10)
        .expect("owner-exit timer");
    let recipient_exit = runtime
        .send_after(sender, exiting_recipient, ReplValue::Int(2), 0, 10)
        .expect("recipient-exit timer");
    let delivered = runtime
        .send_after(sender, survivor, ReplValue::Int(3), 0, 10)
        .expect("surviving timer");

    runtime
        .exit_actor(exiting_owner, VmExitReason::Normal)
        .expect("exit timer owner");
    runtime
        .exit_actor(exiting_recipient, VmExitReason::Normal)
        .expect("exit timer recipient");
    assert_eq!(
        runtime.read_delayed_send(owner_exit, 0),
        Err(format!("missing timer {}", owner_exit.as_u64()))
    );

    let advanced = runtime.advance_actor_timers(10);
    assert!(matches!(
        advanced.timer_events.as_slice(),
        [
            VmTimerEvent::Fired { timer_id: rejected_id, .. },
            VmTimerEvent::Fired { timer_id: delivered_id, .. },
        ] if *rejected_id == recipient_exit && *delivered_id == delivered
    ));
    assert_eq!(
        advanced.deliveries,
        vec![
            VmActorTimerDelivery::Rejected {
                timer_id: recipient_exit,
                diagnostic: format!(
                    "recipient process {} has exited",
                    exiting_recipient.as_u64()
                ),
            },
            VmActorTimerDelivery::Delivered {
                timer_id: delivered,
                message_id: 1,
            },
        ]
    );
    assert!(matches!(
        runtime
            .receive_next_or_block(survivor)
            .expect("surviving timer payload"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(3)
    ));

    let observation = runtime
        .observation_snapshot(
            VmRuntimeEnvironmentProfile::new(16, 1).expect("valid timer parity profile"),
        )
        .expect("timer parity observation");
    assert_eq!(observation.timer_metrics.started, 3);
    assert_eq!(observation.timer_metrics.fired, 2);
    assert_eq!(observation.timer_metrics.owner_exited, 1);
    assert_eq!(observation.timer_metrics.ordering_trace, vec![1, 2, 3]);
    assert!(observation.timers.is_empty());
}
