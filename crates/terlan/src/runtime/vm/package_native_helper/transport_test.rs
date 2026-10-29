use std::io::{BufReader, Cursor};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::transport::{read_message, write_message, Direction, Limits};
use super::{legacy_reply, ReplValue};
use crate::runtime::native_boundary::adapter_abi::{
    PUBLIC_ADAPTER_MAX_FRAME_BYTES, PUBLIC_ADAPTER_MAX_TRANSFER_BYTES,
};

const LIMITS: Limits = Limits {
    frame: PUBLIC_ADAPTER_MAX_FRAME_BYTES,
    transfer: PUBLIC_ADAPTER_MAX_TRANSFER_BYTES,
};

/// Both directions preserve binary payloads larger than a physical frame.
#[test]
fn helper_transport_round_trips_large_requests_and_replies() {
    let bytes = (0..1_420_000_usize * 8)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    for (direction, message) in [
        (
            Direction::Call,
            format!("call 7 b3BlcmF0aW9u x:{}", STANDARD.encode(&bytes)),
        ),
        (
            Direction::Reply,
            format!("reply 7 1 ok_bytes {}", STANDARD.encode(&bytes)),
        ),
    ] {
        let mut wire = Vec::new();
        write_message(&mut wire, &message, 7, direction, LIMITS).expect("write large message");
        assert!(wire.split_inclusive(|byte| *byte == b'\n').count() > 1);
        assert!(wire
            .split_inclusive(|byte| *byte == b'\n')
            .all(|frame| frame.len() <= LIMITS.frame));
        let mut input = BufReader::with_capacity(13, Cursor::new(wire));
        assert_eq!(
            read_message(&mut input, direction, Some(7), LIMITS).expect("read large message"),
            Some(message)
        );
        assert!(read_message(&mut input, direction, None, LIMITS)
            .expect("read EOF")
            .is_none());
    }
}

/// Old exporters remain usable without ever buffering their entire base64 line.
#[test]
fn helper_transport_streams_large_legacy_binary_replies() {
    let bytes = vec![193_u8; 1_420_000 * 8];
    for kind in ["ok_bytes", "result_ok_bytes"] {
        let wire = format!(
            "reply 7 1 {kind} {}\r\nreply 8 1 ok_int 9\n",
            STANDARD.encode(&bytes)
        );
        let mut input = BufReader::with_capacity(17, Cursor::new(wire));
        let value = legacy_reply::read_reply(&mut input, 7, LIMITS).expect("large legacy reply");
        let expected = ReplValue::Bytes(bytes.clone().into());
        assert_eq!(
            value,
            if kind == "result_ok_bytes" {
                super::support::result_ok(expected)
            } else {
                expected
            }
        );
        assert_eq!(
            legacy_reply::read_reply(&mut input, 8, LIMITS).expect("next reply"),
            ReplValue::Int(9)
        );
    }
}

#[test]
fn helper_transport_reads_chunked_bytes_through_vm_reply_decoder() {
    let bytes = vec![7_u8; LIMITS.frame + 31];
    let mut wire = Vec::new();
    write_message(
        &mut wire,
        &format!("reply 1 1 result_ok_bytes {}", STANDARD.encode(&bytes)),
        1,
        Direction::Reply,
        LIMITS,
    )
    .expect("chunk reply");
    assert_eq!(
        legacy_reply::read_reply(&mut Cursor::new(wire), 1, LIMITS).expect("decode reply"),
        super::support::result_ok(ReplValue::Bytes(bytes.into()))
    );
}

#[test]
fn helper_transport_rejects_bad_sequences_and_partial_transfers() {
    let first = format!("reply_chunk 7 0 0 {}\n", STANDARD.encode("reply 7 1 "));
    for suffix in [
        String::new(),
        format!("reply_chunk 7 0 1 {}\n", STANDARD.encode("ok_int 9")),
        format!("reply_chunk 8 1 1 {}\n", STANDARD.encode("ok_int 9")),
        format!("reply_chunk 7 2 1 {}\n", STANDARD.encode("ok_int 9")),
        "reply 7 1 ok_int 9\n".to_string(),
        "reply_chunk 7 1 2 eA==\n".to_string(),
        "reply_chunk 7 1 1 !!!\n".to_string(),
        "reply_chunk 7 1 1 eA== extra\n".to_string(),
        "reply_chunk 7 1 1 eA==".to_string(),
    ] {
        let wire = format!("{first}{suffix}");
        assert!(
            read_message(&mut Cursor::new(wire), Direction::Reply, Some(7), LIMITS).is_err(),
            "accepted {suffix}"
        );
    }
    for message in ["reply 8 1 ok_int 9", "reply 7 1 ok_int 9\ncall 8 b3A="] {
        let wire = format!("reply_chunk 7 0 1 {}\n", STANDARD.encode(message));
        assert!(read_message(&mut Cursor::new(wire), Direction::Reply, Some(7), LIMITS).is_err());
    }
}

#[test]
fn helper_transport_enforces_frame_message_and_binary_bounds() {
    let small = Limits {
        frame: 256,
        transfer: 768,
    };
    assert!(read_message(
        &mut Cursor::new(vec![b'x'; small.frame + 1]),
        Direction::Reply,
        None,
        small
    )
    .is_err());
    assert!(write_message(
        &mut Vec::new(),
        &"x".repeat(small.message() + 1),
        1,
        Direction::Reply,
        small
    )
    .is_err());
    // Several individually valid frames must not bypass the logical bound.
    let wire = (0..20)
        .map(|index| format!("reply_chunk 1 {index} 0 {}\n", STANDARD.encode([b'x'; 100])))
        .collect::<String>();
    assert!(read_message(&mut Cursor::new(wire), Direction::Reply, Some(1), small).is_err());
    for (wire, id) in [
        (
            format!(
                "reply 1 1 ok_bytes {}\n",
                STANDARD.encode(vec![0_u8; small.transfer + 1])
            ),
            1,
        ),
        ("reply 2 1 ok_bytes eA==\n".into(), 1),
        ("reply 1 1 ok_bytes eA==junk\n".into(), 1),
        ("reply 1 1 ok_bytes eA==".into(), 1),
    ] {
        assert!(legacy_reply::read_reply(&mut Cursor::new(wire), id, small).is_err());
    }
}
