//! Actor-message replacements for portable `native_record_SUITE` contracts.

use super::{VmActorReceive, VmActorRuntime};
use crate::runtime::vm::process::VmProcessSource;
use crate::runtime::vm::ReplValue;

const MESSAGES: i64 = 1_000;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("native_record_suite", function, 0)
}

fn record(sequence: i64) -> ReplValue {
    ReplValue::Record {
        name: "native_record_suite.Envelope".to_string(),
        fields: vec![
            ("sequence".to_string(), ReplValue::Int(sequence)),
            (
                "body".to_string(),
                ReplValue::Record {
                    name: "native_record_suite.Pair".to_string(),
                    fields: vec![
                        ("left".to_string(), ReplValue::Int(sequence)),
                        ("right".to_string(), ReplValue::Int(MESSAGES - sequence)),
                    ],
                },
            ),
        ],
    }
}

#[test]
fn native_record_suite_actor_mailbox_preserves_one_thousand_records() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let receiver = runtime.spawn_root(source("receiver"));

    for sequence in 1..=MESSAGES {
        assert_eq!(
            runtime
                .send(sender, receiver, record(sequence))
                .expect("record send"),
            sequence as u64
        );
    }
    assert_eq!(
        runtime
            .processes()
            .get(receiver)
            .expect("receiver")
            .mailbox_len(),
        MESSAGES as usize
    );

    for sequence in 1..=MESSAGES {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(receiver)
            .expect("record receive")
        else {
            panic!("record receive must return a message");
        };
        assert_eq!(message.sender, sender);
        assert_eq!(message.payload, record(sequence));
    }
    assert_eq!(
        runtime
            .processes()
            .get(receiver)
            .expect("receiver")
            .mailbox_len(),
        0
    );
    assert_eq!(
        runtime
            .memory_metrics(receiver)
            .expect("receiver memory")
            .current_bytes,
        0
    );
}
