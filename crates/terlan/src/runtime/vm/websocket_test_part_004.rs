
/// Verifies all-open broadcast is stable for an empty runtime.
///
/// Inputs:
/// - Empty WebSocket runtime.
///
/// Output:
/// - Test passes when open-session listing and broadcast both return empty
///   deterministic results.
///
/// Transformation:
/// - Keeps scheduler and actor broadcast ticks idempotent before any clients
///   are attached.
#[test]
fn vm_websocket_runtime_broadcast_to_all_open_sessions_is_empty_for_empty_runtime() {
    let mut tcp = VmTcpRuntime::new();
    let mut runtime = VmWebSocketRuntime::new();

    assert_eq!(runtime.open_sessions(), Vec::<VmWebSocketSessionId>::new());
    assert_eq!(
        runtime
            .send_frame_to_all_open_sessions(
                &mut tcp,
                VmWebSocketFrame::Text("nobody".to_string()),
            )
            .expect("empty broadcast"),
        Vec::<(VmWebSocketSessionId, usize)>::new()
    );
}

/// Verifies all-open broadcast skips closed sessions deterministically.
///
/// Inputs:
/// - Three WebSocket sessions where the middle session has received close.
///
/// Output:
/// - Test passes when broadcast sends only to open sessions in handle order
///   and leaves the closed peer without a queued server frame.
///
/// Transformation:
/// - Gives room/session actors a registry-owned broadcast primitive without
///   copying open-session scans into application code.
#[test]
fn vm_websocket_runtime_broadcast_to_all_open_sessions_skips_closed_sessions() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.broadcast.a");
    let (client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.broadcast.b");
    let (client_c, server_c) = connected_tcp_pair_at(&mut tcp, "websocket.broadcast.c");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);
    let session_c = runtime.open_session(server_c);
    tcp.send(
        client_b,
        encode_client_control_frame(VmWebSocketControlFrame::Close),
    )
    .expect("send close");
    runtime
        .receive_frame(&mut tcp, session_b, 4096)
        .expect("receive close")
        .expect("closed event");

    let sent = runtime
        .send_frame_to_all_open_sessions(&mut tcp, VmWebSocketFrame::Text("broadcast".to_string()))
        .expect("broadcast");
    let frame_a = tcp
        .receive(client_a, 4096)
        .expect("receive broadcast a")
        .expect("queued broadcast a");
    let frame_c = tcp
        .receive(client_c, 4096)
        .expect("receive broadcast c")
        .expect("queued broadcast c");

    assert_eq!(runtime.open_sessions(), vec![session_a, session_c]);
    assert_eq!(
        sent,
        vec![(session_a, frame_a.len()), (session_c, frame_c.len())]
    );
    assert_eq!(decode_server_text_frame(&frame_a), "broadcast");
    assert_eq!(decode_server_text_frame(&frame_c), "broadcast");
    assert_eq!(
        tcp.receive(client_b, 4096).expect("closed client receive"),
        None
    );
    assert_eq!(
        runtime.inspect_session(session_b).expect("inspect closed"),
        VmWebSocketSessionInfo {
            stream: server_b,
            open: false,
            frames_sent: 0,
            frames_received: 1,
            bytes_sent: 0,
            bytes_received: encode_client_control_frame(VmWebSocketControlFrame::Close).len(),
        }
    );
}

/// Verifies all-open receive is stable for an empty runtime.
///
/// Inputs:
/// - Empty WebSocket runtime.
///
/// Output:
/// - Test passes when plain and auto-pong all-open receive report no frame.
///
/// Transformation:
/// - Keeps actor polling loops idempotent before sessions are attached.
#[test]
fn vm_websocket_runtime_receive_from_all_open_sessions_is_empty_for_empty_runtime() {
    let mut tcp = VmTcpRuntime::new();
    let mut runtime = VmWebSocketRuntime::new();

    assert_eq!(
        runtime
            .receive_frame_from_all_open_sessions(&mut tcp, 4096)
            .expect("empty receive"),
        None
    );
    assert_eq!(
        runtime
            .receive_frame_from_all_open_sessions_with_auto_pong(&mut tcp, 4096)
            .expect("empty auto-pong receive"),
        None
    );
}

/// Verifies all-open receive skips closed sessions in handle order.
///
/// Inputs:
/// - Three sessions where the middle session is closed and the two open
///   sessions have queued text frames.
///
/// Output:
/// - Test passes when receive returns the lower open handle first, then the
///   remaining open handle, and never consumes from the closed session.
///
/// Transformation:
/// - Gives room/session actors a deterministic inbound polling primitive over
///   the VM-owned WebSocket registry.
#[test]
fn vm_websocket_runtime_receive_from_all_open_sessions_skips_closed_sessions() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.receive.all.a");
    let (client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.receive.all.b");
    let (client_c, server_c) = connected_tcp_pair_at(&mut tcp, "websocket.receive.all.c");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);
    let session_c = runtime.open_session(server_c);
    tcp.send(
        client_b,
        encode_client_control_frame(VmWebSocketControlFrame::Close),
    )
    .expect("send close");
    runtime
        .receive_frame(&mut tcp, session_b, 4096)
        .expect("receive close")
        .expect("close event");
    tcp.send(client_c, encode_client_text_frame("third"))
        .expect("send third");
    tcp.send(client_a, encode_client_text_frame("first"))
        .expect("send first");

    let first = runtime
        .receive_frame_from_all_open_sessions(&mut tcp, 4096)
        .expect("receive first")
        .expect("first frame");
    let second = runtime
        .receive_frame_from_all_open_sessions(&mut tcp, 4096)
        .expect("receive second")
        .expect("second frame");

    assert_eq!(
        first,
        (session_a, VmWebSocketFrame::Text("first".to_string()))
    );
    assert_eq!(
        second,
        (session_c, VmWebSocketFrame::Text("third".to_string()))
    );
    assert_eq!(
        runtime
            .receive_frame_from_all_open_sessions(&mut tcp, 4096)
            .expect("receive empty"),
        None
    );
}

/// Verifies all-open receive with auto-pong replies to ping frames.
///
/// Inputs:
/// - One open session with a queued client ping frame.
///
/// Output:
/// - Test passes when all-open auto-pong returns the ping event and emits a
///   matching pong frame over the same VM TCP stream.
///
/// Transformation:
/// - Keeps ping boilerplate inside the VM for actors that poll the whole
///   currently-open session set.
#[test]
fn vm_websocket_runtime_receive_from_all_open_sessions_with_auto_pong_replies_to_ping() {
    let mut tcp = VmTcpRuntime::new();
    let (client, server) = connected_tcp_pair_at(&mut tcp, "websocket.receive.all.ping");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);
    let ping = encode_client_control_frame(VmWebSocketControlFrame::Ping(b"all".to_vec()));
    tcp.send(client, ping.clone()).expect("send ping");

    let received = runtime
        .receive_frame_from_all_open_sessions_with_auto_pong(&mut tcp, 4096)
        .expect("receive ping")
        .expect("ping frame");
    let pong = tcp
        .receive(client, 4096)
        .expect("receive pong")
        .expect("queued pong");

    assert_eq!(
        received,
        (
            session,
            VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(b"all".to_vec()))
        )
    );
    assert_eq!(
        decode_server_control_frame(&pong),
        VmWebSocketControlFrame::Pong(b"all".to_vec())
    );
    assert_eq!(
        runtime.inspect_session(session).expect("inspect session"),
        VmWebSocketSessionInfo {
            stream: server,
            open: true,
            frames_sent: 1,
            frames_received: 1,
            bytes_sent: pong.len(),
            bytes_received: ping.len(),
        }
    );
}

/// Verifies selected-session receive polls sessions in caller order.
///
/// Inputs:
/// - Two registered sessions, both with queued client text frames.
/// - A receive set ordered opposite to session creation order.
///
/// Output:
/// - Test passes when the first call returns the first caller-listed ready
///   session and the second call returns the remaining queued session.
///
/// Transformation:
/// - Gives room/session actors deterministic group receive without exposing
///   registry map iteration order or owning VM TCP streams directly.
#[test]
fn vm_websocket_runtime_receives_first_frame_from_selected_sessions() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.receive.a");
    let (client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.receive.b");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);
    tcp.send(client_a, encode_client_text_frame("alpha"))
        .expect("send alpha");
    tcp.send(client_b, encode_client_text_frame("beta"))
        .expect("send beta");

    let first = runtime
        .receive_frame_from_sessions(&mut tcp, &[session_b, session_a], 4096)
        .expect("receive first")
        .expect("first frame");
    let second = runtime
        .receive_frame_from_sessions(&mut tcp, &[session_b, session_a], 4096)
        .expect("receive second")
        .expect("second frame");

    assert_eq!(
        first,
        (session_b, VmWebSocketFrame::Text("beta".to_string()))
    );
    assert_eq!(
        second,
        (session_a, VmWebSocketFrame::Text("alpha".to_string()))
    );
    assert_eq!(
        runtime.inspect_session(session_a).expect("inspect a"),
        VmWebSocketSessionInfo {
            stream: server_a,
            open: true,
            frames_sent: 0,
            frames_received: 1,
            bytes_sent: 0,
            bytes_received: encode_client_text_frame("alpha").len(),
        }
    );
    assert_eq!(
        runtime.inspect_session(session_b).expect("inspect b"),
        VmWebSocketSessionInfo {
            stream: server_b,
            open: true,
            frames_sent: 0,
            frames_received: 1,
            bytes_sent: 0,
            bytes_received: encode_client_text_frame("beta").len(),
        }
    );
}

/// Verifies selected-session receive validates before consuming frames.
///
/// Inputs:
/// - One open session with queued data.
/// - One closed session and one duplicate receive set.
///
/// Output:
/// - Test passes when invalid receive sets return stable diagnostics and the
///   open session's queued frame remains readable afterward.
///
/// Transformation:
/// - Keeps actor-level group receive atomic for registry and lifecycle
///   validation errors.
#[test]
fn vm_websocket_runtime_group_receive_rejects_invalid_sets_without_consuming() {
    let mut tcp = VmTcpRuntime::new();
    let (open_client, open_server) = connected_tcp_pair_at(&mut tcp, "websocket.receive.open");
    let (closed_client, closed_server) =
        connected_tcp_pair_at(&mut tcp, "websocket.receive.closed");
    let mut runtime = VmWebSocketRuntime::new();
    let open = runtime.open_session(open_server);
    let closed = runtime.open_session(closed_server);
    tcp.send(open_client, encode_client_text_frame("still queued"))
        .expect("send queued");
    tcp.send(
        closed_client,
        encode_client_control_frame(VmWebSocketControlFrame::Close),
    )
    .expect("send close");
    runtime
        .receive_frame(&mut tcp, closed, 4096)
        .expect("receive close")
        .expect("closed event");

    let duplicate = runtime
        .receive_frame_from_sessions(&mut tcp, &[open, open], 4096)
        .expect_err("duplicate rejected");
    assert_eq!(
        duplicate,
        "error[vm_websocket_session]: duplicate session handle in receive set"
    );

    let closed_err = runtime
        .receive_frame_from_sessions(&mut tcp, &[open, closed], 4096)
        .expect_err("closed rejected");
    assert_eq!(closed_err, "error[vm_websocket_session]: session is closed");
    assert_eq!(
        runtime
            .receive_frame(&mut tcp, open, 4096)
            .expect("receive open"),
        Some(VmWebSocketFrame::Text("still queued".to_string()))
    );
}

/// Verifies selected-session receive can auto-pong ready ping frames.
///
/// Inputs:
/// - Two registered sessions: one with queued text and one with queued ping.
/// - A receive set that checks the ping session first.
///
/// Output:
/// - Test passes when the first receive returns the ping session and queues a
///   pong, then the second receive returns the remaining text session.
///
/// Transformation:
/// - Lets room/session actors poll a group while the VM handles WebSocket
///   ping replies consistently with single-session receive.
#[test]
fn vm_websocket_runtime_group_receive_with_auto_pong_replies_to_selected_ping() {
    let mut tcp = VmTcpRuntime::new();
    let (text_client, text_server) = connected_tcp_pair_at(&mut tcp, "websocket.group.text");
    let (ping_client, ping_server) = connected_tcp_pair_at(&mut tcp, "websocket.group.ping");
    let mut runtime = VmWebSocketRuntime::new();
    let text_session = runtime.open_session(text_server);
    let ping_session = runtime.open_session(ping_server);
    let ping = encode_client_control_frame(VmWebSocketControlFrame::Ping(b"group".to_vec()));
    tcp.send(text_client, encode_client_text_frame("group text"))
        .expect("send group text");
    tcp.send(ping_client, ping.clone())
        .expect("send group ping");

    let first = runtime
        .receive_frame_from_sessions_with_auto_pong(&mut tcp, &[ping_session, text_session], 4096)
        .expect("receive ping")
        .expect("ping frame");
    let pong = tcp
        .receive(ping_client, 4096)
        .expect("receive pong")
        .expect("queued pong");
    let second = runtime
        .receive_frame_from_sessions_with_auto_pong(&mut tcp, &[ping_session, text_session], 4096)
        .expect("receive text")
        .expect("text frame");

    assert_eq!(
        first,
        (
            ping_session,
            VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(b"group".to_vec()))
        )
    );
    assert_eq!(
        decode_server_control_frame(&pong),
        VmWebSocketControlFrame::Pong(b"group".to_vec())
    );
    assert_eq!(
        second,
        (
            text_session,
            VmWebSocketFrame::Text("group text".to_string())
        )
    );
    assert_eq!(
        runtime.inspect_session(ping_session).expect("inspect ping"),
        VmWebSocketSessionInfo {
            stream: ping_server,
            open: true,
            frames_sent: 1,
            frames_received: 1,
            bytes_sent: pong.len(),
            bytes_received: ping.len(),
        }
    );
}

/// Verifies empty WebSocket runtime inspection is stable.
///
/// Inputs:
/// - Empty VM WebSocket runtime.
///
/// Output:
/// - Test passes when all aggregate counters are zero.
///
/// Transformation:
/// - Gives debugger/status code a deterministic baseline snapshot.
#[test]
fn vm_websocket_runtime_inspect_reports_empty_state() {
    let runtime = VmWebSocketRuntime::new();

    assert_eq!(
        runtime.inspect(),
        VmWebSocketRuntimeInfo {
            session_count: 0,
            open_sessions: 0,
            closed_sessions: 0,
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    );
}

/// Verifies WebSocket runtime inspection aggregates session traffic.
///
/// Inputs:
/// - Two registered sessions.
/// - One server-sent text frame and one client close frame.
///
/// Output:
/// - Test passes when aggregate open/closed counts and traffic counters match
///   the underlying sessions.
///
/// Transformation:
/// - Exposes runtime-level observability without exposing registry internals.
#[test]
fn vm_websocket_runtime_inspect_aggregates_session_state() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.inspect.a");
    let (client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.inspect.b");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);
    let sent = runtime
        .send_frame(
            &mut tcp,
            session_a,
            VmWebSocketFrame::Text("inspect".to_string()),
        )
        .expect("send inspect frame");
    let close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    let close_len = close.len();
    tcp.send(client_b, close).expect("send close");
    runtime
        .receive_frame(&mut tcp, session_b, 4096)
        .expect("receive close")
        .expect("close event");

    assert!(tcp.receive(client_a, 4096).expect("receive sent").is_some());
    assert_eq!(
        runtime.inspect(),
        VmWebSocketRuntimeInfo {
            session_count: 2,
            open_sessions: 1,
            closed_sessions: 1,
            frames_sent: 1,
            frames_received: 1,
            bytes_sent: sent,
            bytes_received: close_len,
        }
    );
}

/// Verifies per-session inspection is empty for a fresh runtime.
///
/// Inputs:
/// - Empty VM WebSocket runtime.
///
/// Output:
/// - Test passes when no session snapshots are returned.
///
/// Transformation:
/// - Gives debugger/status code deterministic empty-list behavior.
#[test]
fn vm_websocket_runtime_inspect_sessions_reports_empty_state() {
    let runtime = VmWebSocketRuntime::new();

    assert_eq!(
        runtime.inspect_sessions(),
        Vec::<(VmWebSocketSessionId, VmWebSocketSessionInfo)>::new()
    );
}

/// Verifies per-session inspection returns deterministic sorted snapshots.
///
/// Inputs:
/// - Two registered sessions with different traffic state.
///
/// Output:
/// - Test passes when snapshots are sorted by session handle and contain
///   session-local counters.
///
/// Transformation:
/// - Exposes debugger/status session details without exposing registry
///   internals or hash-map iteration order.
#[test]
fn vm_websocket_runtime_inspect_sessions_returns_sorted_snapshots() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.list.a");
    let (_client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.list.b");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);
    let incoming = encode_client_text_frame("listed");
    let incoming_len = incoming.len();
    tcp.send(client_a, incoming).expect("send listed frame");
    runtime
        .receive_frame(&mut tcp, session_a, 4096)
        .expect("receive listed")
        .expect("listed event");

    assert_eq!(
        runtime.inspect_sessions(),
        vec![
            (
                session_a,
                VmWebSocketSessionInfo {
                    stream: server_a,
                    open: true,
                    frames_sent: 0,
                    frames_received: 1,
                    bytes_sent: 0,
                    bytes_received: incoming_len,
                },
            ),
            (
                session_b,
                VmWebSocketSessionInfo {
                    stream: server_b,
                    open: true,
                    frames_sent: 0,
                    frames_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                },
            ),
        ]
    );
}

/// Verifies runtime-level auto-pong works through session handles.
///
/// Inputs:
/// - One registered WebSocket session.
/// - Client ping frame generated by tungstenite.
///
/// Output:
/// - Test passes when runtime receive returns the ping event and queues a pong
///   to the client stream.
///
/// Transformation:
/// - Keeps production handler code on handle-based VM WebSocket APIs while the
///   VM handles ping responses.
#[test]
fn vm_websocket_runtime_receive_frame_with_auto_pong_replies_by_handle() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);
    let ping = encode_client_control_frame(VmWebSocketControlFrame::Ping(b"runtime".to_vec()));
    tcp.send(client, ping).expect("send runtime ping");

    let event = runtime
        .receive_frame_with_auto_pong(&mut tcp, session, 4096)
        .expect("receive runtime ping")
        .expect("runtime ping event");
    let pong = tcp
        .receive(client, 4096)
        .expect("receive runtime pong")
        .expect("queued runtime pong");

    assert_eq!(
        event,
        VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(b"runtime".to_vec()))
    );
    assert_eq!(
        decode_server_control_frame(&pong),
        VmWebSocketControlFrame::Pong(b"runtime".to_vec())
    );
    assert_eq!(
        runtime.inspect_session(session).expect("inspect session"),
        VmWebSocketSessionInfo {
            stream: server,
            open: true,
            frames_sent: 1,
            frames_received: 1,
            bytes_sent: pong.len(),
            bytes_received: encode_client_control_frame(VmWebSocketControlFrame::Ping(
                b"runtime".to_vec()
            ))
            .len(),
        }
    );
}

/// Verifies the runtime can sweep closed WebSocket sessions.
///
/// Inputs:
/// - Two registered sessions, one closed by a client close frame and one left
///   open.
///
/// Output:
/// - Test passes when only the closed session is removed and final state is
///   returned.
///
/// Transformation:
/// - Gives production WebSocket scheduling deterministic cleanup without
///   scanning ad hoc handler-owned maps.
#[test]
fn vm_websocket_runtime_removes_only_closed_sessions() {
    let mut tcp = VmTcpRuntime::new();
    let (closed_client, closed_server) = connected_tcp_pair_at(&mut tcp, "websocket.closed");
    let (_open_client, open_server) = connected_tcp_pair_at(&mut tcp, "websocket.open");
    let mut runtime = VmWebSocketRuntime::new();
    let closed = runtime.open_session(closed_server);
    let open = runtime.open_session(open_server);
    let close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    let close_len = close.len();
    tcp.send(closed_client, close).expect("send close");
    runtime
        .receive_frame(&mut tcp, closed, 4096)
        .expect("receive close")
        .expect("close event");

    let removed = runtime.remove_closed_sessions();

    assert_eq!(
        removed,
        vec![VmWebSocketSessionInfo {
            stream: closed_server,
            open: false,
            frames_sent: 0,
            frames_received: 1,
            bytes_sent: 0,
            bytes_received: close_len,
        }]
    );
    assert_eq!(runtime.session_count(), 1);
    assert_eq!(
        runtime.inspect_session(open).expect("inspect open"),
        VmWebSocketSessionInfo {
            stream: open_server,
            open: true,
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    );
    assert_eq!(
        runtime.inspect_session(closed).expect_err("closed removed"),
        "VM WebSocket session handle is unknown"
    );
}

/// Verifies sweeping closed sessions is stable when nothing is closed.
///
/// Inputs:
/// - One open WebSocket session.
///
/// Output:
/// - Test passes when no sessions are removed.
///
/// Transformation:
/// - Keeps cleanup idempotent for scheduler ticks with no closed sessions.
#[test]
fn vm_websocket_runtime_remove_closed_sessions_is_empty_when_all_open() {
    let (_tcp, _client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    runtime.open_session(server);

    let removed = runtime.remove_closed_sessions();

    assert_eq!(removed, Vec::<VmWebSocketSessionInfo>::new());
    assert_eq!(runtime.session_count(), 1);
}

/// Verifies closed-session cleanup can close VM TCP streams.
///
/// Inputs:
/// - One registered session closed by a client close frame.
///
/// Output:
/// - Test passes when cleanup removes the session and marks the underlying VM
///   TCP stream closed.
///
/// Transformation:
/// - Ties WebSocket lifecycle cleanup back to VM-owned TCP resource cleanup.
#[test]
fn vm_websocket_runtime_remove_closed_sessions_can_close_tcp_streams() {
    let mut tcp = VmTcpRuntime::new();
    let (client, server) = connected_tcp_pair_at(&mut tcp, "websocket.cleanup");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);
    let close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    tcp.send(client, close).expect("send close");
    runtime
        .receive_frame(&mut tcp, session, 4096)
        .expect("receive close")
        .expect("close event");

    let removed = runtime
        .remove_closed_sessions_and_close_streams(&mut tcp)
        .expect("cleanup closed sessions");

    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].stream, server);
    assert_eq!(runtime.session_count(), 0);
    assert!(tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies closed-session cleanup rejects stale raw TCP handles atomically.
///
/// Inputs:
/// - One raw WebSocket session whose stream handle does not exist in VM TCP.
/// - The raw session is marked closed to simulate an adversarial registry
///   fixture after a failed transport cleanup.
///
/// Output:
/// - Test passes when cleanup reports a WebSocket-scoped TCP close diagnostic.
///
/// Transformation:
/// - Keeps invalid raw registry state from silently disappearing during
///   cleanup; production paths use checked open, but adversarial tests pin the
///   failure surface.
#[test]
fn vm_websocket_runtime_remove_closed_sessions_reports_invalid_stream_close() {
    let mut tcp = VmTcpRuntime::new();
    let mut runtime = VmWebSocketRuntime::new();
    let stale_stream = VmTcpStream::test_handle(424_242);
    let session = runtime.open_session(stale_stream);
    runtime
        .sessions
        .get_mut(&session.id)
        .expect("raw session")
        .open = false;

    let err = runtime
        .remove_closed_sessions_and_close_streams(&mut tcp)
        .expect_err("invalid close rejected");

    assert_eq!(
        err,
        "error[vm_websocket_tcp]: failed to close session stream: VM TCP stream handle is unknown"
    );
    assert_eq!(runtime.session_count(), 0);
}

/// Verifies TCP-closing cleanup leaves open sessions and streams alone.
///
/// Inputs:
/// - One registered open WebSocket session.
///
/// Output:
/// - Test passes when no session is removed and the VM TCP stream remains
///   open.
///
/// Transformation:
/// - Keeps scheduler cleanup ticks safe for active WebSocket sessions.
#[test]
fn vm_websocket_runtime_tcp_closing_cleanup_is_empty_when_all_open() {
    let (mut tcp, _client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    runtime.open_session(server);

    let removed = runtime
        .remove_closed_sessions_and_close_streams(&mut tcp)
        .expect("cleanup open sessions");

    assert_eq!(removed, Vec::<VmWebSocketSessionInfo>::new());
    assert_eq!(runtime.session_count(), 1);
    assert!(!tcp.inspect_stream(server).expect("inspect stream").closed);
}
