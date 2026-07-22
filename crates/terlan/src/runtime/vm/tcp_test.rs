use super::super::process::VmProcessId;
use super::{VmTcpListener, VmTcpRuntime, VmTcpStream, VmTcpWake};

/// Verifies listener, connect, accept, send, receive, and inspection flow.
///
/// Inputs:
/// - One logical listener, one client stream, and one accepted server stream.
///
/// Output:
/// - Test passes when bytes cross in both directions and inspection reports
///   owner and queued byte state.
///
/// Transformation:
/// - Exercises VM-owned stream semantics without host async or OS socket state.
#[test]
fn tcp_runtime_accepts_streams_and_moves_bytes_between_peers() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("127.0.0.1:8080").expect("listen");
    let client = runtime
        .connect("127.0.0.1:8080", "client_actor")
        .expect("connect");
    let server = runtime
        .accept(listener, "server_actor")
        .expect("accept")
        .expect("queued stream");

    assert_eq!(runtime.send(client, b"hello".to_vec()).expect("send"), 5);
    assert_eq!(
        runtime
            .inspect_stream(server)
            .expect("inspect")
            .queued_bytes,
        5
    );
    assert_eq!(
        runtime.receive(server, 1024).expect("receive"),
        Some(b"hello".to_vec())
    );
    assert_eq!(runtime.receive(server, 1024).expect("empty"), None);

    assert_eq!(runtime.send(server, b"ok".to_vec()).expect("reply"), 2);
    assert_eq!(
        runtime.receive(client, 1024).expect("client receive"),
        Some(b"ok".to_vec())
    );
    assert_eq!(
        runtime.inspect_stream(client).expect("client info").owner,
        Some("client_actor".to_string())
    );
    assert_eq!(
        runtime.inspect_stream(server).expect("server info").owner,
        Some("server_actor".to_string())
    );
}

#[test]
fn tcp_runtime_termination_releases_buffers_and_waiters() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("queued stream");
    let reader = VmProcessId::from_raw_for_test(1);
    let writer = VmProcessId::from_raw_for_test(2);

    runtime
        .park_receive(server, reader)
        .expect("park server reader");
    runtime
        .set_stream_inbox_limit(client, 8)
        .expect("limit client inbox");
    runtime.send(server, b"response".to_vec()).expect("send");
    assert!(runtime
        .park_send(server, writer)
        .expect("park server writer"));
    assert_eq!(runtime.metrics().waiting_readers, 1);
    assert_eq!(runtime.metrics().waiting_writers, 1);
    assert_eq!(runtime.metrics().queued_bytes, 8);

    runtime.close_stream(server).expect("close server");
    runtime.cancel_stream(client).expect("cancel client");
    runtime.close_listener(listener).expect("close listener");

    let metrics = runtime.metrics();
    assert_eq!(metrics.open_listeners, 0);
    assert_eq!(metrics.open_streams, 0);
    assert_eq!(metrics.queued_messages, 0);
    assert_eq!(metrics.queued_bytes, 0);
    assert_eq!(metrics.waiting_readers, 0);
    assert_eq!(metrics.waiting_writers, 0);
}

#[test]
fn tcp_runtime_listener_close_releases_backlog_streams_and_accept_waiters() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let first = runtime.connect("service", "first").expect("connect first");
    let second = runtime
        .connect("service", "second")
        .expect("connect second");
    let waiting_listener = runtime.listen("waiting").expect("listen waiting");
    runtime
        .park_accept(waiting_listener, VmProcessId::from_raw_for_test(1))
        .expect("park accept waiter");

    assert_eq!(runtime.metrics().queued_accepts, 2);
    assert_eq!(runtime.metrics().waiting_readers, 1);
    runtime.close_listener(listener).expect("close listener");
    runtime
        .close_listener(waiting_listener)
        .expect("close waiting listener");

    let metrics = runtime.metrics();
    assert_eq!(metrics.open_listeners, 0);
    assert_eq!(metrics.queued_accepts, 0);
    assert_eq!(metrics.waiting_readers, 0);
    assert_eq!(
        runtime
            .send(first, b"late".to_vec())
            .expect_err("first peer closed"),
        "VM TCP peer stream is closed"
    );
    assert_eq!(
        runtime
            .send(second, b"late".to_vec())
            .expect_err("second peer closed"),
        "VM TCP peer stream is closed"
    );
}

/// Verifies write-side half-close keeps the read side open.
///
/// Inputs:
/// - One connected VM TCP client/server pair.
/// - A request payload followed by client write-side close.
///
/// Output:
/// - Test passes when the client can no longer send, the server can observe
///   peer write EOF, and the server can still send a response to the client.
///
/// Transformation:
/// - Models HTTP request EOF without using full stream close, which would make
///   the response path unavailable.
#[test]
fn tcp_runtime_write_half_close_blocks_sender_but_allows_peer_reply() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("queued stream");

    runtime.send(client, b"request".to_vec()).expect("send");
    runtime.close_write(client).expect("half close client");

    assert_eq!(
        runtime
            .inspect_stream(client)
            .expect("inspect client")
            .write_closed,
        true
    );
    assert_eq!(
        runtime
            .peer_write_closed(server)
            .expect("server sees peer EOF"),
        true
    );
    assert_eq!(
        runtime.receive(server, 1024).expect("server receive"),
        Some(b"request".to_vec())
    );
    assert_eq!(runtime.receive(server, 1024).expect("server empty"), None);
    assert_eq!(
        runtime
            .send(client, b"second request".to_vec())
            .expect_err("client write side is closed"),
        "VM TCP stream write side is closed"
    );
    assert_eq!(
        runtime
            .park_send(client, VmProcessId::from_raw_for_test(900))
            .expect_err("client write side park send is closed"),
        "VM TCP stream write side is closed"
    );

    runtime.send(server, b"response".to_vec()).expect("reply");
    assert_eq!(
        runtime.receive(client, 1024).expect("client receive"),
        Some(b"response".to_vec())
    );
}

/// Verifies receive splitting and listener backlog behavior.
///
/// Inputs:
/// - Two client connections and one oversized payload.
///
/// Output:
/// - Test passes when accept preserves FIFO order and bounded reads preserve
///   unread bytes.
///
/// Transformation:
/// - Models stream backpressure-facing reads without depending on a socket
///   scheduler.
#[test]
fn tcp_runtime_preserves_accept_order_and_splits_large_receives() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let first = runtime.connect("service", "first").expect("first");
    let second = runtime.connect("service", "second").expect("second");
    let accepted_first = runtime
        .accept(listener, "server")
        .expect("accept first")
        .expect("first queued");
    let accepted_second = runtime
        .accept(listener, "server")
        .expect("accept second")
        .expect("second queued");

    runtime.send(first, b"abcdef".to_vec()).expect("send");
    assert_eq!(
        runtime.receive(accepted_first, 2).expect("chunk one"),
        Some(b"ab".to_vec())
    );
    assert_eq!(
        runtime.receive(accepted_first, 4).expect("chunk two"),
        Some(b"cdef".to_vec())
    );
    runtime.send(second, b"z".to_vec()).expect("send second");
    assert_eq!(
        runtime.receive(accepted_second, 1).expect("second receive"),
        Some(b"z".to_vec())
    );
    assert_eq!(runtime.accept(listener, "server").expect("empty"), None);
}

/// Verifies bounded listener backlog backpressure.
///
/// Inputs:
/// - A listener with backlog capacity one and two attempted clients.
///
/// Output:
/// - Test passes when the second connect is rejected until the server accepts
///   the first stream.
///
/// Transformation:
/// - Locks VM-owned accept-side backpressure before production HTTP consumes
///   TCP listener resources.
#[test]
fn tcp_runtime_applies_listener_backlog_limit() {
    let mut runtime = VmTcpRuntime::new();
    assert_eq!(
        runtime
            .listen_with_backlog("bad", 0)
            .expect_err("zero backlog should fail"),
        "VM TCP listener backlog limit must be greater than 0"
    );
    let listener = runtime
        .listen_with_backlog("service", 1)
        .expect("bounded listener");
    let first = runtime.connect("service", "first").expect("first");

    assert_eq!(
        runtime
            .connect("service", "second")
            .expect_err("backlog full"),
        "VM TCP listener `service` backlog is full"
    );

    let accepted = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("queued stream");
    let second = runtime.connect("service", "second").expect("second");

    runtime.send(first, b"one".to_vec()).expect("first send");
    assert_eq!(
        runtime.receive(accepted, 3).expect("receive first"),
        Some(b"one".to_vec())
    );
    assert_eq!(
        runtime.inspect_stream(second).expect("second stream").owner,
        Some("second".to_string())
    );
}

/// Verifies listener inspection reports accept-side pressure and lifecycle.
///
/// Inputs:
/// - One bounded listener, one parked acceptor, one queued connection, and a
///   close operation.
///
/// Output:
/// - Test passes when listener inspection exposes address, backlog capacity,
///   queued accepts, waiting acceptors, and closed state.
///
/// Transformation:
/// - Locks VM-owned listener observability needed by production HTTP stream
///   scheduling without exposing host socket descriptors.
#[test]
fn tcp_runtime_inspects_listener_backlog_waiters_and_closed_state() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime
        .listen_with_backlog("service", 2)
        .expect("bounded listener");

    let initial = runtime.inspect_listener(listener).expect("inspect initial");
    assert_eq!(initial.address, "service");
    assert_eq!(initial.backlog_limit, 2);
    assert_eq!(initial.queued_accepts, 0);
    assert_eq!(initial.waiting_acceptors, 0);
    assert!(!initial.closed);

    let process = VmProcessId::from_raw_for_test(7);
    assert!(runtime.park_accept(listener, process).expect("park accept"));
    let parked = runtime.inspect_listener(listener).expect("inspect parked");
    assert_eq!(parked.waiting_acceptors, 1);

    let _client = runtime.connect("service", "client").expect("connect");
    let queued = runtime.inspect_listener(listener).expect("inspect queued");
    assert_eq!(queued.queued_accepts, 1);
    assert_eq!(queued.waiting_acceptors, 0);

    runtime.close_listener(listener).expect("close listener");
    let closed = runtime.inspect_listener(listener).expect("inspect closed");
    assert!(closed.closed);
    assert_eq!(
        runtime
            .inspect_listener(VmTcpListener { id: 404 })
            .expect_err("unknown listener"),
        "VM TCP listener handle is unknown"
    );
}

/// Verifies stream inbox backpressure.
///
/// Inputs:
/// - One stream pair with a small unread-byte limit on the server side.
///
/// Output:
/// - Test passes when writes beyond queued capacity are rejected until the
///   receiver drains bytes.
///
/// Transformation:
/// - Locks VM-owned stream write backpressure before HTTP relies on TCP
///   resources for handler scheduling.
#[test]
fn tcp_runtime_applies_stream_inbox_limit() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");

    assert_eq!(
        runtime
            .set_stream_inbox_limit(server, 0)
            .expect_err("zero inbox limit"),
        "VM TCP stream inbox limit must be greater than 0"
    );
    runtime
        .set_stream_inbox_limit(server, 5)
        .expect("set inbox limit");
    assert_eq!(
        runtime.inspect_stream(server).expect("inspect").inbox_limit,
        5
    );
    assert_eq!(runtime.send(client, b"abc".to_vec()).expect("first"), 3);
    assert_eq!(
        runtime
            .send(client, b"def".to_vec())
            .expect_err("inbox full"),
        "VM TCP peer inbox is full"
    );
    assert_eq!(
        runtime.receive(server, 3).expect("drain"),
        Some(b"abc".to_vec())
    );
    assert_eq!(runtime.send(client, b"def".to_vec()).expect("second"), 3);
}

/// Verifies write readiness wakeups when receive drains peer capacity.
///
/// Inputs:
/// - A full peer inbox and a sender process parked on write readiness.
///
/// Output:
/// - Test passes when receive returns a write wake intent and clears the
///   writer wait list.
///
/// Transformation:
/// - Models VM-owned write backpressure without host async readiness APIs.
#[test]
fn tcp_runtime_parks_send_and_reports_wakeup_when_peer_drains_capacity() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    let writer = VmProcessId::from_raw_for_test(31);

    runtime
        .set_stream_inbox_limit(server, 3)
        .expect("set inbox limit");
    runtime.send(client, b"abc".to_vec()).expect("fill inbox");
    assert!(runtime.park_send(client, writer).expect("park writer"));
    assert!(runtime
        .park_send(client, writer)
        .expect("duplicate writer suppressed"));
    assert_eq!(
        runtime
            .inspect_stream(server)
            .expect("inspect server")
            .waiting_writers,
        1
    );

    let (received, wakeups) = runtime
        .receive_with_wakeups(server, 3)
        .expect("receive with wakeups");

    assert_eq!(received, Some(b"abc".to_vec()));
    assert_eq!(
        wakeups,
        vec![VmTcpWake::Write {
            process: writer,
            stream: client
        }]
    );
    assert_eq!(
        runtime
            .inspect_stream(server)
            .expect("inspect server")
            .waiting_writers,
        0
    );
    assert!(!runtime
        .park_send(client, writer)
        .expect("available peer capacity should not park"));
}

/// Verifies close, cancel, cleanup, and stable diagnostics.
///
/// Inputs:
/// - Closed listeners, cancelled streams, owner cleanup, and invalid handles
///   reached through public runtime operations.
///
/// Output:
/// - Test passes when lifecycle diagnostics stay stable.
///
/// Transformation:
/// - Locks VM ownership semantics before HTTP uses these stream resources.
#[test]
fn tcp_runtime_rejects_closed_cancelled_and_invalid_resources() {
    let mut runtime = VmTcpRuntime::new();
    assert_eq!(
        runtime.listen(" ").expect_err("empty listener"),
        "VM TCP listener address cannot be empty"
    );
    let listener = runtime.listen("service").expect("listen");
    assert_eq!(
        runtime.listen("service").expect_err("duplicate listener"),
        "VM TCP listener `service` already exists"
    );
    assert_eq!(
        runtime
            .connect("missing", "client")
            .expect_err("missing listener"),
        "VM TCP listener `missing` was not found"
    );

    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    runtime.close_listener(listener).expect("close listener");
    assert_eq!(
        runtime
            .accept(listener, "server")
            .expect_err("accept closed"),
        "VM TCP listener is closed"
    );

    runtime.close_stream(server).expect("close server");
    assert_eq!(
        runtime
            .send(client, b"data".to_vec())
            .expect_err("peer closed"),
        "VM TCP peer stream is closed"
    );

    runtime.cancel_stream(client).expect("cancel client");
    assert_eq!(
        runtime.receive(client, 1).expect_err("cancelled receive"),
        "VM TCP stream is cancelled"
    );

    let listener = runtime.listen("other").expect("listen other");
    let owned = runtime.connect("other", "owned").expect("owned");
    assert_eq!(runtime.close_owner_streams("owned"), 1);
    assert!(runtime.inspect_stream(owned).expect("inspect owned").closed);
    assert_eq!(runtime.close_owner_streams("unknown"), 0);
    assert_eq!(
        runtime
            .accept(listener, "server")
            .expect("accept owned")
            .is_some(),
        true
    );
}

/// Verifies receive limit validation.
///
/// Inputs: one accepted stream and zero max byte limit.
/// Output: stable diagnostic.
/// Transformation: rejects impossible read contracts before touching stream
/// state.
#[test]
fn tcp_runtime_rejects_zero_receive_limit() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    runtime.connect("service", "client").expect("connect");
    let stream = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("stream");

    assert_eq!(
        runtime.receive(stream, 0).expect_err("zero max bytes"),
        "VM TCP receive max_bytes must be greater than 0"
    );
}

/// Verifies adversarial stream/listener diagnostics.
///
/// Inputs:
/// - Unknown handles, orphan streams, closed/cancelled streams, and cancelled
///   peers.
///
/// Output:
/// - Stable diagnostics for every public TCP operation that touches invalid or
///   unavailable resources.
///
/// Transformation:
/// - Keeps VM TCP failure behavior explicit before higher-level HTTP and
///   actor abstractions depend on these handles.
#[test]
fn tcp_runtime_reports_unknown_or_unavailable_resource_paths() {
    let mut runtime = VmTcpRuntime::new();
    let unknown_listener = VmTcpListener { id: 404 };
    let unknown_stream = VmTcpStream { id: 505 };
    let process = VmProcessId::from_raw_for_test(41);

    assert_eq!(
        runtime
            .accept(unknown_listener, "server")
            .expect_err("unknown accept"),
        "VM TCP listener handle is unknown"
    );
    assert_eq!(
        runtime
            .park_accept(unknown_listener, process)
            .expect_err("unknown park accept"),
        "VM TCP listener handle is unknown"
    );
    assert_eq!(
        runtime
            .close_listener(unknown_listener)
            .expect_err("unknown close listener"),
        "VM TCP listener handle is unknown"
    );
    assert_eq!(
        runtime
            .inspect_stream(unknown_stream)
            .expect_err("unknown inspect"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .close_stream(unknown_stream)
            .expect_err("unknown close stream"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .close_write(unknown_stream)
            .expect_err("unknown close write"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .peer_write_closed(unknown_stream)
            .expect_err("unknown peer write closed"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .set_stream_inbox_limit(unknown_stream, 1)
            .expect_err("unknown set inbox limit"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .receive(unknown_stream, 1)
            .expect_err("unknown receive"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .park_receive(unknown_stream, process)
            .expect_err("unknown park receive"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .send(unknown_stream, b"x".to_vec())
            .expect_err("unknown send"),
        "VM TCP stream handle is unknown"
    );
    assert_eq!(
        runtime
            .park_send(unknown_stream, process)
            .expect_err("unknown park send"),
        "VM TCP stream handle is unknown"
    );

    let orphan = runtime.allocate_stream(Some("orphan".to_string()));
    assert_eq!(
        runtime
            .send(orphan, b"x".to_vec())
            .expect_err("orphan send"),
        "VM TCP stream has no connected peer"
    );
    assert_eq!(
        runtime
            .park_send(orphan, process)
            .expect_err("orphan park send"),
        "VM TCP stream has no connected peer"
    );
    assert_eq!(
        runtime
            .peer_write_closed(orphan)
            .expect_err("orphan peer write closed"),
        "VM TCP stream has no connected peer"
    );

    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("stream");

    runtime.close_stream(client).expect("close client");
    assert_eq!(
        runtime
            .send(client, b"x".to_vec())
            .expect_err("closed send"),
        "VM TCP stream is closed"
    );
    assert_eq!(
        runtime
            .park_send(client, process)
            .expect_err("closed park send"),
        "VM TCP stream is closed"
    );
    assert_eq!(
        runtime
            .park_receive(client, process)
            .expect_err("closed park receive"),
        "VM TCP stream is closed"
    );
    assert_eq!(
        runtime.close_write(client).expect_err("closed close write"),
        "VM TCP stream is closed"
    );

    let listener = runtime.listen("peer-closed").expect("listen peer closed");
    let client_with_closed_peer = runtime
        .connect("peer-closed", "client")
        .expect("connect peer closed");
    let closed_peer = runtime
        .accept(listener, "server")
        .expect("accept peer closed")
        .expect("closed peer");
    runtime.close_stream(closed_peer).expect("close peer");
    assert_eq!(
        runtime
            .park_send(client_with_closed_peer, process)
            .expect_err("closed peer park send"),
        "VM TCP peer stream is closed"
    );

    runtime.cancel_stream(server).expect("cancel server");
    assert_eq!(
        runtime
            .send(orphan, b"x".to_vec())
            .expect_err("orphan remains no peer"),
        "VM TCP stream has no connected peer"
    );

    let listener = runtime.listen("peer-cancel").expect("listen peer cancel");
    let client = runtime
        .connect("peer-cancel", "client")
        .expect("connect peer cancel");
    let server = runtime
        .accept(listener, "server")
        .expect("accept peer cancel")
        .expect("peer stream");
    runtime.cancel_stream(server).expect("cancel peer");
    assert_eq!(
        runtime
            .send(client, b"x".to_vec())
            .expect_err("cancelled peer send"),
        "VM TCP peer stream is cancelled"
    );
    assert_eq!(
        runtime
            .park_send(client, process)
            .expect_err("cancelled peer park send"),
        "VM TCP peer stream is cancelled"
    );

    runtime.cancel_stream(client).expect("cancel client");
    assert_eq!(
        runtime
            .send(client, b"x".to_vec())
            .expect_err("cancelled send"),
        "VM TCP stream is cancelled"
    );
    assert_eq!(
        runtime
            .park_send(client, process)
            .expect_err("cancelled park send"),
        "VM TCP stream is cancelled"
    );
    assert_eq!(
        runtime
            .park_receive(client, process)
            .expect_err("cancelled park receive"),
        "VM TCP stream is cancelled"
    );
    assert_eq!(
        runtime
            .close_write(client)
            .expect_err("cancelled close write"),
        "VM TCP stream is cancelled"
    );
}

/// Verifies accept readiness wake intents.
///
/// Inputs:
/// - One listener with no accepted streams and one parked VM process.
///
/// Output:
/// - Test passes when a later connection produces one accept wake intent and
///   the stream remains available for accept.
///
/// Transformation:
/// - Locks the scheduler-facing contract without coupling TCP to scheduler
///   queue internals.
#[test]
fn tcp_runtime_parks_accept_and_reports_wakeup_when_connection_arrives() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let process = VmProcessId::from_raw_for_test(11);

    assert!(runtime.park_accept(listener, process).expect("park accept"));
    assert!(runtime
        .park_accept(listener, process)
        .expect("duplicate park suppresses waiter"));

    let (_client, wakeups) = runtime
        .connect_with_wakeups("service", "client")
        .expect("connect");
    assert_eq!(wakeups, vec![VmTcpWake::Accept { process, listener }]);
    assert!(!runtime
        .park_accept(listener, process)
        .expect("ready listener should not park"));

    let accepted = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("stream remains queued");
    assert_eq!(
        runtime.inspect_stream(accepted).expect("inspect").owner,
        Some("server".to_string())
    );
    assert!(runtime
        .park_accept(listener, process)
        .expect("empty after accept can park again"));

    runtime.close_listener(listener).expect("close listener");
    assert_eq!(
        runtime
            .park_accept(listener, process)
            .expect_err("closed listener should reject park"),
        "VM TCP listener is closed"
    );
}

/// Verifies receive does not wake writers while peer capacity remains full.
///
/// Inputs:
/// - A full inbox whose first queued message is empty and whose second message
///   keeps the inbox at the configured limit after receive.
///
/// Output:
/// - Test passes when no write wake intent is returned.
///
/// Transformation:
/// - Locks the TCP backpressure edge where a receive succeeds but still leaves
///   no room for blocked writers.
#[test]
fn tcp_runtime_receive_does_not_wake_writers_when_capacity_remains_full() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    let writer = VmProcessId::from_raw_for_test(51);

    runtime
        .set_stream_inbox_limit(server, 5)
        .expect("set inbox limit");
    runtime.send(client, Vec::new()).expect("empty send");
    runtime.send(client, b"abcde".to_vec()).expect("full send");
    assert!(runtime.park_send(client, writer).expect("park writer"));

    let (received, wakeups) = runtime
        .receive_with_wakeups(server, 1)
        .expect("receive empty chunk");

    assert_eq!(received, Some(Vec::new()));
    assert!(wakeups.is_empty());
    assert_eq!(
        runtime
            .inspect_stream(server)
            .expect("inspect server")
            .waiting_writers,
        1
    );
}

/// Verifies readable readiness wake intents.
///
/// Inputs:
/// - One connected stream pair and one parked reader process.
///
/// Output:
/// - Test passes when sending bytes wakes the parked reader exactly once and
///   immediate-readable streams do not park.
///
/// Transformation:
/// - Models the future scheduler handoff for HTTP stream reads without host
///   async state.
#[test]
fn tcp_runtime_parks_receive_and_reports_wakeup_when_bytes_arrive() {
    let mut runtime = VmTcpRuntime::new();
    let listener = runtime.listen("service").expect("listen");
    let client = runtime.connect("service", "client").expect("connect");
    let server = runtime
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    let process = VmProcessId::from_raw_for_test(21);

    assert!(runtime.park_receive(server, process).expect("park receive"));
    assert!(runtime
        .park_receive(server, process)
        .expect("duplicate receive waiter"));
    assert_eq!(
        runtime
            .inspect_stream(server)
            .expect("inspect")
            .waiting_readers,
        1
    );

    let (sent, wakeups) = runtime
        .send_with_wakeups(client, b"hello".to_vec())
        .expect("send");
    assert_eq!(sent, 5);
    assert_eq!(
        wakeups,
        vec![VmTcpWake::Read {
            process,
            stream: server
        }]
    );
    assert_eq!(
        runtime
            .inspect_stream(server)
            .expect("inspect")
            .waiting_readers,
        0
    );
    assert!(!runtime
        .park_receive(server, process)
        .expect("readable stream does not park"));
    assert_eq!(
        runtime.receive(server, 1024).expect("receive"),
        Some(b"hello".to_vec())
    );
}
