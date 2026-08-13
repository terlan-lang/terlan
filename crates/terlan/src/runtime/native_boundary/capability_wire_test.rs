use std::io::Cursor;

use super::*;

/// Round-trips every owned recursive value through the shared codec.
#[test]
fn capability_wire_round_trips_shared_request_values() {
    let request = CapabilityRequest::Call {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: 7,
        owner_id: 9,
        capability: "example".to_string(),
        operation: "std.example.call".to_string(),
        arguments: vec![CapabilityValue::List(vec![
            CapabilityValue::Unit,
            CapabilityValue::Text("value".to_string()),
            CapabilityValue::Bytes(vec![0, 255]),
            CapabilityValue::Int(-4),
            CapabilityValue::Float(1.5),
            CapabilityValue::Bool(true),
            CapabilityValue::Handle(CapabilityHandle {
                id: 3,
                generation: 2,
            }),
            CapabilityValue::OptionalText(None),
            CapabilityValue::OptionalHandle(Some(CapabilityHandle {
                id: 8,
                generation: 5,
            })),
        ])],
    };
    let mut bytes = Vec::new();
    write_json_frame(&mut bytes, &request, 4_096).expect("bounded request frame");
    let decoded: CapabilityRequest = read_json_frame(&mut Cursor::new(bytes), 4_096)
        .expect("valid request frame")
        .expect("request before EOF");

    assert_eq!(decoded, request);
}

/// Rejects malformed, empty, oversized, and unsupported-version frames.
#[test]
fn capability_wire_fails_closed_at_frame_and_version_limits() {
    let malformed =
        read_json_frame::<CapabilityResponse>(&mut Cursor::new(b"{\"type\":\"reply\"}\n"), 64)
            .expect_err("incomplete response");
    assert!(malformed.contains("capability_worker.frame"));

    let empty = read_json_frame::<CapabilityResponse>(&mut Cursor::new(b"\n"), 64)
        .expect_err("empty frame");
    assert!(empty.contains("capability_worker.frame"));

    let oversized = read_json_frame::<CapabilityResponse>(&mut Cursor::new(b"123456789\n"), 8)
        .expect_err("oversized frame");
    assert!(oversized.contains("payload_limit"));

    assert!(validate_protocol_version(CAPABILITY_PROTOCOL_VERSION).is_ok());
    assert!(validate_protocol_version(CAPABILITY_PROTOCOL_VERSION + 1).is_err());
}

/// Accepts exactly bounded frames and rejects incomplete or malformed bytes.
#[test]
fn capability_wire_reader_enforces_complete_bounded_frames() {
    assert_eq!(
        read_frame_bytes(&mut Cursor::new(b"12345678\n"), 8).expect("exact frame"),
        Some(b"12345678".to_vec())
    );
    assert_eq!(
        read_frame_bytes(&mut Cursor::new(Vec::<u8>::new()), 8).expect("clean EOF"),
        None
    );

    let oversized =
        read_frame_bytes(&mut Cursor::new(b"123456789\n"), 8).expect_err("oversized frame");
    assert!(oversized.contains("payload_limit"));

    let truncated =
        read_frame_bytes(&mut Cursor::new(b"12345678"), 8).expect_err("unterminated frame");
    assert!(truncated.contains("truncated frame"));

    let malformed = read_json_frame::<CapabilityResponse>(&mut Cursor::new([0xff, b'\n']), 8)
        .expect_err("non-UTF-8 JSON frame");
    assert!(malformed.contains("capability_worker.frame"));
}

/// Preserves successful and failed adapter terms across the wire outcome.
#[test]
fn capability_outcomes_preserve_stable_reply_terms() {
    let success = CapabilityOutcome::Ok {
        value: CapabilityValue::List(vec![CapabilityValue::Bool(true)]),
    };
    assert_eq!(
        success.into_reply(),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::List(vec![NativeBoundaryTerm::Bool(
            true
        )]))
    );

    let failure = CapabilityOutcome::Error {
        code: "example.failed".to_string(),
        message: "failed".to_string(),
        offset: 4,
    };
    assert_eq!(
        failure.into_reply(),
        NativeBoundaryReplyTerm::Error {
            code: "example.failed".to_string(),
            message: "failed".to_string(),
            offset: 4,
        }
    );
}

/// Bounds recursive term work even when a serialized request is compact.
#[test]
fn capability_term_budget_rejects_excessive_recursive_work() {
    let accepted = vec![CapabilityValue::List(vec![CapabilityValue::Unit])];
    assert!(validate_capability_term_budget(&accepted).is_ok());

    let rejected = vec![CapabilityValue::List(
        (0..MAX_CAPABILITY_TERM_COUNT)
            .map(|_| CapabilityValue::Unit)
            .collect(),
    )];
    assert!(validate_capability_term_budget(&rejected).is_err());
}

/// Traverses records and lists when finding owned handles and enforcing bounds.
#[test]
fn capability_recursive_records_preserve_ownership_and_budget_limits() {
    let first = CapabilityHandle {
        id: 3,
        generation: 4,
    };
    let second = CapabilityHandle {
        id: 8,
        generation: 9,
    };
    let value = CapabilityValue::Record {
        name: "Nested".to_string(),
        fields: vec![
            ("first".to_string(), CapabilityValue::Handle(first)),
            (
                "list".to_string(),
                CapabilityValue::List(vec![CapabilityValue::OptionalHandle(Some(second))]),
            ),
        ],
    };
    assert_eq!(value.owned_handles(), vec![second, first]);
    assert!(validate_capability_term_budget(&[value]).is_ok());

    let rejected = CapabilityValue::Record {
        name: "Oversized".to_string(),
        fields: (0..MAX_CAPABILITY_TERM_COUNT)
            .map(|index| (index.to_string(), CapabilityValue::Unit))
            .collect(),
    };
    assert!(validate_capability_term_budget(&[rejected]).is_err());
}
