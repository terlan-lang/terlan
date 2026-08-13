use std::panic::{catch_unwind, AssertUnwindSafe};

use super::super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.ReceiveSuiteParity", function, 0)
}

fn receive_selected(
    runtime: &mut VmActorRuntime,
    recipient: VmProcessId,
    predicate: impl FnMut(&VmMessage) -> bool,
) -> VmMessage {
    let VmActorReceive::Message(message) = runtime
        .selective_receive_or_block(recipient, predicate)
        .expect("selective receive")
    else {
        panic!("selected receive must return a message");
    };
    message
}

#[test]
fn receive_suite_large_backlog_priority_wakeup_and_correlation_contract() {
    const BACKLOG_MESSAGES: i64 = 512;

    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let receiver = runtime.spawn_root(source("receiver"));

    for sequence in 0..BACKLOG_MESSAGES {
        runtime
            .send(
                sender,
                receiver,
                ReplValue::Tuple(vec![
                    ReplValue::Atom("backlog".to_string()),
                    ReplValue::Int(sequence),
                ]),
            )
            .expect("queue ordinary backlog message");
    }

    let reductions_before_miss = runtime
        .processes
        .get(receiver)
        .expect("receiver")
        .reductions;
    assert_eq!(
        runtime
            .selective_receive_or_block(receiver, |message| {
                message.payload == ReplValue::Atom("not-present".to_string())
            })
            .expect("scan ordinary backlog"),
        VmActorReceive::Blocked
    );
    assert_eq!(
        runtime
            .processes
            .get(receiver)
            .expect("blocked receiver")
            .state,
        super::super::super::process::VmProcessState::Blocked
    );
    assert_eq!(
        runtime
            .processes
            .get(receiver)
            .expect("receiver after miss")
            .reductions
            - reductions_before_miss,
        BACKLOG_MESSAGES as u64
    );

    for priority in ["first", "second"] {
        runtime
            .send_priority(
                sender,
                receiver,
                ReplValue::Tuple(vec![
                    ReplValue::Atom("priority".to_string()),
                    ReplValue::Atom(priority.to_string()),
                ]),
            )
            .expect("priority reply wakes receiver");
    }
    assert_eq!(
        runtime
            .processes
            .get(receiver)
            .expect("woken receiver")
            .state,
        super::super::super::process::VmProcessState::Runnable
    );

    for expected in ["first", "second"] {
        let message = receive_selected(&mut runtime, receiver, |message| {
            matches!(
                &message.payload,
                ReplValue::Tuple(values)
                    if values.first() == Some(&ReplValue::Atom("priority".to_string()))
            )
        });
        assert_eq!(
            message.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("priority".to_string()),
                ReplValue::Atom(expected.to_string()),
            ])
        );
    }

    runtime
        .send(
            sender,
            receiver,
            ReplValue::Tuple(vec![
                ReplValue::Atom("reply".to_string()),
                ReplValue::Int(42),
            ]),
        )
        .expect("queue correlated reply after backlog");
    assert_eq!(
        receive_selected(&mut runtime, receiver, |message| {
            message.payload
                == ReplValue::Tuple(vec![
                    ReplValue::Atom("reply".to_string()),
                    ReplValue::Int(42),
                ])
        })
        .payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("reply".to_string()),
            ReplValue::Int(42),
        ])
    );
    assert_eq!(
        runtime
            .processes
            .get(receiver)
            .expect("receiver with retained backlog")
            .mailbox_len(),
        BACKLOG_MESSAGES as usize
    );

    for expected_sequence in 0..BACKLOG_MESSAGES {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(receiver)
            .expect("drain retained ordinary backlog")
        else {
            panic!("ordinary backlog must remain queued");
        };
        assert_eq!(
            message.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("backlog".to_string()),
                ReplValue::Int(expected_sequence),
            ])
        );
    }
}

#[test]
fn receive_suite_exception_and_nested_receive_preserve_mailbox_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let receiver = runtime.spawn_root(source("receiver"));

    for payload in ["noise", "abort", "inner", "outer"] {
        runtime
            .send(sender, receiver, ReplValue::Atom(payload.to_string()))
            .expect("queue receive marker regression message");
    }

    let reductions_before_abort = runtime
        .processes
        .get(receiver)
        .expect("receiver")
        .reductions;
    let aborted_receive = catch_unwind(AssertUnwindSafe(|| {
        runtime
            .selective_receive_or_block(receiver, |message| {
                if message.payload == ReplValue::Atom("abort".to_string()) {
                    panic!("simulated receive interruption");
                }
                message.payload == ReplValue::Atom("outer".to_string())
            })
            .expect("interrupted selective receive")
    }));
    assert!(aborted_receive.is_err());
    let receiver_after_abort = runtime
        .processes
        .get(receiver)
        .expect("receiver after abort");
    assert_eq!(receiver_after_abort.mailbox_len(), 4);
    assert_eq!(receiver_after_abort.reductions, reductions_before_abort);

    assert_eq!(
        receive_selected(&mut runtime, receiver, |message| {
            message.payload == ReplValue::Atom("inner".to_string())
        })
        .payload,
        ReplValue::Atom("inner".to_string())
    );
    assert_eq!(
        receive_selected(&mut runtime, receiver, |message| {
            message.payload == ReplValue::Atom("outer".to_string())
        })
        .payload,
        ReplValue::Atom("outer".to_string())
    );

    for expected in ["noise", "abort"] {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(receiver)
            .expect("drain skipped message")
        else {
            panic!("skipped message must remain queued");
        };
        assert_eq!(message.payload, ReplValue::Atom(expected.to_string()));
    }
}
