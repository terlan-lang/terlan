
/// Verifies accepted WebSocket upgrade metadata serializes to HTTP/1 bytes.
///
/// Inputs:
/// - RFC example WebSocket handshake response metadata.
///
/// Output:
/// - Test passes when the VM serializes the exact switching-protocol response
///   head without adding a response body.
///
/// Transformation:
/// - Locks the upgrade wire shape before live VM TCP/TLS response emission uses
///   it.
#[test]
fn vm_websocket_upgrade_response_serializes_http1_switching_protocols() {
    let response =
        build_websocket_upgrade_response("dGhlIHNhbXBsZSBub25jZQ==").expect("upgrade response");

    let bytes = serialize_websocket_upgrade_response(&response).expect("serialized response");

    assert_eq!(
        String::from_utf8(bytes).expect("utf8 response"),
        concat!(
            "HTTP/1.1 101 Switching Protocols\r\n",
            "upgrade: websocket\r\n",
            "connection: Upgrade\r\n",
            "sec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n",
            "\r\n"
        )
    );
}

/// Verifies accepted WebSocket upgrade metadata is validated before writing.
///
/// Inputs:
/// - Invalid response status.
/// - Invalid header value containing newline characters.
///
/// Output:
/// - Test passes when malformed metadata reports stable diagnostics instead of
///   writing ambiguous HTTP bytes.
///
/// Transformation:
/// - Keeps the VM upgrade sender strict even when a future caller constructs
///   response metadata outside the normal handshake helper.
#[test]
fn vm_websocket_upgrade_response_serialization_rejects_invalid_metadata() {
    assert_eq!(
        serialize_websocket_upgrade_response(&VmWebSocketUpgradeResponse {
            status: 200,
            headers: Vec::new(),
        })
        .expect_err("non-upgrade status rejected"),
        "error[vm_websocket_upgrade]: response status must be 101"
    );
    let err = serialize_websocket_upgrade_response(&VmWebSocketUpgradeResponse {
        status: 101,
        headers: vec![("x-bad".to_string(), "bad\r\nvalue".to_string())],
    })
    .expect_err("invalid header value rejected");

    assert!(
        err.starts_with("error[vm_websocket_upgrade]: invalid header `x-bad` value:"),
        "{err}"
    );
}

/// Verifies accepted WebSocket upgrades can be emitted over VM TCP.
///
/// Inputs:
/// - One accepted WebSocket upgrade bound to the server side of a VM TCP pair.
///
/// Output:
/// - Test passes when the client peer receives the exact HTTP/1 upgrade
///   response bytes.
///
/// Transformation:
/// - Proves the VM upgrade handoff can now cross the TCP byte boundary, not
///   just produce metadata for higher layers.
#[test]
fn vm_websocket_runtime_send_upgrade_response_writes_to_peer() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let endpoint = VmWebSocketEndpointPlan::new(8, 2048).expect("endpoint plan");
    let mut runtime = VmWebSocketRuntime::new();
    let accepted = runtime
        .accept_upgrade(&tcp, server, &endpoint, "dGhlIHNhbXBsZSBub25jZQ==")
        .expect("accepted upgrade");

    let written = runtime
        .send_upgrade_response(&mut tcp, &accepted)
        .expect("send upgrade response");
    let received = tcp
        .receive(client, written)
        .expect("client receive")
        .expect("upgrade response bytes");

    assert_eq!(written, received.len());
    assert_eq!(
        String::from_utf8(received).expect("utf8 response"),
        concat!(
            "HTTP/1.1 101 Switching Protocols\r\n",
            "upgrade: websocket\r\n",
            "connection: Upgrade\r\n",
            "sec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n",
            "\r\n"
        )
    );
}

/// Verifies upgrade response emission reports dead VM TCP streams.
///
/// Inputs:
/// - One accepted WebSocket upgrade whose bound TCP stream is closed before
///   the response is written.
///
/// Output:
/// - Test passes when sending reports a stable VM WebSocket/TCP diagnostic.
///
/// Transformation:
/// - Keeps live upgrade emission from silently succeeding after transport
///   cleanup or cancellation races.
#[test]
fn vm_websocket_runtime_send_upgrade_response_rejects_closed_stream() {
    let (mut tcp, _client, server) = connected_tcp_pair();
    let endpoint = VmWebSocketEndpointPlan::new(8, 2048).expect("endpoint plan");
    let mut runtime = VmWebSocketRuntime::new();
    let accepted = runtime
        .accept_upgrade(&tcp, server, &endpoint, "dGhlIHNhbXBsZSBub25jZQ==")
        .expect("accepted upgrade");
    tcp.close_stream(server).expect("close server stream");

    assert_eq!(
        runtime
            .send_upgrade_response(&mut tcp, &accepted)
            .expect_err("closed stream rejected"),
        "error[vm_websocket_tcp]: failed to send upgrade response: VM TCP stream is closed"
    );
}

/// Verifies accepted WebSocket upgrades can be emitted over VM TLS streams.
///
/// Inputs:
/// - One VM TCP client/server stream pair wrapped in rustls TLS.
/// - One accepted WebSocket upgrade bound to the TLS server stream.
///
/// Output:
/// - Test passes when the client decrypts the exact HTTP/1 upgrade response
///   bytes.
///
/// Transformation:
/// - Proves WebSocket upgrade emission reuses VM TLS plaintext writes instead
///   of introducing a host-socket path.
#[test]
fn vm_websocket_runtime_send_tls_upgrade_response_writes_to_peer() {
    let (dir, cert_path, key_path, cert_der) =
        write_websocket_tls_cert_pair("websocket_tls_upgrade");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("websocket-tls.local").expect("listener");
    let client = tcp
        .connect("websocket-tls.local", "client")
        .expect("connect client");
    let server = tcp
        .accept(listener, "server")
        .expect("accept server")
        .expect("queued server stream");
    let mut tls_runtime = VmTlsRuntime::new();
    tls_runtime
        .install_listener_plan(listener, websocket_tls_manual_plan(cert_path, key_path))
        .expect("install TLS plan");
    let mut tls_client = websocket_tls_client_for_cert(cert_der);
    let mut tls_stream = VmTlsTcpServerStream::new(
        server,
        tls_runtime
            .start_listener_server_connection(listener)
            .expect("start TLS server"),
    );
    complete_websocket_tls_tcp_handshake(&mut tls_client, &mut tcp, client, &mut tls_stream);
    let endpoint = VmWebSocketEndpointPlan::new(8, 2048).expect("endpoint plan");
    let mut runtime = VmWebSocketRuntime::new();
    let accepted = runtime
        .accept_upgrade(&tcp, server, &endpoint, "dGhlIHNhbXBsZSBub25jZQ==")
        .expect("accepted upgrade");

    let written = runtime
        .send_tls_upgrade_response(&mut tcp, &mut tls_stream, &accepted)
        .expect("send TLS upgrade response");
    let response = read_websocket_tls_client_plaintext(&mut tcp, client, &mut tls_client);

    assert_eq!(
        written,
        serialize_websocket_upgrade_response(&accepted.response)
            .expect("serialized response")
            .len()
    );
    assert_eq!(
        response,
        concat!(
            "HTTP/1.1 101 Switching Protocols\r\n",
            "upgrade: websocket\r\n",
            "connection: Upgrade\r\n",
            "sec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n",
            "\r\n"
        )
    );
    fs::remove_dir_all(dir).expect("cleanup TLS fixture");
}

/// Verifies TLS upgrade emission rejects mismatched stream/session handles.
///
/// Inputs:
/// - Two accepted VM TCP streams.
/// - TLS stream for one accepted stream and WebSocket session for the other.
///
/// Output:
/// - Test passes when the mismatch reports a stable diagnostic before writing.
///
/// Transformation:
/// - Prevents cross-session upgrade writes when HTTP/TLS routing state is wired
///   incorrectly.
#[test]
fn vm_websocket_runtime_send_tls_upgrade_response_rejects_stream_mismatch() {
    let (dir, cert_path, key_path, cert_der) =
        write_websocket_tls_cert_pair("websocket_tls_mismatch");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp
        .listen("websocket-tls-mismatch.local")
        .expect("listener");
    let client_a = tcp
        .connect("websocket-tls-mismatch.local", "client-a")
        .expect("connect client a");
    let server_a = tcp
        .accept(listener, "server-a")
        .expect("accept server a")
        .expect("queued server a stream");
    let (_client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.tls.mismatch.b");
    let mut tls_runtime = VmTlsRuntime::new();
    tls_runtime
        .install_listener_plan(listener, websocket_tls_manual_plan(cert_path, key_path))
        .expect("install TLS plan");
    let mut tls_client = websocket_tls_client_for_cert(cert_der);
    let mut tls_stream = VmTlsTcpServerStream::new(
        server_a,
        tls_runtime
            .start_listener_server_connection(listener)
            .expect("start TLS server"),
    );
    complete_websocket_tls_tcp_handshake(&mut tls_client, &mut tcp, client_a, &mut tls_stream);
    let endpoint = VmWebSocketEndpointPlan::new(8, 2048).expect("endpoint plan");
    let mut runtime = VmWebSocketRuntime::new();
    let accepted_b = runtime
        .accept_upgrade(&tcp, server_b, &endpoint, "dGhlIHNhbXBsZSBub25jZQ==")
        .expect("accepted upgrade b");

    assert_eq!(
        runtime
            .send_tls_upgrade_response(&mut tcp, &mut tls_stream, &accepted_b)
            .expect_err("mismatched TLS stream rejected"),
        "error[vm_websocket_tls]: TLS stream does not match WebSocket session"
    );
    assert_eq!(tcp.receive(client_a, 4096).expect("client receive"), None);
    fs::remove_dir_all(dir).expect("cleanup TLS fixture");
}

/// Verifies VM WebSocket sessions can be found from their VM TCP stream.
///
/// Inputs:
/// - One checked-open WebSocket session backed by a live VM TCP stream.
/// - One unrelated VM TCP stream.
///
/// Output:
/// - Test passes when the bound stream resolves to the session, unrelated
///   streams resolve to `None`, and removed sessions no longer resolve.
///
/// Transformation:
/// - Gives VM transport cleanup and diagnostics a registry-owned way to map
///   TCP resources back to WebSocket session ownership.
#[test]
fn vm_websocket_runtime_session_for_stream_returns_bound_session() {
    let mut tcp = VmTcpRuntime::new();
    let (_client, server) = connected_tcp_pair_at(&mut tcp, "websocket.lookup.bound");
    let (_other_client, other_server) = connected_tcp_pair_at(&mut tcp, "websocket.lookup.other");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime
        .open_session_checked(&tcp, server)
        .expect("checked open");

    assert_eq!(runtime.session_for_stream(server), Some(session));
    assert_eq!(runtime.session_for_stream(other_server), None);

    runtime.remove_session(session).expect("remove session");

    assert_eq!(runtime.session_for_stream(server), None);
}

/// Verifies raw duplicate stream bindings resolve deterministically.
///
/// Inputs:
/// - Two raw WebSocket sessions intentionally bound to the same VM TCP stream.
///
/// Output:
/// - Test passes when stream lookup returns the lower session handle.
///
/// Transformation:
/// - Keeps adversarial/debug behavior stable even though production handoff
///   uses checked open and rejects duplicate stream ownership.
#[test]
fn vm_websocket_runtime_session_for_stream_is_deterministic_for_duplicate_raw_bindings() {
    let (_tcp, _client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let first = runtime.open_session(server);
    let second = runtime.open_session(server);

    assert_eq!(runtime.session_for_stream(server), Some(first));
    assert_ne!(first, second);
}

/// Verifies stream-based session detach removes without writing close frames.
///
/// Inputs:
/// - One checked-open WebSocket session bound to a VM TCP stream.
///
/// Output:
/// - Test passes when detaching by stream returns the session handle and final
///   session state, removes the registry entry, sends no close frame, and
///   leaves the VM TCP stream state untouched.
///
/// Transformation:
/// - Gives VM transport close/error handlers a cleanup path for already-closed
///   or externally-owned streams without pretending to perform a graceful
///   WebSocket close.
#[test]
fn vm_websocket_runtime_remove_session_for_stream_detaches_without_sending_close() {
    let mut tcp = VmTcpRuntime::new();
    let (client, server) = connected_tcp_pair_at(&mut tcp, "websocket.detach.bound");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime
        .open_session_checked(&tcp, server)
        .expect("checked open");

    let removed = runtime
        .remove_session_for_stream(server)
        .expect("bound session");

    assert_eq!(
        removed,
        (
            session,
            VmWebSocketSessionInfo {
                stream: server,
                open: true,
                frames_sent: 0,
                frames_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
            }
        )
    );
    assert_eq!(runtime.session_for_stream(server), None);
    assert_eq!(runtime.session_count(), 0);
    assert_eq!(tcp.receive(client, 4096).expect("client receive"), None);
    assert!(!tcp.inspect_stream(server).expect("inspect stream").closed);
}

/// Verifies stream-based session detach is stable for unbound streams.
///
/// Inputs:
/// - One tracked WebSocket session and one unrelated VM TCP stream.
///
/// Output:
/// - Test passes when the unrelated stream returns `None` and leaves the
///   tracked session state unchanged.
///
/// Transformation:
/// - Keeps transport cleanup idempotent for non-WebSocket stream events.
#[test]
fn vm_websocket_runtime_remove_session_for_stream_ignores_unbound_stream() {
    let mut tcp = VmTcpRuntime::new();
    let (_client, server) = connected_tcp_pair_at(&mut tcp, "websocket.detach.bound");
    let (_other_client, other_server) = connected_tcp_pair_at(&mut tcp, "websocket.detach.other");
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime
        .open_session_checked(&tcp, server)
        .expect("checked open");

    assert_eq!(runtime.remove_session_for_stream(other_server), None);
    assert_eq!(runtime.session_for_stream(server), Some(session));
    assert_eq!(runtime.session_count(), 1);
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
}

/// Verifies inactive VM TCP streams prune their WebSocket sessions.
///
/// Inputs:
/// - Three WebSocket sessions: one live, one with a closed TCP stream, and one
///   with a cancelled TCP stream.
///
/// Output:
/// - Test passes when only the closed/cancelled stream sessions are removed in
///   handle order and the live session remains registered.
///
/// Transformation:
/// - Gives VM transport cleanup a deterministic pass for stream-level
///   disconnects that happen below the WebSocket protocol layer.
#[test]
fn vm_websocket_runtime_remove_inactive_stream_sessions_prunes_closed_and_cancelled_streams() {
    let mut tcp = VmTcpRuntime::new();
    let (_live_client, live_server) = connected_tcp_pair_at(&mut tcp, "websocket.prune.live");
    let (_closed_client, closed_server) = connected_tcp_pair_at(&mut tcp, "websocket.prune.closed");
    let (_cancelled_client, cancelled_server) =
        connected_tcp_pair_at(&mut tcp, "websocket.prune.cancelled");
    let mut runtime = VmWebSocketRuntime::new();
    let live = runtime
        .open_session_checked(&tcp, live_server)
        .expect("open live");
    let closed = runtime
        .open_session_checked(&tcp, closed_server)
        .expect("open closed");
    let cancelled = runtime
        .open_session_checked(&tcp, cancelled_server)
        .expect("open cancelled");
    tcp.close_stream(closed_server).expect("close stream");
    tcp.cancel_stream(cancelled_server).expect("cancel stream");

    let removed = runtime
        .remove_inactive_stream_sessions(&tcp)
        .expect("remove inactive streams");

    assert_eq!(
        removed,
        vec![
            (
                closed,
                VmWebSocketSessionInfo {
                    stream: closed_server,
                    open: true,
                    frames_sent: 0,
                    frames_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                }
            ),
            (
                cancelled,
                VmWebSocketSessionInfo {
                    stream: cancelled_server,
                    open: true,
                    frames_sent: 0,
                    frames_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                }
            ),
        ]
    );
    assert_eq!(runtime.session_count(), 1);
    assert_eq!(runtime.session_for_stream(live_server), Some(live));
    assert_eq!(runtime.session_for_stream(closed_server), None);
    assert_eq!(runtime.session_for_stream(cancelled_server), None);
}

/// Verifies inactive-stream pruning rejects invalid handles before removal.
///
/// Inputs:
/// - One valid session backed by a closed VM TCP stream.
/// - One raw session backed by an unknown VM TCP stream handle.
///
/// Output:
/// - Test passes when pruning reports a stable diagnostic and does not remove
///   either session.
///
/// Transformation:
/// - Keeps transport cleanup atomic when adversarial or stale raw session
///   fixtures introduce invalid stream ownership.
#[test]
fn vm_websocket_runtime_remove_inactive_stream_sessions_rejects_invalid_stream_without_partial_prune(
) {
    let mut tcp = VmTcpRuntime::new();
    let (_client, closed_server) =
        connected_tcp_pair_at(&mut tcp, "websocket.prune.invalid.closed");
    let mut runtime = VmWebSocketRuntime::new();
    let closed = runtime
        .open_session_checked(&tcp, closed_server)
        .expect("open closed");
    let invalid = runtime.open_session(VmTcpStream::test_handle(999_999));
    tcp.close_stream(closed_server).expect("close stream");

    let err = runtime
        .remove_inactive_stream_sessions(&tcp)
        .expect_err("invalid stream rejected");

    assert_eq!(
        err,
        "error[vm_websocket_tcp]: failed to inspect session stream: VM TCP stream handle is unknown"
    );
    assert_eq!(runtime.session_count(), 2);
    assert_eq!(runtime.session_for_stream(closed_server), Some(closed));
    assert_eq!(
        runtime.session_for_stream(VmTcpStream::test_handle(999_999)),
        Some(invalid)
    );
}

/// Verifies the VM WebSocket runtime owns send accounting by handle.
///
/// Inputs:
/// - One registered WebSocket session.
/// - One text frame sent through the runtime handle.
///
/// Output:
/// - Test passes when the client decodes the frame and the session counters
///   are updated through the registry.
///
/// Transformation:
/// - Keeps WebSocket send scheduling handle-based for future VM actors.
#[test]
fn vm_websocket_runtime_sends_by_session_handle() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let written = runtime
        .send_frame(
            &mut tcp,
            session,
            VmWebSocketFrame::Text("runtime send".to_string()),
        )
        .expect("runtime send");
    let frame = tcp
        .receive(client, written)
        .expect("receive runtime frame")
        .expect("queued runtime frame");

    assert_eq!(decode_server_text_frame(&frame), "runtime send");
    assert_eq!(
        runtime.inspect_session(session).expect("inspect session"),
        VmWebSocketSessionInfo {
            stream: server,
            open: true,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: written,
            bytes_received: 0,
        }
    );
}

/// Verifies runtime fan-out sends a frame to selected sessions.
///
/// Inputs:
/// - Two registered WebSocket sessions.
/// - One text frame sent to both session handles.
///
/// Output:
/// - Test passes when both clients decode the same frame and per-session byte
///   accounting is returned.
///
/// Transformation:
/// - Gives room/session actors a generic VM-owned fan-out primitive without
///   embedding room protocol concepts in the WebSocket runtime.
#[test]
fn vm_websocket_runtime_sends_frame_to_selected_sessions() {
    let mut tcp = VmTcpRuntime::new();
    let (client_a, server_a) = connected_tcp_pair_at(&mut tcp, "websocket.fanout.a");
    let (client_b, server_b) = connected_tcp_pair_at(&mut tcp, "websocket.fanout.b");
    let mut runtime = VmWebSocketRuntime::new();
    let session_a = runtime.open_session(server_a);
    let session_b = runtime.open_session(server_b);

    let sent = runtime
        .send_frame_to_sessions(
            &mut tcp,
            &[session_a, session_b],
            VmWebSocketFrame::Text("fanout".to_string()),
        )
        .expect("fanout send");
    let frame_a = tcp
        .receive(client_a, 4096)
        .expect("receive fanout a")
        .expect("queued fanout a");
    let frame_b = tcp
        .receive(client_b, 4096)
        .expect("receive fanout b")
        .expect("queued fanout b");

    assert_eq!(decode_server_text_frame(&frame_a), "fanout");
    assert_eq!(decode_server_text_frame(&frame_b), "fanout");
    assert_eq!(
        sent,
        vec![(session_a, frame_a.len()), (session_b, frame_b.len())]
    );
    assert_eq!(
        runtime.inspect_session(session_a).expect("inspect a"),
        VmWebSocketSessionInfo {
            stream: server_a,
            open: true,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: frame_a.len(),
            bytes_received: 0,
        }
    );
    assert_eq!(
        runtime.inspect_session(session_b).expect("inspect b"),
        VmWebSocketSessionInfo {
            stream: server_b,
            open: true,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: frame_b.len(),
            bytes_received: 0,
        }
    );
}

/// Verifies fan-out rejects invalid send sets before writing bytes.
///
/// Inputs:
/// - One open session, one closed session, and one duplicate send set.
///
/// Output:
/// - Test passes when invalid send sets return stable diagnostics and no
///   frame is queued to the open client.
///
/// Transformation:
/// - Keeps actor-level fan-out atomic for registry and lifecycle validation
///   errors instead of partially delivering messages.
#[test]
fn vm_websocket_runtime_fanout_rejects_invalid_sets_without_partial_send() {
    let mut tcp = VmTcpRuntime::new();
    let (open_client, open_server) = connected_tcp_pair_at(&mut tcp, "websocket.fanout.open");
    let (closed_client, closed_server) = connected_tcp_pair_at(&mut tcp, "websocket.fanout.closed");
    let mut runtime = VmWebSocketRuntime::new();
    let open = runtime.open_session(open_server);
    let closed = runtime.open_session(closed_server);
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
        .send_frame_to_sessions(
            &mut tcp,
            &[open, open],
            VmWebSocketFrame::Text("duplicate".to_string()),
        )
        .expect_err("duplicate rejected");
    assert_eq!(
        duplicate,
        "error[vm_websocket_session]: duplicate session handle in send set"
    );

    let closed_err = runtime
        .send_frame_to_sessions(
            &mut tcp,
            &[open, closed],
            VmWebSocketFrame::Text("closed".to_string()),
        )
        .expect_err("closed rejected");
    assert_eq!(closed_err, "error[vm_websocket_session]: session is closed");
    assert_eq!(
        tcp.receive(open_client, 4096).expect("open client receive"),
        None
    );
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
}

/// Verifies best-effort fan-out reports invalid sessions while sending valid ones.
///
/// Inputs:
/// - One open session, one peer-closed session, and one fabricated unknown
///   session handle.
///
/// Output:
/// - Test passes when the open client receives the frame and the invalid
///   entries report per-session errors.
///
/// Transformation:
/// - Gives room actors a non-atomic broadcast path for stale membership lists
///   while keeping diagnostics explicit.
#[test]
fn vm_websocket_runtime_best_effort_fanout_reports_partial_results() {
    let mut tcp = VmTcpRuntime::new();
    let (open_client, open_server) =
        connected_tcp_pair_at(&mut tcp, "websocket.best_effort_fanout.open");
    let (closed_client, closed_server) =
        connected_tcp_pair_at(&mut tcp, "websocket.best_effort_fanout.closed");
    let mut runtime = VmWebSocketRuntime::new();
    let open = runtime.open_session(open_server);
    let closed = runtime.open_session(closed_server);
    let unknown = VmWebSocketSessionId { id: 99_999 };
    tcp.send(
        closed_client,
        encode_client_control_frame(VmWebSocketControlFrame::Close),
    )
    .expect("send close");
    runtime
        .receive_frame(&mut tcp, closed, 4096)
        .expect("receive close")
        .expect("closed event");

    let outcomes = runtime.send_frame_to_sessions_best_effort(
        &mut tcp,
        &[unknown, open, closed],
        VmWebSocketFrame::Text("best effort".to_string()),
    );
    let open_frame = tcp
        .receive(open_client, 4096)
        .expect("receive open fanout")
        .expect("queued open fanout");

    assert_eq!(decode_server_text_frame(&open_frame), "best effort");
    assert_eq!(
        outcomes,
        vec![
            VmWebSocketSendOutcome {
                session: unknown,
                result: Err("VM WebSocket session handle is unknown".to_string()),
            },
            VmWebSocketSendOutcome {
                session: open,
                result: Ok(open_frame.len()),
            },
            VmWebSocketSendOutcome {
                session: closed,
                result: Err("error[vm_websocket_session]: session is closed".to_string()),
            },
        ]
    );
    assert_eq!(
        tcp.receive(closed_client, 4096)
            .expect("closed client receive"),
        None
    );
    assert_eq!(
        runtime.inspect_session(open).expect("inspect open"),
        VmWebSocketSessionInfo {
            stream: open_server,
            open: true,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: open_frame.len(),
            bytes_received: 0,
        }
    );
}

/// Verifies best-effort fan-out reports duplicates without replaying frames.
///
/// Inputs:
/// - One open session repeated twice in a best-effort send set.
///
/// Output:
/// - Test passes when the client receives one frame and the duplicate entry
///   reports a stable diagnostic.
///
/// Transformation:
/// - Prevents accidental duplicate actor references from duplicating user
///   visible WebSocket messages.
#[test]
fn vm_websocket_runtime_best_effort_fanout_reports_duplicates() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let mut runtime = VmWebSocketRuntime::new();
    let session = runtime.open_session(server);

    let outcomes = runtime.send_frame_to_sessions_best_effort(
        &mut tcp,
        &[session, session],
        VmWebSocketFrame::Text("once".to_string()),
    );
    let frame = tcp
        .receive(client, 4096)
        .expect("receive frame")
        .expect("queued frame");

    assert_eq!(decode_server_text_frame(&frame), "once");
    assert_eq!(tcp.receive(client, 4096).expect("client receive"), None);
    assert_eq!(
        outcomes,
        vec![
            VmWebSocketSendOutcome {
                session,
                result: Ok(frame.len()),
            },
            VmWebSocketSendOutcome {
                session,
                result: Err(
                    "error[vm_websocket_session]: duplicate session handle in send set".to_string(),
                ),
            },
        ]
    );
    assert_eq!(
        runtime.inspect_session(session).expect("inspect"),
        VmWebSocketSessionInfo {
            stream: server,
            open: true,
            frames_sent: 1,
            frames_received: 0,
            bytes_sent: frame.len(),
            bytes_received: 0,
        }
    );
}
