use super::super::{
    actor_timer::VmActorTimerDelivery,
    actor_timer_options::{
        VmActorTimerCancelMode, VmActorTimerDeadline, VmActorTimerInformation,
        VmActorTimerOptionOutcome, VmActorTimerOptionResult, VmActorTimerReadMode,
    },
    VmActorReceive, VmActorRuntime, VmExitReason, VmProcessId, VmProcessSource,
};
use crate::runtime::vm::{memory::VmMemoryLimits, timer::VmTimerEvent, ReplValue};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimerOptions", name, 0)
}

fn remaining(ticks: u64) -> VmActorTimerOptionResult {
    VmActorTimerOptionResult::Information(VmActorTimerInformation::Remaining(ticks))
}

fn assert_reply(
    runtime: &mut VmActorRuntime,
    recipient: VmProcessId,
    record_name: &str,
    timer_id: u64,
    result: ReplValue,
) {
    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("receive timer option reply"),
        VmActorReceive::Message(message)
            if message.sender == recipient
                && message.payload == ReplValue::Record {
                    name: record_name.to_string(),
                    fields: vec![
                        ("timer_id".to_string(), ReplValue::String(timer_id.to_string())),
                        ("result".to_string(), result),
                    ],
                }
    ));
}

fn remaining_value(ticks: u64) -> ReplValue {
    ReplValue::Record {
        name: "TimerRemaining".to_string(),
        fields: vec![("ticks".to_string(), ReplValue::String(ticks.to_string()))],
    }
}

#[test]
fn timer_deadline_modes_schedule_relative_absolute_and_already_due_messages() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));

    let relative = runtime
        .send_with_deadline(
            sender,
            recipient,
            ReplValue::Atom("relative".to_string()),
            10,
            VmActorTimerDeadline::Relative(5),
        )
        .expect("relative deadline");
    let absolute = runtime
        .send_with_deadline(
            sender,
            recipient,
            ReplValue::Atom("absolute".to_string()),
            10,
            VmActorTimerDeadline::Absolute(16),
        )
        .expect("absolute deadline");
    let already_due = runtime
        .start_message_timer_with_deadline(
            sender,
            recipient,
            ReplValue::Atom("due".to_string()),
            10,
            VmActorTimerDeadline::Absolute(8),
        )
        .expect("already-due absolute timer");

    assert_eq!(runtime.read_delayed_send(relative, 10), Ok(5));
    assert_eq!(runtime.read_delayed_send(absolute, 10), Ok(6));
    let due = runtime.advance_actor_timers(10);
    assert!(matches!(
        due.timer_events.as_slice(),
        [VmTimerEvent::DeadlineMissed { timer_id, late_by_ticks: 2, .. }]
            if *timer_id == already_due
    ));
    assert_eq!(
        due.deliveries,
        vec![VmActorTimerDelivery::Delivered {
            timer_id: already_due,
            message_id: 1,
        }]
    );
    assert_eq!(runtime.advance_actor_timers(15).deliveries.len(), 1);
    assert_eq!(runtime.advance_actor_timers(16).deliveries.len(), 1);
}

#[test]
fn timer_read_modes_return_or_deliver_active_and_stale_information() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("reader"));
    let timer = runtime
        .send_after(actor, actor, ReplValue::Unit, 0, 20)
        .expect("timer");

    assert_eq!(
        runtime
            .read_delayed_send_with_mode(actor, timer, 12, VmActorTimerReadMode::Synchronous)
            .expect("synchronous read"),
        VmActorTimerOptionOutcome {
            result: remaining(8),
            timer_event: None,
            reply_message_id: None,
        }
    );
    assert_eq!(
        runtime
            .read_delayed_send_with_mode(actor, timer, 13, VmActorTimerReadMode::Asynchronous)
            .expect("asynchronous read"),
        VmActorTimerOptionOutcome {
            result: VmActorTimerOptionResult::Acknowledged,
            timer_event: None,
            reply_message_id: Some(1),
        }
    );
    assert_reply(
        &mut runtime,
        actor,
        "TimerReadReply",
        timer.as_u64(),
        remaining_value(7),
    );
    runtime
        .cancel_delayed_send(timer, 13)
        .expect("make timer stale");
    assert_eq!(
        runtime
            .read_delayed_send_with_mode(actor, timer, 13, VmActorTimerReadMode::Synchronous)
            .expect("stale synchronous read")
            .result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Missing)
    );
    let stale = runtime
        .read_delayed_send_with_mode(actor, timer, 13, VmActorTimerReadMode::Asynchronous)
        .expect("stale asynchronous read");
    assert_eq!(stale.reply_message_id, Some(2));
    assert_reply(
        &mut runtime,
        actor,
        "TimerReadReply",
        timer.as_u64(),
        ReplValue::Atom("missing".to_string()),
    );
}

#[test]
fn timer_cancel_modes_cover_information_suppression_async_replies_and_stale_ids() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("canceller"));
    let sync_info = runtime
        .send_after(actor, actor, ReplValue::Int(1), 0, 20)
        .expect("sync info timer");
    let sync_quiet = runtime
        .send_after(actor, actor, ReplValue::Int(2), 0, 20)
        .expect("sync quiet timer");
    let async_info = runtime
        .send_after(actor, actor, ReplValue::Int(3), 0, 20)
        .expect("async info timer");
    let async_quiet = runtime
        .send_after(actor, actor, ReplValue::Int(4), 0, 20)
        .expect("async quiet timer");

    let first = runtime
        .cancel_delayed_send_with_mode(
            actor,
            sync_info,
            5,
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            },
        )
        .expect("sync info cancellation");
    assert_eq!(first.result, remaining(15));
    assert!(matches!(
        first.timer_event,
        Some(VmTimerEvent::Cancelled { timer_id, .. }) if timer_id == sync_info
    ));
    let quiet = runtime
        .cancel_delayed_send_with_mode(
            actor,
            sync_quiet,
            5,
            VmActorTimerCancelMode::Synchronous {
                include_information: false,
            },
        )
        .expect("sync quiet cancellation");
    assert_eq!(quiet.result, VmActorTimerOptionResult::Acknowledged);
    assert_eq!(quiet.reply_message_id, None);

    let asynchronous = runtime
        .cancel_delayed_send_with_mode(
            actor,
            async_info,
            6,
            VmActorTimerCancelMode::Asynchronous {
                include_information: true,
            },
        )
        .expect("async info cancellation");
    assert_eq!(asynchronous.result, VmActorTimerOptionResult::Acknowledged);
    assert_eq!(asynchronous.reply_message_id, Some(1));
    assert_reply(
        &mut runtime,
        actor,
        "TimerCancelReply",
        async_info.as_u64(),
        remaining_value(14),
    );
    let asynchronous_quiet = runtime
        .cancel_delayed_send_with_mode(
            actor,
            async_quiet,
            6,
            VmActorTimerCancelMode::Asynchronous {
                include_information: false,
            },
        )
        .expect("async quiet cancellation");
    assert_eq!(asynchronous_quiet.reply_message_id, None);

    let stale_sync = runtime
        .cancel_delayed_send_with_mode(
            actor,
            sync_info,
            6,
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            },
        )
        .expect("stale sync cancellation");
    assert_eq!(
        stale_sync.result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Missing)
    );
    let stale_async = runtime
        .cancel_delayed_send_with_mode(
            actor,
            sync_info,
            6,
            VmActorTimerCancelMode::Asynchronous {
                include_information: true,
            },
        )
        .expect("stale async cancellation");
    assert_eq!(stale_async.reply_message_id, Some(2));
    assert_reply(
        &mut runtime,
        actor,
        "TimerCancelReply",
        sync_info.as_u64(),
        ReplValue::Atom("missing".to_string()),
    );
    assert_eq!(runtime.delayed_send_count(), 0);
}

#[test]
fn timer_option_rejections_preserve_active_timer_and_reply_atomicity() {
    let limits = VmMemoryLimits::new(8, 12).expect("limits");
    let mut runtime = VmActorRuntime::with_memory_limits(limits);
    let actor = runtime.spawn_root(source("bounded"));
    let timer = runtime
        .send_after(actor, actor, ReplValue::Unit, 0, 20)
        .expect("timer");
    let missing = VmProcessId::from_raw_for_test(999);

    assert_eq!(
        runtime.cancel_delayed_send_with_mode(
            missing,
            timer,
            5,
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            },
        ),
        Err("missing sender process 999".to_string())
    );
    assert_eq!(runtime.read_delayed_send(timer, 5), Ok(15));
    assert_eq!(
        runtime.read_delayed_send_with_mode(actor, timer, 5, VmActorTimerReadMode::Asynchronous,),
        Err("actor process 1 exceeded its VM mailbox memory hard limit".to_string())
    );
    assert_eq!(runtime.read_delayed_send(timer, 5), Ok(15));
    assert_eq!(
        runtime.cancel_delayed_send_with_mode(
            actor,
            timer,
            5,
            VmActorTimerCancelMode::Asynchronous {
                include_information: true,
            },
        ),
        Err("actor process 1 exceeded its VM mailbox memory hard limit".to_string())
    );
    assert_eq!(runtime.read_delayed_send(timer, 5), Ok(15));
    assert_eq!(runtime.delayed_send_count(), 1);
    assert_eq!(
        runtime
            .receive_next_or_block(actor)
            .expect("failed reply leaves mailbox empty"),
        VmActorReceive::Blocked
    );

    runtime
        .exit_actor(actor, VmExitReason::Normal)
        .expect("exit actor");
    assert_eq!(runtime.delayed_send_count(), 0);
}
