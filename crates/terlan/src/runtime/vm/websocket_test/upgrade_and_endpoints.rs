use super::*;

use tungstenite::protocol::{frame::Frame, Message, Role};

/// Verifies the VM WebSocket handshake response uses tungstenite's accept key.
///
/// Inputs:
/// - RFC example `Sec-WebSocket-Key`.
///
/// Output:
/// - Test passes when the VM response status and headers match the expected
///   protocol-switch metadata.
///
/// Transformation:
/// - Proves the VM owns the handshake response shape while delegating the
///   protocol hash to maintained tungstenite code.
#[test]
pub(super) fn vm_websocket_upgrade_response_uses_tungstenite_accept_key() {
    let response =
        build_websocket_upgrade_response("dGhlIHNhbXBsZSBub25jZQ==").expect("upgrade response");

    assert_eq!(response.status, 101);
    assert_eq!(
        response.headers,
        vec![
            ("upgrade".to_string(), "websocket".to_string()),
            ("connection".to_string(), "Upgrade".to_string()),
            (
                "sec-websocket-accept".to_string(),
                "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=".to_string(),
            ),
        ]
    );
}

/// Verifies blank WebSocket keys never produce protocol-switch metadata.
///
/// Inputs:
/// - Whitespace-only `Sec-WebSocket-Key`.
///
/// Output:
/// - Test passes when the VM reports a stable diagnostic.
///
/// Transformation:
/// - Keeps malformed handshakes out of later VM WebSocket stream scheduling.
#[test]
pub(super) fn vm_websocket_upgrade_response_rejects_blank_key() {
    let err = build_websocket_upgrade_response("   ").expect_err("blank key");

    assert_eq!(err, "error[vm_websocket]: missing Sec-WebSocket-Key");
}

/// Verifies source-level WebSocket adapter constructors build VM frame values.
///
/// Inputs:
/// - Text, ping, pong, and close adapter constructor calls.
///
/// Output:
/// - Test passes when each constructor returns the corresponding VM frame.
///
/// Transformation:
/// - Locks `std.http.WebSocket` NativeBoundary manifest rows to concrete VM-owned
///   adapter functions instead of placeholder declarations.
#[test]
pub(super) fn vm_websocket_adapter_frame_constructors_build_typed_frames() {
    assert_eq!(
        text("hello".to_string()),
        VmWebSocketFrame::Text("hello".to_string())
    );
    assert_eq!(
        ping("alive".to_string()),
        VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(b"alive".to_vec()))
    );
    assert_eq!(
        pong("alive".to_string()),
        VmWebSocketFrame::Control(VmWebSocketControlFrame::Pong(b"alive".to_vec()))
    );
    assert_eq!(
        close(),
        VmWebSocketFrame::Control(VmWebSocketControlFrame::Close)
    );
}

/// Verifies source-level WebSocket endpoint constructors validate bounds.
///
/// Inputs:
/// - Valid endpoint queue/frame limits.
/// - Zero pending-frame and frame-byte limits.
///
/// Output:
/// - Test passes when valid plans are preserved and invalid limits produce
///   stable VM diagnostics.
///
/// Transformation:
/// - Keeps route-level WebSocket endpoint descriptors bounded before any live
///   socket state is allocated.
#[test]
pub(super) fn vm_websocket_adapter_endpoint_validates_channel_limits() {
    assert_eq!(
        endpoint(16, 4096).expect("endpoint plan"),
        VmWebSocketEndpointPlan {
            max_pending_frames: 16,
            max_frame_bytes: 4096,
            binary_payload_policy: VmWebSocketBinaryPayloadPolicy::Reject,
            callbacks: None,
        }
    );
    let pending = endpoint(0, 4096).expect_err("zero pending frames");
    assert_eq!(pending.domain(), terlan_runtime_abi::ErrorDomain::VmRuntime);
    assert_eq!(pending.code(), "vm_websocket_endpoint");
    assert_eq!(
        pending.context(),
        "error[vm_websocket_endpoint]: max_pending_frames must be greater than 0"
    );
    let frame = endpoint(16, 0).expect_err("zero frame bytes");
    assert_eq!(frame.code(), "vm_websocket_endpoint");
    assert_eq!(
        frame.context(),
        "error[vm_websocket_endpoint]: max_frame_bytes must be greater than 0"
    );
}

/// Verifies endpoint plans open VM-owned inbound queues with the same bounds.
///
/// Inputs:
/// - Valid WebSocket endpoint plan.
///
/// Output:
/// - Test passes when the opened queue reports the plan's pending-frame and
///   frame-byte limits.
///
/// Transformation:
/// - Connects source-level endpoint declarations to VM-owned per-connection
///   buffering instead of leaving limits as inert metadata.
#[test]
pub(super) fn vm_websocket_endpoint_opens_bounded_inbound_queue() {
    let endpoint = VmWebSocketEndpointPlan::new(3, 8).expect("endpoint plan");
    let queue = endpoint.open_inbound_queue();

    assert_eq!(
        queue.inspect(),
        VmWebSocketInboundQueueInfo {
            pending_frames: 0,
            max_pending_frames: 3,
            queued_frame_bytes: 0,
            max_frame_bytes: 8,
        }
    );
}

/// Verifies inbound queues preserve frame order and byte-pressure counters.
///
/// Inputs:
/// - Text, ping, and close frames pushed into one bounded queue.
///
/// Output:
/// - Test passes when frames pop in FIFO order and queued byte counters shrink
///   as payload-bearing frames are consumed.
///
/// Transformation:
/// - Gives later scheduler receive loops a deterministic VM-owned buffer
///   contract.
#[test]
pub(super) fn vm_websocket_inbound_queue_preserves_order_and_pressure() {
    let mut queue = VmWebSocketEndpointPlan::new(4, 16)
        .expect("endpoint plan")
        .open_inbound_queue();

    queue
        .push(VmWebSocketFrame::Text("one".to_string()))
        .expect("push text");
    queue
        .push(VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(
            b"two".to_vec(),
        )))
        .expect("push ping");
    queue
        .push(VmWebSocketFrame::Control(VmWebSocketControlFrame::Close))
        .expect("push close");

    assert_eq!(
        queue.inspect(),
        VmWebSocketInboundQueueInfo {
            pending_frames: 3,
            max_pending_frames: 4,
            queued_frame_bytes: 6,
            max_frame_bytes: 16,
        }
    );
    assert_eq!(queue.pop(), Some(VmWebSocketFrame::Text("one".to_string())));
    assert_eq!(
        queue.pop(),
        Some(VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(
            b"two".to_vec(),
        )))
    );
    assert_eq!(
        queue.inspect(),
        VmWebSocketInboundQueueInfo {
            pending_frames: 1,
            max_pending_frames: 4,
            queued_frame_bytes: 0,
            max_frame_bytes: 16,
        }
    );
    assert_eq!(
        queue.pop(),
        Some(VmWebSocketFrame::Control(VmWebSocketControlFrame::Close))
    );
    assert_eq!(queue.pop(), None);
}

/// Verifies inbound queues reject full and oversized frame cases.
///
/// Inputs:
/// - Queue with one pending-frame slot and small frame-byte limit.
/// - Oversized text and control payloads.
///
/// Output:
/// - Test passes when backpressure and frame-size diagnostics are stable.
///
/// Transformation:
/// - Prevents malformed or overloaded WebSocket clients from bypassing
///   per-connection VM queue bounds.
#[test]
pub(super) fn vm_websocket_inbound_queue_rejects_full_and_oversized_frames() {
    let mut full_queue = VmWebSocketEndpointPlan::new(1, 8)
        .expect("endpoint plan")
        .open_inbound_queue();
    full_queue
        .push(VmWebSocketFrame::Text("ok".to_string()))
        .expect("first frame");

    assert_eq!(
        full_queue
            .push(VmWebSocketFrame::Text("next".to_string()))
            .expect_err("queue full"),
        "error[vm_websocket_queue]: pending frame queue is full"
    );

    let mut size_queue = VmWebSocketEndpointPlan::new(4, 3)
        .expect("endpoint plan")
        .open_inbound_queue();
    assert_eq!(
        size_queue
            .push(VmWebSocketFrame::Text("toolong".to_string()))
            .expect_err("text too large"),
        "error[vm_websocket_queue]: frame exceeds max_frame_bytes"
    );
    assert_eq!(
        size_queue
            .push(VmWebSocketFrame::Control(VmWebSocketControlFrame::Pong(
                b"toolong".to_vec(),
            )))
            .expect_err("control too large"),
        "error[vm_websocket_queue]: frame exceeds max_frame_bytes"
    );
    assert_eq!(
        size_queue.inspect(),
        VmWebSocketInboundQueueInfo {
            pending_frames: 0,
            max_pending_frames: 4,
            queued_frame_bytes: 0,
            max_frame_bytes: 3,
        }
    );
}

/// Verifies endpoint plans declare binary payload rejection explicitly.
///
/// Inputs:
/// - Default endpoint plan.
/// - Client binary WebSocket frame generated by tungstenite.
///
/// Output:
/// - Test passes when the endpoint reports `Reject` and the unified frame
///   decoder returns the stable unsupported-binary diagnostic.
///
/// Transformation:
/// - Makes binary payload behavior an endpoint policy contract instead of an
///   undocumented side effect of the current decoder.
#[test]
pub(super) fn vm_websocket_endpoint_declares_binary_payload_rejection_policy() {
    let endpoint = VmWebSocketEndpointPlan::new(4, 32).expect("endpoint plan");

    assert_eq!(
        endpoint.binary_payload_policy(),
        VmWebSocketBinaryPayloadPolicy::Reject
    );
    assert_eq!(
        decode_client_frame(&encode_client_binary_frame(&[1, 2, 3])).expect_err("binary rejection"),
        "error[vm_websocket_frame]: unsupported frame kind binary"
    );
}

/// Verifies VM WebSocket text decoding accepts real client frames.
///
/// Inputs:
/// - A masked client text frame generated by tungstenite.
///
/// Output:
/// - Test passes when the VM helper returns the text payload.
///
/// Transformation:
/// - Proves the VM frame boundary uses maintained tungstenite parsing instead
///   of hand-written frame or masking logic.
#[test]
pub(super) fn vm_websocket_decodes_tungstenite_client_text_frame() {
    let client_frame = encode_client_text_frame("hello from client");

    let decoded = decode_client_text_frame(&client_frame).expect("decoded client frame");

    assert_eq!(decoded, "hello from client");
}

/// Verifies VM WebSocket text encoding is readable by a tungstenite client.
///
/// Inputs:
/// - Server text payload encoded by the VM helper.
///
/// Output:
/// - Test passes when a tungstenite client decodes the same text payload.
///
/// Transformation:
/// - Locks server-frame serialization to maintained tungstenite behavior while
///   keeping the output bytes owned by the VM runtime.
#[test]
pub(super) fn vm_websocket_encodes_server_text_frame_for_tungstenite_client() {
    let server_frame = encode_server_text_frame("hello from server").expect("server frame");

    let decoded = decode_server_text_frame(&server_frame);

    assert_eq!(decoded, "hello from server");
}

/// Verifies malformed frame bytes fail with a stable VM diagnostic prefix.
///
/// Inputs:
/// - Truncated WebSocket frame header bytes.
///
/// Output:
/// - Test passes when decoding fails before producing a text payload.
///
/// Transformation:
/// - Preserves strict VM diagnostics while leaving low-level frame validation
///   to tungstenite.
#[test]
pub(super) fn vm_websocket_rejects_malformed_text_frame_with_stable_error() {
    let err = decode_client_text_frame(&[0x81, 0xff]).expect_err("malformed frame");

    assert!(
        err.starts_with("error[vm_websocket_frame]: failed to decode text frame:"),
        "{err}"
    );
}

/// Verifies VM TCP can carry a client WebSocket text frame into the VM.
///
/// Inputs:
/// - VM TCP client/server stream pair.
/// - Client text frame generated by tungstenite.
///
/// Output:
/// - Test passes when the server-side VM WebSocket helper decodes the text.
///
/// Transformation:
/// - Proves WebSocket text frames can enter the VM over VM-owned TCP without
///   host sockets or async WebSocket adapters.
#[test]
pub(super) fn vm_websocket_receives_client_text_frame_over_vm_tcp() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let frame = encode_client_text_frame("tcp client frame");
    tcp.send(client, frame).expect("send client frame");

    let decoded = receive_client_text_frame(&mut tcp, server, 4096)
        .expect("receive frame")
        .expect("queued frame");

    assert_eq!(decoded, "tcp client frame");
}

/// Verifies VM TCP can carry a server WebSocket text frame to a client peer.
///
/// Inputs:
/// - VM TCP client/server stream pair.
/// - Server text payload encoded by the VM WebSocket helper.
///
/// Output:
/// - Test passes when a tungstenite client decodes the received frame bytes.
///
/// Transformation:
/// - Proves VM WebSocket output is transport-ready for the VM TCP stream layer.
#[test]
pub(super) fn vm_websocket_sends_server_text_frame_over_vm_tcp() {
    let (mut tcp, client, server) = connected_tcp_pair();

    let written = send_server_text_frame(&mut tcp, server, "tcp server frame").expect("send frame");
    let frame = tcp
        .receive(client, written)
        .expect("receive server frame")
        .expect("queued server frame");

    assert_eq!(decode_server_text_frame(&frame), "tcp server frame");
}

/// Verifies empty VM TCP reads do not become malformed WebSocket diagnostics.
///
/// Inputs:
/// - Empty accepted VM TCP stream.
///
/// Output:
/// - Test passes when no frame is reported.
///
/// Transformation:
/// - Keeps scheduler-facing idle reads distinct from malformed frame bytes.
#[test]
pub(super) fn vm_websocket_receive_over_vm_tcp_reports_none_for_empty_stream() {
    let (mut tcp, _client, server) = connected_tcp_pair();

    let decoded = receive_client_text_frame(&mut tcp, server, 4096).expect("empty receive");

    assert_eq!(decoded, None);
}

/// Verifies direct VM TCP text receive wraps transport diagnostics.
///
/// Inputs:
/// - Empty accepted VM TCP stream.
/// - A zero-byte receive limit rejected by the VM TCP runtime.
///
/// Output:
/// - Test passes when the WebSocket helper returns a stable WebSocket-layer
///   diagnostic instead of leaking the raw VM TCP error directly.
///
/// Transformation:
/// - Keeps HTTP/WebSocket handler diagnostics stable at the VM WebSocket
///   boundary while VM TCP owns stream validation.
#[test]
pub(super) fn vm_websocket_receive_over_vm_tcp_wraps_transport_receive_errors() {
    let (mut tcp, _client, server) = connected_tcp_pair();

    let err = receive_client_text_frame(&mut tcp, server, 0).expect_err("zero limit rejected");

    assert_eq!(
        err,
        "error[vm_websocket_tcp]: failed to receive text frame: VM TCP receive max_bytes must be greater than 0"
    );
}

/// Verifies direct VM TCP text send wraps transport diagnostics.
///
/// Inputs:
/// - Accepted VM TCP stream cancelled before the WebSocket helper sends.
///
/// Output:
/// - Test passes when the WebSocket helper reports a stable send diagnostic.
///
/// Transformation:
/// - Prevents cancelled VM streams from surfacing as raw TCP errors in
///   WebSocket handlers.
#[test]
pub(super) fn vm_websocket_send_over_vm_tcp_wraps_transport_send_errors() {
    let (mut tcp, _client, server) = connected_tcp_pair();
    tcp.cancel_stream(server).expect("cancel server stream");

    let err =
        send_server_text_frame(&mut tcp, server, "cancelled").expect_err("cancelled send rejected");

    assert_eq!(
        err,
        "error[vm_websocket_tcp]: failed to send text frame: VM TCP stream is cancelled"
    );
}

/// Verifies VM WebSocket control decoding accepts a client ping frame.
///
/// Inputs:
/// - A masked client ping frame generated by tungstenite.
///
/// Output:
/// - Test passes when the VM helper reports a typed ping with the same
///   payload.
///
/// Transformation:
/// - Keeps ping parsing delegated to tungstenite while exposing VM-owned
///   control events.
#[test]
pub(super) fn vm_websocket_decodes_client_ping_control_frame() {
    let frame = encode_client_control_frame(VmWebSocketControlFrame::Ping(b"alive".to_vec()));

    let decoded = decode_client_control_frame(&frame).expect("control frame");

    assert_eq!(decoded, VmWebSocketControlFrame::Ping(b"alive".to_vec()));
}

/// Verifies VM WebSocket control decoding accepts client pong frames.
///
/// Inputs:
/// - A masked client pong frame generated by tungstenite.
///
/// Output:
/// - Test passes when the VM helper reports a typed pong with the same payload.
///
/// Transformation:
/// - Covers the control-frame path used by future heartbeat state machines
///   without giving handlers raw tungstenite messages.
#[test]
pub(super) fn vm_websocket_decodes_client_pong_control_frame() {
    let frame = encode_client_control_frame(VmWebSocketControlFrame::Pong(b"alive".to_vec()));

    let decoded = decode_client_control_frame(&frame).expect("pong frame");

    assert_eq!(decoded, VmWebSocketControlFrame::Pong(b"alive".to_vec()));
}

/// Verifies text readers reject control frames with stable diagnostics.
///
/// Inputs:
/// - A masked client ping frame generated by tungstenite.
///
/// Output:
/// - Test passes when the text decoder reports the received control kind.
///
/// Transformation:
/// - Keeps specialized VM WebSocket readers strict and debuggable when a
///   handler chooses a text-only receive path.
#[test]
pub(super) fn vm_websocket_text_reader_rejects_ping_frame() {
    let frame = encode_client_control_frame(VmWebSocketControlFrame::Ping(b"not text".to_vec()));

    let err = decode_client_text_frame(&frame).expect_err("ping rejected by text reader");

    assert_eq!(
        err,
        "error[vm_websocket_frame]: expected text frame, received ping"
    );
}

/// Verifies VM WebSocket control encoding emits a client-readable pong frame.
///
/// Inputs:
/// - Server pong control event encoded by the VM helper.
///
/// Output:
/// - Test passes when a tungstenite client decodes the same pong payload.
///
/// Transformation:
/// - Locks server control-frame serialization to maintained tungstenite
///   behavior.
#[test]
pub(super) fn vm_websocket_encodes_server_pong_control_frame_for_tungstenite_client() {
    let frame = encode_server_control_frame(VmWebSocketControlFrame::Pong(b"alive".to_vec()))
        .expect("server pong frame");

    let decoded = decode_server_control_frame(&frame);

    assert_eq!(decoded, VmWebSocketControlFrame::Pong(b"alive".to_vec()));
}

/// Verifies VM WebSocket close frames are typed as control events.
///
/// Inputs:
/// - A client close frame generated by tungstenite.
///
/// Output:
/// - Test passes when the VM helper reports `Close`.
///
/// Transformation:
/// - Gives future VM session lifecycle code a stable close event without
///   exposing raw tungstenite messages.
#[test]
pub(super) fn vm_websocket_decodes_client_close_control_frame() {
    let frame = encode_client_control_frame(VmWebSocketControlFrame::Close);

    let decoded = decode_client_control_frame(&frame).expect("close frame");

    assert_eq!(decoded, VmWebSocketControlFrame::Close);
}

/// Verifies control-frame readers reject data frames with stable diagnostics.
///
/// Inputs:
/// - A client text frame generated by tungstenite.
///
/// Output:
/// - Test passes when the VM control reader rejects it as non-control input.
///
/// Transformation:
/// - Keeps VM WebSocket session scheduling from accidentally treating data as
///   lifecycle/control traffic.
#[test]
pub(super) fn vm_websocket_control_reader_rejects_text_frame() {
    let frame = encode_client_text_frame("not control");

    let err = decode_client_control_frame(&frame).expect_err("text frame rejection");

    assert_eq!(
        err,
        "error[vm_websocket_frame]: expected control frame, received text"
    );
}

/// Verifies the unified VM frame decoder accepts text and control frames.
///
/// Inputs:
/// - Client text and ping frames generated by tungstenite.
///
/// Output:
/// - Test passes when both decode to typed VM frame events.
///
/// Transformation:
/// - Gives actor code one parser surface for mixed WebSocket streams while
///   keeping protocol handling in tungstenite.
#[test]
pub(super) fn vm_websocket_decode_client_frame_accepts_text_and_control() {
    let text = decode_client_frame(&encode_client_text_frame("mixed text")).expect("text frame");
    let ping = decode_client_frame(&encode_client_control_frame(VmWebSocketControlFrame::Ping(
        b"mixed ping".to_vec(),
    )))
    .expect("ping frame");

    assert_eq!(text, VmWebSocketFrame::Text("mixed text".to_string()));
    assert_eq!(
        ping,
        VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(b"mixed ping".to_vec()))
    );
}

/// Verifies unsupported data frames produce stable VM diagnostics.
///
/// Inputs:
/// - Client binary frame generated by tungstenite.
///
/// Output:
/// - Test passes when the unified decoder rejects the frame kind.
///
/// Transformation:
/// - Keeps the first VM WebSocket actor surface intentionally text/control
///   focused until binary application payloads are designed.
#[test]
pub(super) fn vm_websocket_decode_client_frame_rejects_binary_frame() {
    let err =
        decode_client_frame(&encode_client_binary_frame(&[1, 2, 3])).expect_err("binary rejection");

    assert_eq!(
        err,
        "error[vm_websocket_frame]: unsupported frame kind binary"
    );
}

/// Verifies low-level WebSocket helpers cover control and malformed edges.
///
/// Inputs:
/// - Server ping frame encoding.
/// - Client pong frame decoding through the unified reader.
/// - Malformed control and mixed-frame bytes.
/// - Empty and invalid VM TCP receive requests.
///
/// Output:
/// - Test passes when each helper returns typed VM events or stable
///   diagnostics.
///
/// Transformation:
/// - Keeps WebSocket protocol parsing delegated to tungstenite while locking
///   VM-owned edge diagnostics around control frames and TCP receive wrappers.
#[test]
pub(super) fn vm_websocket_low_level_helpers_cover_control_and_error_edges() {
    let server_ping =
        encode_server_control_frame(VmWebSocketControlFrame::Ping(b"server".to_vec()))
            .expect("server ping");
    assert_eq!(
        decode_server_control_frame(&server_ping),
        VmWebSocketControlFrame::Ping(b"server".to_vec())
    );

    let client_pong =
        encode_client_control_frame(VmWebSocketControlFrame::Pong(b"client".to_vec()));
    assert_eq!(
        decode_client_frame(&client_pong).expect("client pong"),
        VmWebSocketFrame::Control(VmWebSocketControlFrame::Pong(b"client".to_vec()))
    );
    assert_eq!(
        decode_client_text_frame(&client_pong).expect_err("pong rejected by text reader"),
        "error[vm_websocket_frame]: expected text frame, received pong"
    );

    let client_close = encode_client_control_frame(VmWebSocketControlFrame::Close);
    assert_eq!(
        decode_client_text_frame(&client_close).expect_err("close rejected by text reader"),
        "error[vm_websocket_frame]: expected text frame, received close"
    );

    assert_eq!(
        websocket_message_kind(&Message::Frame(Frame::ping(Vec::new()))),
        "frame"
    );

    let control_err =
        decode_client_control_frame(&[0x89, 0xff]).expect_err("malformed control frame");
    assert!(
        control_err.starts_with("error[vm_websocket_frame]: failed to decode control frame:"),
        "{control_err}"
    );

    let frame_err = decode_client_frame(&[0x81, 0xff]).expect_err("malformed mixed frame");
    assert!(
        frame_err.starts_with("error[vm_websocket_frame]: failed to decode frame:"),
        "{frame_err}"
    );

    assert_eq!(
        encode_server_text_frame_with_stream("fail", VmWebSocketMemoryStream::failing_writer())
            .expect_err("text encode write failure"),
        "error[vm_websocket_frame]: failed to encode text frame: IO error: injected websocket write failure"
    );
    assert_eq!(
        encode_control_frame_with_stream(
            Role::Server,
            VmWebSocketControlFrame::Ping(Vec::new()),
            VmWebSocketMemoryStream::failing_writer(),
        )
        .expect_err("control encode write failure"),
        "error[vm_websocket_frame]: failed to encode control frame: IO error: injected websocket write failure"
    );

    let (mut tcp, _client, server) = connected_tcp_pair();
    assert_eq!(
        receive_client_control_frame(&mut tcp, server, 4096).expect("empty control receive"),
        None
    );
    assert_eq!(
        receive_client_control_frame(&mut tcp, server, 0).expect_err("zero limit rejected"),
        "error[vm_websocket_tcp]: failed to receive control frame: VM TCP receive max_bytes must be greater than 0"
    );
}

/// Verifies VM TCP can carry client control frames into the VM.
///
/// Inputs:
/// - VM TCP client/server stream pair.
/// - Client ping frame generated by tungstenite.
///
/// Output:
/// - Test passes when the server-side VM helper decodes the ping payload.
///
/// Transformation:
/// - Proves WebSocket control frames can enter the VM over VM-owned TCP
///   without host sockets or async WebSocket adapters.
#[test]
pub(super) fn vm_websocket_receives_client_control_frame_over_vm_tcp() {
    let (mut tcp, client, server) = connected_tcp_pair();
    let frame = encode_client_control_frame(VmWebSocketControlFrame::Ping(b"tcp".to_vec()));
    tcp.send(client, frame).expect("send client control frame");

    let decoded = receive_client_control_frame(&mut tcp, server, 4096)
        .expect("receive control frame")
        .expect("queued control frame");

    assert_eq!(decoded, VmWebSocketControlFrame::Ping(b"tcp".to_vec()));
}

/// Verifies VM TCP can carry server control frames to a client peer.
///
/// Inputs:
/// - VM TCP client/server stream pair.
/// - Server close frame encoded by the VM WebSocket helper.
///
/// Output:
/// - Test passes when a tungstenite client decodes the received close frame.
///
/// Transformation:
/// - Proves VM WebSocket control output is transport-ready for VM TCP streams.
#[test]
pub(super) fn vm_websocket_sends_server_control_frame_over_vm_tcp() {
    let (mut tcp, client, server) = connected_tcp_pair();

    let written = send_server_control_frame(&mut tcp, server, VmWebSocketControlFrame::Close)
        .expect("send close frame");
    let frame = tcp
        .receive(client, written)
        .expect("receive server control frame")
        .expect("queued server control frame");

    assert_eq!(
        decode_server_control_frame(&frame),
        VmWebSocketControlFrame::Close
    );
}
