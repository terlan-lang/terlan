
/// Verifies active runtime close sends close frame and releases VM TCP stream.
///
/// Inputs:
/// - One registered open WebSocket session.
///
/// Output:
/// - Test passes when the client receives a close frame, the session is
///   removed, and the VM TCP stream is closed.
///
/// Transformation:
/// - Gives production shutdown a single VM-owned close/remove/release path.
#[test]
fn vm_websocket_runtime_close_session_and_stream_sends_close_for_open_session() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let final_state = runtime
        .close_session_and_stream(&mut tcp, session)
        .expect("close session");
    let close = tcp
        .receive(client, 4096)
        .expect("receive close")
        .expect("queued close frame");

    assert_eq!(
        decode_server_control_frame(&close),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        final_state,
        VmWebSocketSessionInfo {
            stream: server,
            open: false,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: close.len(),
            bytes_received: 0,
        }
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
    assert_eq!(
        runtime
            .inspect_session(session)
            .expect_err("removed session"),
        "VM WebSocket session handle is unknown"
    );
}

/// Verifies scheduler timeout termination is graceful and inspectable.
///
/// Inputs:
/// - One registered open WebSocket session.
/// - Timeout termination reason.
///
/// Output:
/// - Test passes when timeout termination sends a close frame, closes the VM
///   TCP stream, removes the session, and returns the timeout reason.
///
/// Transformation:
/// - Gives scheduler timeout handling a typed lifecycle path instead of an
///   anonymous close call.
#[test]
fn vm_websocket_runtime_timeout_termination_sends_close_and_reason() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let termination = runtime
        .terminate_session_and_stream(&mut tcp, session, VmWebSocketTerminationReason::Timeout)
        .expect("timeout termination");
    let close = tcp
        .receive(client, 4096)
        .expect("receive close")
        .expect("queued close frame");

    assert_eq!(
        decode_server_control_frame(&close),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        termination,
        VmWebSocketTermination {
            session,
            reason: VmWebSocketTerminationReason::Timeout,
            info: VmWebSocketSessionInfo {
                stream: server,
                open: false,
                frames_sent: 1,
                frames_received: 0,
                bytes_sent: close.len(),
                bytes_received: 0,
            },
        }
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies scheduler cancellation termination is abrupt and inspectable.
///
/// Inputs:
/// - One registered open WebSocket session.
/// - Cancellation termination reason.
///
/// Output:
/// - Test passes when cancellation removes the session, cancels the VM TCP
///   stream, sends no close frame, and returns the cancellation reason.
///
/// Transformation:
/// - Gives actor cancellation a separate WebSocket lifecycle from graceful
///   timeout shutdown.
#[test]
fn vm_websocket_runtime_cancelled_termination_cancels_stream_without_close_frame() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let termination = runtime
        .terminate_session_and_stream(&mut tcp, session, VmWebSocketTerminationReason::Cancelled)
        .expect("cancelled termination");

    assert_eq!(
        termination,
        VmWebSocketTermination {
            session,
            reason: VmWebSocketTerminationReason::Cancelled,
            info: VmWebSocketSessionInfo {
                stream: server,
                open: true,
                frames_sent: 0,
                frames_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
            },
        }
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(
        tcp.inspect_stream(server)
            .expect("inspect stream")
            .cancelled
    );
    assert_eq!(tcp.receive(client, 4096).expect("client receive"), None);
}

/// Verifies active runtime close does not send a duplicate close frame.
///
/// Inputs:
/// - One session already closed by a client close frame.
///
/// Output:
/// - Test passes when runtime cleanup removes and closes the stream without
///   queuing a second close frame to the client.
///
/// Transformation:
/// - Keeps shutdown idempotent after peer-initiated close.
#[test]
fn vm_websocket_runtime_close_session_and_stream_skips_close_for_closed_session() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);
    let close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    let close_len = close.len();
    tcp.send(client, close).expect("send client close");
    runtime
        .receive_frame(&mut tcp, session, 4096)
        .expect("receive close")
        .expect("close event");

    let final_state = runtime
        .close_session_and_stream(&mut tcp, session)
        .expect("close closed session");

    assert_eq!(
        final_state,
        VmWebSocketSessionInfo {
            stream: server,
            open: false,
            frames_sent: 0,
            frames_received: 1,
            bytes_sent: 0,
            bytes_received: close_len,
        }
    );
    assert_eq!(tcp.receive(client, 4096).expect("client receive"), None);
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies closing an already-closed raw session wraps invalid TCP handles.
///
/// Inputs:
/// - One raw WebSocket session whose stream handle does not exist in VM TCP.
/// - The raw session is marked closed, so WebSocket close-frame send is
///   skipped and stream release is attempted directly.
///
/// Output:
/// - Test passes when stream release failure is reported through a stable
///   WebSocket-layer diagnostic.
///
/// Transformation:
/// - Pins the failure mode used by supervisor cleanup when stale raw test
///   fixtures or future recovery code hold invalid stream references.
#[test]
fn vm_websocket_runtime_close_session_and_stream_reports_invalid_stream_release() {
    let mut tcp = VmTcpRuntime::new();
    let mut runtime = VmWebSocketRuntime::new();
    let stale_stream = VmTcpStream::test_handle(525_252);
    let session = runtime.open_session(stale_stream);
    runtime
        .sessions
        .get_mut(&session.id)
        .expect("raw session")
        .open = false;

    let err = runtime
        .close_session_and_stream(&mut tcp, session)
        .expect_err("invalid release rejected");

    assert_eq!(
        err,
        "error[vm_websocket_tcp]: failed to close session stream: VM TCP stream handle is unknown"
    );
    assert_eq!(runtime.session_count(), 0);
}

/// Verifies transport-owned stream shutdown can close its WebSocket session.
///
/// Inputs:
/// - One checked-open WebSocket session bound to a VM TCP stream.
///
/// Output:
/// - Test passes when closing by stream returns the session handle, sends a
///   close frame to the client, removes the session, and closes the VM TCP
///   stream.
///
/// Transformation:
/// - Lets VM transport cleanup close WebSocket ownership by stream handle
///   without exposing the runtime session registry.
#[test]
fn vm_websocket_runtime_close_stream_session_and_stream_closes_bound_session() {
    let mut tcp = VmTcpRuntime::new();
    let (client, server) = connected_tcp_pair_at(&mut tcp, "websocket.close.stream.bound");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime
        .open_session_checked(&tcp, server)
        .expect("checked open");

    let closed = runtime
        .close_stream_session_and_stream(&mut tcp, server)
        .expect("close by stream")
        .expect("bound session");
    let frame = tcp
        .receive(client, 4096)
        .expect("receive close frame")
        .expect("queued close frame");

    assert_eq!(closed.0, session);
    assert_eq!(
        decode_server_control_frame(&frame),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        closed.1,
        VmWebSocketSessionInfo {
            stream: server,
            open: false,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: frame.len(),
            bytes_received: 0,
        }
    );
    assert_eq!(runtime.session_for_stream(server), None);
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies stream-based session shutdown is a no-op for unbound streams.
///
/// Inputs:
/// - One bound WebSocket session and one unrelated VM TCP stream.
///
/// Output:
/// - Test passes when closing the unrelated stream returns `None` and leaves
///   the tracked WebSocket session open.
///
/// Transformation:
/// - Keeps transport cleanup idempotent when stream close events arrive for
///   non-WebSocket or already detached VM TCP streams.
#[test]
fn vm_websocket_runtime_close_stream_session_and_stream_ignores_unbound_stream() {
    let mut tcp = VmTcpRuntime::new();
    let (_client, server) = connected_tcp_pair_at(&mut tcp, "websocket.close.stream.bound");
    let (_other_client, other_server) =
        connected_tcp_pair_at(&mut tcp, "websocket.close.stream.other");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime
        .open_session_checked(&tcp, server)
        .expect("checked open");

    let closed = runtime
        .close_stream_session_and_stream(&mut tcp, other_server)
        .expect("close unbound stream");

    assert_eq!(closed, None);
    assert_eq!(runtime.session_for_stream(server), Some(session));
    assert_eq!(
        runtime.inspect_session(session).expect("inspect session"),
        VmWebSocketSessionInfo {
            stream: server,
            open: true,
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    );
    assert!(!tcp.inspect_stream(server).expect("inspect bound").closed);
    assert!(
        !tcp.inspect_stream(other_server)
            .expect("inspect other")
            .closed
    );
}

/// Verifies selected-session shutdown closes mixed lifecycle sessions.
///
/// Inputs:
/// - One open session and one session already closed by the peer.
///
/// Output:
/// - Test passes when both sessions are removed, both VM TCP streams are
///   closed, and only the open session receives a server close frame.
///
/// Transformation:
/// - Gives room/session actors a generic VM-owned shutdown path for selected
///   WebSocket handles without duplicating TCP cleanup logic.
#[test]
fn vm_websocket_runtime_close_sessions_and_streams_handles_selected_sessions() {
    let mut tcp = VmTcpRuntime::new();
    let (open_client, open_server) = connected_tcp_pair_at(&mut tcp, "websocket.close.open");
    let (closed_client, closed_server) = connected_tcp_pair_at(&mut tcp, "websocket.close.closed");
    let mut runtime = VmWebSocketRuntime::new();
    let open = runtime.open_session(open_server);
    let closed = runtime.open_session(closed_server);
    let close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    let close_len = close.len();
    tcp.send(closed_client, close).expect("send client close");
    runtime
        .receive_frame(&mut tcp, closed, 4096)
        .expect("receive close")
        .expect("closed event");

    let final_states = runtime
        .close_sessions_and_streams(&mut tcp, &[open, closed])
        .expect("close selected sessions");
    let open_close = tcp
        .receive(open_client, 4096)
        .expect("receive open close")
        .expect("queued open close");

    assert_eq!(
        decode_server_control_frame(&open_close),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        final_states,
        vec![
            (
                open,
                VmWebSocketSessionInfo {
                    stream: open_server,
                    open: false,
                    frames_sent: 1,
                    frames_received: 0,
                    bytes_sent: open_close.len(),
                    bytes_received: 0,
                },
            ),
            (
                closed,
                VmWebSocketSessionInfo {
                    stream: closed_server,
                    open: false,
                    frames_sent: 0,
                    frames_received: 1,
                    bytes_sent: 0,
                    bytes_received: close_len,
                },
            ),
        ]
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(
        tcp.inspect_stream(open_server)
            .expect("inspect open")
            .closed
    );
    assert!(
        tcp.inspect_stream(closed_server)
            .expect("inspect closed")
            .closed
    );
    assert_eq!(
        tcp.receive(closed_client, 4096)
            .expect("closed client receive"),
        None
    );
}

/// Verifies selected-session shutdown rejects duplicate handles before close.
///
/// Inputs:
/// - One open session repeated twice in a close set.
///
/// Output:
/// - Test passes when the duplicate set is rejected and no close frame is
///   queued to the client.
///
/// Transformation:
/// - Keeps group shutdown validation deterministic before resource mutation.
#[test]
fn vm_websocket_runtime_close_sessions_rejects_duplicate_without_partial_close() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let err = runtime
        .close_sessions_and_streams(&mut tcp, &[session, session])
        .expect_err("duplicate close rejected");

    assert_eq!(
        err,
        "error[vm_websocket_session]: duplicate session handle in close set"
    );
    assert_eq!(tcp.receive(client, 4096).expect("client receive"), None);
    assert_eq!(runtime.session_count(), 1);
    assert!(!tcp.inspect_stream(server).expect("inspect stream").closed);
    assert!(runtime.inspect_session(session).expect("inspect").open);
}

/// Verifies best-effort selected shutdown reports partial success.
///
/// Inputs:
/// - One open WebSocket session and one fabricated unknown session handle.
///
/// Output:
/// - Test passes when the valid session is closed and the unknown handle is
///   reported as a per-session error.
///
/// Transformation:
/// - Gives actor/supervisor cleanup a bulk shutdown surface that does not
///   abandon valid sessions after one stale handle is encountered.
#[test]
fn vm_websocket_runtime_best_effort_close_sessions_reports_partial_results() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);
    let unknown = VmWebSocketSessionId { id: 99_999 };

    let outcomes = runtime.close_sessions_and_streams_best_effort(&mut tcp, &[unknown, session]);
    let close = tcp
        .receive(client, 4096)
        .expect("receive close")
        .expect("queued close frame");

    assert_eq!(
        decode_server_control_frame(&close),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        outcomes,
        vec![
            VmWebSocketCloseOutcome {
                session: unknown,
                result: Err("VM WebSocket session handle is unknown".to_string()),
            },
            VmWebSocketCloseOutcome {
                session,
                result: Ok(VmWebSocketSessionInfo {
                    stream: server,
                    open: false,
                    frames_sent: 1,
                    frames_received: 0,
                    bytes_sent: close.len(),
                    bytes_received: 0,
                }),
            },
        ]
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies best-effort selected shutdown reports duplicates without replaying.
///
/// Inputs:
/// - One open WebSocket session repeated twice in a best-effort close set.
///
/// Output:
/// - Test passes when the session is closed once and the duplicate entry
///   reports a stable diagnostic without sending a second close frame.
///
/// Transformation:
/// - Keeps best-effort cleanup deterministic for accidental duplicate actor
///   references.
#[test]
fn vm_websocket_runtime_best_effort_close_sessions_reports_duplicates() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let outcomes = runtime.close_sessions_and_streams_best_effort(&mut tcp, &[session, session]);
    let close = tcp
        .receive(client, 4096)
        .expect("receive close")
        .expect("queued close frame");

    assert_eq!(
        decode_server_control_frame(&close),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(tcp.receive(client, 4096).expect("client receive"), None);
    assert_eq!(
        outcomes,
        vec![
            VmWebSocketCloseOutcome {
                session,
                result: Ok(VmWebSocketSessionInfo {
                    stream: server,
                    open: false,
                    frames_sent: 1,
                    frames_received: 0,
                    bytes_sent: close.len(),
                    bytes_received: 0,
                }),
            },
            VmWebSocketCloseOutcome {
                session,
                result: Err(
                    "error[vm_websocket_session]: duplicate session handle in close set"
                        .to_string(),
                ),
            },
        ]
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies all-session shutdown is stable for an empty runtime.
///
/// Inputs:
/// - Empty WebSocket runtime.
///
/// Output:
/// - Test passes when shutdown returns an empty result.
///
/// Transformation:
/// - Keeps listener shutdown idempotent when no WebSocket sessions were
///   accepted.
#[test]
fn vm_websocket_runtime_close_all_sessions_is_empty_for_empty_runtime() {
    let mut tcp = VmTcpRuntime::new();
    let mut runtime = VmWebSocketRuntime::new();

    let closed = runtime
        .close_all_sessions_and_streams(&mut tcp)
        .expect("close all empty");

    assert_eq!(
        closed,
        Vec::<(VmWebSocketSessionId, VmWebSocketSessionInfo)>::new()
    );
    assert_eq!(runtime.session_count(), 0);
}

/// Verifies all-session shutdown closes sessions in deterministic order.
///
/// Inputs:
/// - Two open sessions and one peer-closed session.
///
/// Output:
/// - Test passes when all final states are returned in handle order and all
///   VM TCP streams are closed.
///
/// Transformation:
/// - Gives production listener shutdown a deterministic VM-owned WebSocket
///   cleanup path without exposing registry iteration order.
#[test]
fn vm_websocket_runtime_close_all_sessions_closes_every_tracked_session() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.close_all.a");
    let (client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.close_all.b");
    let (client_c, server_c) = connected_tcp_pair_at(&mut tcp, "websocket.close_all.c");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);
    let session_c = runtime.open_session(server_c);
    let client_close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    let client_close_len = client_close.len();
    tcp.send(client_b, client_close).expect("send client close");
    runtime
        .receive_frame(&mut tcp, session_b, 4096)
        .expect("receive client close")
        .expect("close event");

    let final_states = runtime
        .close_all_sessions_and_streams(&mut tcp)
        .expect("close all sessions");
    let close_a = tcp
        .receive(client_a, 4096)
        .expect("receive close a")
        .expect("queued close a");
    let close_c = tcp
        .receive(client_c, 4096)
        .expect("receive close c")
        .expect("queued close c");

    assert_eq!(
        decode_server_control_frame(&close_a),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        decode_server_control_frame(&close_c),
        VmWebSocketControlFrame::Close
    );
    assert_eq!(
        final_states,
        vec![
            (
                session_a,
                VmWebSocketSessionInfo {
                    stream: server_a,
                    open: false,
                    frames_sent: 1,
                    frames_received: 0,
                    bytes_sent: close_a.len(),
                    bytes_received: 0,
                },
            ),
            (
                session_b,
                VmWebSocketSessionInfo {
                    stream: server_b,
                    open: false,
                    frames_sent: 0,
                    frames_received: 1,
                    bytes_sent: 0,
                    bytes_received: client_close_len,
                },
            ),
            (
                session_c,
                VmWebSocketSessionInfo {
                    stream: server_c,
                    open: false,
                    frames_sent: 1,
                    frames_received: 0,
                    bytes_sent: close_c.len(),
                    bytes_received: 0,
                },
            ),
        ]
    );
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server_a).expect("inspect a").closed);
    assert!(tcp.inspect_stream(server_b).expect("inspect b").closed);
    assert!(tcp.inspect_stream(server_c).expect("inspect c").closed);
    assert_eq!(tcp.receive(client_b, 4096).expect("client b receive"), None);
}

/// Verifies WebSocket runtime removal and unknown-handle diagnostics.
///
/// Inputs:
/// - One registered WebSocket session and one fabricated missing handle.
///
/// Output:
/// - Test passes when removal returns final state and later lookup fails with
///   a stable diagnostic.
///
/// Transformation:
/// - Gives production session cleanup deterministic registry behavior.
#[test]
fn vm_websocket_runtime_removes_sessions_and_rejects_unknown_handles() {
    let (_tcp, _client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);
    let unknown = VmWebSocketSessionId { id: 99_999 };

    assert_eq!(
        runtime
            .inspect_session(unknown)
            .expect_err("unknown inspect"),
        "VM WebSocket session handle is unknown"
    );

    let removed = runtime.remove_session(session).expect("remove session");

    assert_eq!(
        removed,
        VmWebSocketSessionInfo {
            stream: server,
            open: true,
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    );
    assert_eq!(runtime.session_count(), 0);
    assert_eq!(
        runtime
            .inspect_session(session)
            .expect_err("removed inspect"),
        "VM WebSocket session handle is unknown"
    );
}

/// Memory stream used by tests to make tungstenite produce client frames.
struct TestWebSocketMemoryStream {
    read: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

impl TestWebSocketMemoryStream {
    /// Creates a memory stream with preloaded inbound bytes.
    fn new(read_bytes: Vec<u8>) -> Self {
        Self {
            read: Cursor::new(read_bytes),
            written: Vec::new(),
        }
    }
}

impl Read for TestWebSocketMemoryStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.read.read(buffer)
    }
}

impl Write for TestWebSocketMemoryStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Encodes a browser/client text frame with tungstenite.
fn encode_client_text_frame(text: &str) -> Vec<u8> {
    let stream = TestWebSocketMemoryStream::new(Vec::new());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, None);
    socket.send(Message::text(text)).expect("client send");
    socket.into_inner().written
}

/// Encodes a browser/client binary frame with tungstenite.
fn encode_client_binary_frame(bytes: &[u8]) -> Vec<u8> {
    let stream = TestWebSocketMemoryStream::new(Vec::new());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, None);
    socket
        .send(Message::binary(bytes.to_vec()))
        .expect("client binary send");
    socket.into_inner().written
}

/// Encodes a browser/client control frame with tungstenite.
fn encode_client_control_frame(frame: VmWebSocketControlFrame) -> Vec<u8> {
    encode_test_control_frame(Role::Client, frame)
}

/// Decodes a server frame with tungstenite acting as the client peer.
fn decode_server_text_frame(frame: &[u8]) -> String {
    let stream = TestWebSocketMemoryStream::new(frame.to_vec());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, None);
    match socket.read().expect("client read") {
        Message::Text(text) => text.to_string(),
        other => panic!("expected text frame, got {other:?}"),
    }
}

/// Decodes a server control frame with tungstenite acting as the client peer.
fn decode_server_control_frame(frame: &[u8]) -> VmWebSocketControlFrame {
    let stream = TestWebSocketMemoryStream::new(frame.to_vec());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, None);
    match socket.read().expect("client read") {
        Message::Ping(payload) => VmWebSocketControlFrame::Ping(payload.to_vec()),
        Message::Pong(payload) => VmWebSocketControlFrame::Pong(payload.to_vec()),
        Message::Close(_) => VmWebSocketControlFrame::Close,
        other => panic!("expected control frame, got {other:?}"),
    }
}

/// Encodes a test control frame for a tungstenite endpoint role.
fn encode_test_control_frame(role: Role, frame: VmWebSocketControlFrame) -> Vec<u8> {
    let stream = TestWebSocketMemoryStream::new(Vec::new());
    let mut socket = WebSocket::from_raw_socket(stream, role, None);
    let message = match frame {
        VmWebSocketControlFrame::Ping(payload) => Message::Ping(payload.into()),
        VmWebSocketControlFrame::Pong(payload) => Message::Pong(payload.into()),
        VmWebSocketControlFrame::Close => Message::Close(None),
    };
    socket.send(message).expect("control send");
    socket.into_inner().written
}

/// Creates a self-signed certificate pair for VM WebSocket TLS tests.
fn write_websocket_tls_cert_pair(name: &str) -> (std::path::PathBuf, String, String, Vec<u8>) {
    let dir = test_fs::temp_path("vm_websocket_tls", name);
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    let cert_der = generated.cert.der().as_ref().to_vec();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&cert_path, generated.cert.pem()).expect("write cert");
    fs::write(&key_path, generated.key_pair.serialize_pem()).expect("write key");
    (
        dir,
        cert_path.to_string_lossy().to_string(),
        key_path.to_string_lossy().to_string(),
        cert_der,
    )
}

/// Builds a manual TLS plan for WebSocket TLS tests.
fn websocket_tls_manual_plan(cert_path: String, key_path: String) -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Manual,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: Some(cert_path),
        key_path: Some(key_path),
        passphrase_env: None,
        ca_path: None,
        server_name: None,
        trust_local: None,
    }
}

/// Creates a rustls client that trusts the generated WebSocket TLS cert.
fn websocket_tls_client_for_cert(cert_der: Vec<u8>) -> ClientConnection {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(cert_der))
        .expect("root cert should install");
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
    ClientConnection::new(
        Arc::new(config),
        ServerName::try_from("localhost").expect("server name"),
    )
    .expect("client connection")
}

/// Flushes pending rustls client bytes into VM TCP.
fn flush_websocket_tls_client_to_tcp(
    client: &mut ClientConnection,
    tcp: &mut VmTcpRuntime,
    client_stream: crate::runtime::vm::tcp::VmTcpStream,
) {
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).expect("client writes TLS");
    if !bytes.is_empty() {
        tcp.send(client_stream, bytes)
            .expect("client sends TLS over VM TCP");
    }
}

/// Pumps pending VM TCP TLS bytes into the rustls client.
fn pump_websocket_tls_tcp_to_client(
    tcp: &mut VmTcpRuntime,
    client_stream: crate::runtime::vm::tcp::VmTcpStream,
    client: &mut ClientConnection,
) {
    while let Some(bytes) = tcp
        .receive(client_stream, 16 * 1024)
        .expect("client receives TLS")
    {
        let consumed = client
            .read_tls(&mut Cursor::new(bytes.as_slice()))
            .expect("client reads server TLS bytes");
        assert_eq!(consumed, bytes.len());
        client
            .process_new_packets()
            .expect("client processes TLS packets");
    }
}

/// Completes a VM TCP-backed TLS handshake for WebSocket tests.
fn complete_websocket_tls_tcp_handshake(
    client: &mut ClientConnection,
    tcp: &mut VmTcpRuntime,
    client_stream: crate::runtime::vm::tcp::VmTcpStream,
    server: &mut VmTlsTcpServerStream,
) {
    flush_websocket_tls_client_to_tcp(client, tcp, client_stream);
    for _ in 0..10 {
        let _ = server.poll(tcp).expect("server polls TLS over VM TCP");
        pump_websocket_tls_tcp_to_client(tcp, client_stream, client);
        flush_websocket_tls_client_to_tcp(client, tcp, client_stream);
        if !client.is_handshaking() && !server.inspect().handshaking {
            return;
        }
    }
    panic!("WebSocket TLS VM TCP handshake did not complete");
}

/// Reads decrypted plaintext currently available to the rustls client.
fn read_websocket_tls_client_plaintext(
    tcp: &mut VmTcpRuntime,
    client_stream: crate::runtime::vm::tcp::VmTcpStream,
    client: &mut ClientConnection,
) -> String {
    pump_websocket_tls_tcp_to_client(tcp, client_stream, client);
    let mut response = [0; 4096];
    let read = client
        .reader()
        .read(&mut response)
        .expect("client reads decrypted response");
    std::str::from_utf8(&response[..read])
        .expect("response UTF-8")
        .to_string()
}

/// Creates a connected VM TCP client/server stream pair.
fn connected_tcp_pair() -> (
    VmTcpRuntime,
    crate::runtime::vm::tcp::VmTcpStream,
    crate::runtime::vm::tcp::VmTcpStream,
) {
    let mut tcp = VmTcpRuntime::new();
    let (client, server) = connected_tcp_pair_at(&mut tcp, "websocket.test");
    (tcp, client, server)
}

/// Creates a connected VM TCP stream pair inside an existing runtime.
fn connected_tcp_pair_at(
    tcp: &mut VmTcpRuntime,
    address: &str,
) -> (
    crate::runtime::vm::tcp::VmTcpStream,
    crate::runtime::vm::tcp::VmTcpStream,
) {
    let listener = tcp.listen(address).expect("listen");
    let client = tcp.connect(address, "client").expect("connect client");
    let server = tcp
        .accept(listener, "server")
        .expect("accept server")
        .expect("queued server stream");
    (client, server)
}
