use super::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.MessageQueueData", name, 0)
}

/// Replaces OTP's mutable on-heap/off-heap message-queue representation tests.
///
/// Terlan exposes one VM-owned mailbox storage policy. Observable message
/// identity, ordering, payload ownership, inspection counts, and logical
/// mailbox charges must remain stable regardless of internal allocation.
#[test]
fn mailbox_storage_preserves_mixed_payloads_and_separates_heap_accounting() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source("sender"));
    let recipient = processes.spawn_root(source("recipient"));
    let mut expected = Vec::new();
    let mut expected_bytes = 0usize;

    for batch in 0..4 {
        let payloads = [
            ReplValue::Atom(format!("batch_{batch}")),
            ReplValue::Int(batch),
            ReplValue::Tuple(vec![
                ReplValue::Atom("tag".to_string()),
                ReplValue::Int(batch),
            ]),
            ReplValue::List(vec![ReplValue::Int(batch), ReplValue::Int(batch + 1)]),
        ];
        for (index, payload) in payloads.into_iter().enumerate() {
            let accounted_bytes = (batch as usize + 1) * (index + 1);
            let id = processes
                .send_accounted(sender, recipient, payload.clone(), accounted_bytes)
                .expect("mixed mailbox payload should enqueue");
            expected_bytes += accounted_bytes;
            expected.push((id, payload, accounted_bytes));
        }

        let snapshot = processes
            .snapshot(recipient)
            .expect("recipient snapshot should remain inspectable");
        assert_eq!(snapshot.mailbox_messages, expected.len());
        assert_eq!(
            snapshot.heap_bytes, 0,
            "mailbox allocation must not silently become process heap ownership"
        );
        assert_eq!(
            processes
                .get(recipient)
                .expect("recipient should remain live")
                .mailbox_accounted_bytes()
                .expect("bounded mailbox charges should sum"),
            expected_bytes
        );
    }

    let recipient_process = processes
        .get_mut(recipient)
        .expect("recipient should remain live");
    for (expected_id, expected_payload, expected_charge) in expected {
        let message = recipient_process
            .receive_next()
            .expect("every queued message should remain available");
        assert_eq!(message.id, expected_id);
        assert_eq!(message.sender, sender);
        assert_eq!(message.payload, expected_payload);
        assert_eq!(message.accounted_bytes, expected_charge);
    }
    assert_eq!(recipient_process.mailbox_len(), 0);
    assert_eq!(
        recipient_process
            .mailbox_accounted_bytes()
            .expect("empty mailbox accounting should remain valid"),
        0
    );
}
