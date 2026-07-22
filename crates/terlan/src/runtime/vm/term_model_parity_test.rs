use super::super::process::{VmProcessSource, VmProcessTable};
use super::super::{map_value::VmMapValue, ReplValue};
use super::{logical_value_bytes, VmMemoryAccountant, VmMemoryLimits};

/// Verifies portable structured-term behavior from the historical VM suite.
///
/// Inputs:
/// - Empty and populated byte, tuple, list, and insertion-ordered map values.
///
/// Output:
/// - Exact cardinalities, stable map order, and replacement without reordering.
///
/// Transformation:
/// - Ports value semantics while intentionally omitting retired BEAM word tags
///   and shallow-term headers from the typed Terlan VM model.
#[test]
fn term_model_preserves_aggregate_cardinality_order_and_replacement() {
    let empty_bytes = ReplValue::Bytes(Vec::new().into());
    let bytes = ReplValue::Bytes(vec![1, 2, 3].into());
    let empty_tuple = ReplValue::Tuple(Vec::new());
    let tuple = ReplValue::Tuple(vec![ReplValue::Int(1), ReplValue::Int(2)]);
    let empty_list = ReplValue::List(Vec::new());
    let list = ReplValue::List(vec![ReplValue::Int(3)]);

    assert_eq!(byte_len(&empty_bytes), Some(0));
    assert_eq!(byte_len(&bytes), Some(3));
    assert_eq!(aggregate_len(&empty_tuple), Some(0));
    assert_eq!(aggregate_len(&tuple), Some(2));
    assert_eq!(aggregate_len(&empty_list), Some(0));
    assert_eq!(aggregate_len(&list), Some(1));
    assert_eq!(aggregate_len(&ReplValue::Int(1)), None);

    let atom_key = ReplValue::Atom("key".to_string());
    let int_key = ReplValue::Int(1);
    let mut map = VmMapValue::from_entries(vec![
        (atom_key.clone(), ReplValue::Int(9)),
        (int_key.clone(), ReplValue::Int(2)),
    ]);
    assert_eq!(map.len(), 2);
    assert_eq!(map.lookup(&atom_key), Some(&ReplValue::Int(9)));

    map.insert_or_replace(atom_key.clone(), ReplValue::Int(10));
    assert_eq!(map.len(), 2);
    assert_eq!(map.lookup(&atom_key), Some(&ReplValue::Int(10)));
    assert_eq!(
        map.to_entries(),
        vec![(atom_key, ReplValue::Int(10)), (int_key, ReplValue::Int(2)),]
    );
}

/// Verifies structured VM values have deterministic retained-size estimates.
///
/// Inputs:
/// - Empty aggregates and one nested tuple containing scalar, byte, and map
///   values.
///
/// Output:
/// - Exact logical byte counts independent of host pointer size or BEAM words.
///
/// Transformation:
/// - Replaces historical heap-word estimates with the VM-owned logical memory
///   accounting contract used for pressure decisions and actor messages.
#[test]
fn term_model_accounts_structured_values_without_backend_word_assumptions() {
    for value in [
        ReplValue::Bytes(Vec::new().into()),
        ReplValue::Tuple(Vec::new()),
        ReplValue::List(Vec::new()),
        ReplValue::Map(Vec::new()),
    ] {
        assert_eq!(logical_value_bytes(&value), Ok(16));
    }

    let map = VmMapValue::from_entries(vec![
        (ReplValue::Atom("atom-key".to_string()), ReplValue::Int(9)),
        (ReplValue::Int(1), ReplValue::Int(2)),
    ]);
    let map_value = ReplValue::MapIndexed(map);
    assert_eq!(logical_value_bytes(&map_value), Ok(96));

    let nested = ReplValue::Tuple(vec![
        ReplValue::Int(1),
        ReplValue::Bytes(vec![1, 2, 3, 4, 5].into()),
        map_value,
    ]);
    assert_eq!(logical_value_bytes(&nested), Ok(165));
}

/// Verifies persistent compound values cannot alias mutable actor state.
///
/// Inputs:
/// - An indexed VM map at the A-CHAMP activation boundary, a structurally
///   shared clone, and a nested actor-message payload containing that clone.
///
/// Output:
/// - Independent sender updates, an unchanged shared snapshot, and an exact
///   mailbox value after the sender continues mutating its own map.
///
/// Transformation:
/// - Replaces the historical BEAM owner-wrapper and copied-word assertions
///   with Terlan's actual ownership contract: immutable structural sharing is
///   permitted, but persistent updates and actor delivery cannot expose
///   mutable aliases across value owners.
#[test]
fn term_model_isolates_shared_compound_values_across_actor_delivery() {
    let entries = (0..128)
        .map(|index| (ReplValue::Int(index), ReplValue::Int(index * 10)))
        .collect::<Vec<_>>();
    let shared_snapshot = VmMapValue::from_entries(entries);
    let mut sender_map = shared_snapshot.clone();

    sender_map.insert_or_replace(ReplValue::Int(0), ReplValue::Int(999));
    sender_map.insert_or_replace(ReplValue::Int(128), ReplValue::Int(1_280));
    assert_eq!(
        shared_snapshot.lookup(&ReplValue::Int(0)),
        Some(&ReplValue::Int(0))
    );
    assert_eq!(shared_snapshot.lookup(&ReplValue::Int(128)), None);
    assert_eq!(
        sender_map.lookup(&ReplValue::Int(0)),
        Some(&ReplValue::Int(999))
    );

    let payload = ReplValue::Tuple(vec![
        ReplValue::Atom("compound".to_string()),
        ReplValue::List(vec![ReplValue::MapIndexed(shared_snapshot.clone())]),
    ]);
    let expected = payload.clone();
    let payload_bytes = logical_value_bytes(&payload).expect("compound payload size");
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(VmProcessSource::new("owner", "sender", 0));
    let recipient = processes.spawn_root(VmProcessSource::new("owner", "recipient", 0));
    let limits = VmMemoryLimits::new(payload_bytes, payload_bytes).expect("exact limits");
    let mut memory = VmMemoryAccountant::new(limits);

    let sent = memory
        .send_value_message(&mut processes, sender, recipient, payload)
        .expect("send compound value");
    assert_eq!(sent.published_message_id(), Some(1));

    sender_map.insert_or_replace(ReplValue::Int(1), ReplValue::Int(-1));
    let received = memory
        .receive_message(&mut processes, recipient)
        .expect("receive compound value")
        .expect("compound message");
    assert_eq!(received.payload, expected);
    assert_eq!(received.accounted_bytes, payload_bytes);
    assert_eq!(
        shared_snapshot.lookup(&ReplValue::Int(1)),
        Some(&ReplValue::Int(10))
    );
}

fn byte_len(value: &ReplValue) -> Option<usize> {
    match value {
        ReplValue::Bytes(bytes) => Some(bytes.len()),
        _ => None,
    }
}

fn aggregate_len(value: &ReplValue) -> Option<usize> {
    match value {
        ReplValue::Tuple(items) | ReplValue::List(items) => Some(items.len()),
        _ => None,
    }
}
