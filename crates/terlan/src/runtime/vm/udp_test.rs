use super::super::process::VmProcessId;
use super::{VmUdpRuntime, VmUdpWake};

/// Verifies VM-owned UDP packet bursts, receive wakeups, and inspection.
///
/// Inputs:
/// - Two logical UDP sockets and one parked receiver process.
///
/// Output:
/// - Test passes when packets preserve datagram boundaries, wake the receiver,
///   and report queue pressure through inspection.
///
/// Transformation:
/// - Exercises datagram readiness without OS sockets or an external async
///   runtime.
#[test]
fn udp_runtime_delivers_packet_bursts_and_wakes_receiver() {
    let mut runtime = VmUdpRuntime::new();
    let client = runtime
        .bind_with_inbox_limit("client:1", "client_actor", 4)
        .expect("bind client");
    let server = runtime
        .bind_with_inbox_limit("server:1", "server_actor", 4)
        .expect("bind server");
    let receiver = VmProcessId::from_raw_for_test(700);

    assert!(runtime
        .park_receive(server, receiver)
        .expect("park server receive"));
    let wakeups = runtime
        .send_to_with_wakeups(client, "server:1", b"one".to_vec())
        .expect("send one");
    assert_eq!(
        wakeups,
        vec![VmUdpWake::Receive {
            process: receiver,
            socket: server
        }]
    );
    runtime
        .send_to_with_wakeups(client, "server:1", b"two".to_vec())
        .expect("send two");

    let info = runtime.inspect_socket(server).expect("inspect server");
    assert_eq!(info.queued_packets, 2);
    assert_eq!(info.queued_bytes, 6);
    assert_eq!(info.waiting_receivers, 0);

    let first = runtime
        .receive_from(server)
        .expect("receive one")
        .expect("packet one");
    assert_eq!(first.source, "client:1");
    assert_eq!(first.bytes, b"one".to_vec());
    let second = runtime
        .receive_from(server)
        .expect("receive two")
        .expect("packet two");
    assert_eq!(second.bytes, b"two".to_vec());
    assert_eq!(runtime.receive_from(server).expect("empty"), None);
}

/// Verifies UDP backpressure, close behavior, and owner cleanup.
///
/// Inputs:
/// - A bounded server socket, an over-capacity packet attempt, and owner
///   cancellation.
///
/// Output:
/// - Test passes when inbox pressure rejects excess packets and closed sockets
///   cannot receive later sends.
///
/// Transformation:
/// - Locks VM resource cleanup semantics for datagram sockets before HTTP,
///   package, debugger, or ACME transports consume them.
#[test]
fn udp_runtime_enforces_backpressure_and_cancels_owner_sockets() {
    let mut runtime = VmUdpRuntime::new();
    assert_eq!(
        runtime
            .bind_with_inbox_limit("bad", "owner", 0)
            .expect_err("zero inbox should fail"),
        "VM UDP socket inbox limit must be greater than 0"
    );
    let client = runtime.bind("client:1", "client_actor").expect("client");
    let server = runtime
        .bind_with_inbox_limit("server:1", "server_actor", 1)
        .expect("server");

    runtime
        .send_to_with_wakeups(client, "server:1", b"one".to_vec())
        .expect("first send");
    assert_eq!(
        runtime
            .send_to_with_wakeups(client, "server:1", b"two".to_vec())
            .expect_err("server inbox is full"),
        "VM UDP socket `server:1` inbox is full"
    );

    let closed = runtime.cancel_owner_sockets("server_actor");
    assert_eq!(closed, vec![server]);
    assert_eq!(
        runtime
            .send_to_with_wakeups(client, "server:1", b"after close".to_vec())
            .expect_err("server address removed"),
        "VM UDP socket `server:1` was not found"
    );
    assert_eq!(
        runtime
            .receive_from(server)
            .expect_err("closed socket cannot receive"),
        "VM UDP socket is closed"
    );
}
