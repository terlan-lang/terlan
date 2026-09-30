use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error as _;
use std::rc::Rc;

use tungstenite::protocol::frame::{coding::Data, coding::OpCode, Frame};

use super::*;

#[derive(Default)]
struct Buffers {
    inbound: VecDeque<u8>,
    written: Vec<u8>,
    write_budget: Option<usize>,
}

#[derive(Clone, Default)]
struct Memory(Rc<RefCell<Buffers>>);

impl Read for Memory {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        if state.inbound.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = output.len().min(state.inbound.len());
        for byte in &mut output[..count] {
            *byte = state.inbound.pop_front().unwrap();
        }
        Ok(count)
    }
}

impl Write for Memory {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        if state.write_budget == Some(0) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(state.write_budget.unwrap_or(usize::MAX));
        state.written.extend_from_slice(&bytes[..count]);
        if let Some(remaining) = &mut state.write_budget {
            *remaining -= count;
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encoded(role: Role, messages: Vec<Message>) -> Vec<u8> {
    let memory = Memory::default();
    let mut peer = WebSocket::from_raw_socket(memory.clone(), role, None);
    for message in messages {
        peer.send(message).unwrap();
    }
    let bytes = memory.0.borrow().written.clone();
    bytes
}

fn incoming(bytes: Vec<u8>, limit: usize) -> (Server<Memory>, Memory) {
    let memory = Memory::default();
    memory.0.borrow_mut().inbound.extend(bytes);
    (Server::new(memory.clone(), limit), memory)
}

fn read_output(memory: &Memory) -> WebSocket<Memory> {
    let peer = Memory::default();
    peer.0
        .borrow_mut()
        .inbound
        .extend(memory.0.borrow().written.iter().copied());
    WebSocket::from_raw_socket(peer, Role::Client, None)
}

#[test]
fn maintained_messages_and_automatic_pong_use_the_package_codec() {
    let messages = vec![
        Message::text("hello"),
        Message::binary(vec![0, 255]),
        Message::Ping(vec![1, 2].into()),
        Message::Pong(vec![3].into()),
        Message::Close(None),
    ];
    let (mut server, memory) = incoming(encoded(Role::Client, messages.clone()), 32);
    for message in messages {
        assert_eq!(server.read().unwrap(), message);
        match server.flush() {
            Ok(()) => {}
            Err(error) => assert_eq!(error.kind(), ErrorKind::Closed),
        }
    }
    let mut peer = read_output(&memory);
    assert_eq!(peer.read().unwrap(), Message::Pong(vec![1, 2].into()));
    assert_eq!(peer.read().unwrap(), Message::Close(None));
    assert_eq!(server.read().unwrap_err().kind(), ErrorKind::Closed);
}

#[test]
fn partial_frames_survive_every_read_boundary_without_duplicate_messages() {
    let bytes = encoded(Role::Client, vec![Message::text("hello world")]);
    for split in 0..bytes.len() {
        let (mut server, memory) = incoming(bytes[..split].to_vec(), 11);
        assert_eq!(server.read().unwrap_err().kind(), ErrorKind::WouldBlock);
        memory.0.borrow_mut().inbound.extend(&bytes[split..]);
        assert_eq!(server.read().unwrap(), Message::text("hello world"));
        assert_eq!(server.read().unwrap_err().kind(), ErrorKind::WouldBlock);
    }
}

#[test]
fn endpoint_limits_apply_before_delivery_to_frames_and_fragmented_messages() {
    for payload in ["", "a", "abc", "abcd"] {
        let (mut server, _) = incoming(encoded(Role::Client, vec![Message::text(payload)]), 3);
        match server.read() {
            Ok(value) => {
                assert!(payload.len() <= 3);
                assert_eq!(value, Message::text(payload));
            }
            Err(error) => {
                assert!(payload.len() > 3);
                assert_eq!(error.kind(), ErrorKind::Failed);
                assert!(matches!(error.0, tungstenite::Error::Capacity(_)));
            }
        }
    }
    for limit in [3, 4] {
        let fragments = vec![
            Message::Frame(Frame::message(
                b"ab".to_vec(),
                OpCode::Data(Data::Text),
                false,
            )),
            Message::Frame(Frame::message(
                b"cd".to_vec(),
                OpCode::Data(Data::Continue),
                true,
            )),
        ];
        let (mut server, _) = incoming(encoded(Role::Client, fragments), limit);
        if limit == 4 {
            assert_eq!(server.read().unwrap(), Message::text("abcd"));
        } else {
            assert!(matches!(
                server.read().unwrap_err().0,
                tungstenite::Error::Capacity(_)
            ));
        }
    }
}

#[test]
fn oversized_declared_length_is_rejected_without_waiting_for_payload() {
    let bytes = encoded(Role::Client, vec![Message::text("x".repeat(256))]);
    let (mut server, _) = incoming(bytes[..8].to_vec(), 16);
    assert!(matches!(
        server.read().unwrap_err().0,
        tungstenite::Error::Capacity(_)
    ));
}

#[test]
fn unmasked_frames_and_invalid_utf8_remain_maintained_parser_errors() {
    let invalid_text = Message::Frame(Frame::message(vec![255], OpCode::Data(Data::Text), true));
    for bytes in [
        encoded(Role::Server, vec![Message::text("unmasked")]),
        encoded(Role::Client, vec![invalid_text]),
    ] {
        let (mut server, _) = incoming(bytes, 32);
        assert_eq!(server.read().unwrap_err().kind(), ErrorKind::Failed);
    }
}

#[test]
fn partial_writes_resume_by_flush_without_resubmitting_accepted_text() {
    let memory = Memory::default();
    memory.0.borrow_mut().write_budget = Some(3);
    let mut server = Server::new(memory.clone(), 32);
    let payload = "x".repeat(128 * 1024);
    assert_eq!(
        server.write_text(payload.clone()).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(memory.0.borrow().written.len(), 3);
    assert_eq!(server.flush().unwrap_err().kind(), ErrorKind::WouldBlock);
    memory.0.borrow_mut().write_budget = None;
    server.flush().unwrap();
    let mut peer = read_output(&memory);
    assert_eq!(peer.read().unwrap(), Message::text(payload));
    assert!(
        matches!(peer.read(), Err(tungstenite::Error::Io(error)) if error.kind() == io::ErrorKind::WouldBlock)
    );
}

#[test]
fn queued_text_and_unsupported_close_preserve_wire_policy() {
    let (mut server, memory) = incoming(vec![], 32);
    server.write_text("accepted".into()).unwrap();
    server.flush().unwrap();
    server.close_unsupported().unwrap();
    server.flush().unwrap();
    let mut peer = read_output(&memory);
    assert_eq!(peer.read().unwrap(), Message::text("accepted"));
    assert_eq!(
        peer.read().unwrap(),
        Message::Close(Some(CloseFrame {
            code: CloseCode::Unsupported,
            reason: "unsupported channel payload".into(),
        }))
    );
}

#[test]
fn classification_preserves_transport_categories_and_error_sources() {
    for (kind, expected) in [
        (io::ErrorKind::WouldBlock, ErrorKind::WouldBlock),
        (io::ErrorKind::Interrupted, ErrorKind::Interrupted),
        (io::ErrorKind::BrokenPipe, ErrorKind::Disconnected),
        (io::ErrorKind::ConnectionAborted, ErrorKind::Disconnected),
        (io::ErrorKind::ConnectionReset, ErrorKind::Disconnected),
        (io::ErrorKind::NotConnected, ErrorKind::Disconnected),
        (io::ErrorKind::UnexpectedEof, ErrorKind::Disconnected),
        (io::ErrorKind::PermissionDenied, ErrorKind::Failed),
    ] {
        let error = Error(tungstenite::Error::Io(kind.into()));
        assert_eq!(error.kind(), expected);
        assert_eq!(error.to_string(), error.0.to_string());
        assert!(error.source().is_some());
    }
    for error in [
        tungstenite::Error::ConnectionClosed,
        tungstenite::Error::AlreadyClosed,
    ] {
        assert_eq!(Error(error).kind(), ErrorKind::Closed);
    }
}

#[test]
fn handshake_response_uses_maintained_hash_and_rejects_empty_keys() {
    let response = upgrade_response(" \tdGhlIHNhbXBsZSBub25jZQ==\r\n").unwrap();
    assert_eq!(response.status, 101);
    assert_eq!(
        response.headers,
        vec![
            ("upgrade".into(), "websocket".into()),
            ("connection".into(), "Upgrade".into()),
            (
                "sec-websocket-accept".into(),
                "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=".into()
            ),
        ]
    );
    for key in ["", " \t\r\n", "\u{2003}"] {
        let error = upgrade_response(key).unwrap_err();
        assert_eq!(error.code(), "http.websocket.key");
        assert_eq!(error.message(), "missing Sec-WebSocket-Key");
        assert_eq!(error.status(), 400);
    }
}
