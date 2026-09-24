//! Actual VM-owned TLS streams distinguish authenticated closure from truncation.

use super::*;
use std::future::poll_fn;
use std::sync::mpsc;
use std::time::Instant;

#[test]
fn tls_plaintext_is_drained_before_authenticated_close_or_truncation() {
    let (config, client_config) = tls_pair();
    let (send, receive) = mpsc::channel();
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, _route| {
        let config = Arc::clone(&config);
        let send = send.clone();
        Box::pin(async move {
            let mut io = tls_io::VmTlsHyperIo::handshake(stream, config)
                .await
                .map_err(|error| error.to_string())?;
            let mut received = Vec::new();
            let outcome = loop {
                // Deliberately split one TLS record across many Hyper reads.
                let mut bytes = [0; 3];
                let mut buffer = hyper::rt::ReadBuf::new(&mut bytes);
                let result =
                    poll_fn(|context| Pin::new(&mut io).poll_read(context, buffer.unfilled()))
                        .await;
                match result {
                    Err(error) => break Err(error.kind()),
                    Ok(()) if buffer.filled().is_empty() => break Ok(()),
                    Ok(()) => received.extend_from_slice(buffer.filled()),
                }
            };
            send.send((received, outcome)).unwrap();
            Ok(())
        })
    });
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0).unwrap();
    let mut server =
        start_protocol_tasks_with_topology(listener, factory, VmSchedulerTopology::new(1).unwrap())
            .unwrap();
    for clean_close in [false, true] {
        let tcp = std::net::TcpStream::connect(server.local_addr()).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let connection = ClientConnection::new(
            Arc::clone(&client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        let mut client = StreamOwned::new(connection, tcp);
        client.write_all(b"complete plaintext before EOF").unwrap();
        if clean_close {
            client.conn.send_close_notify();
        }
        client.flush().unwrap();
        client.sock.shutdown(std::net::Shutdown::Write).unwrap();
        let (received, outcome) = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(received, b"complete plaintext before EOF");
        assert_eq!(
            outcome,
            if clean_close {
                Ok(())
            } else {
                Err(std::io::ErrorKind::UnexpectedEof)
            }
        );
    }
    server.stop().unwrap();
}

#[test]
fn silent_tls_peer_expires_on_the_vm_owner_timer() {
    let (config, _) = tls_pair();
    let (send, receive) = mpsc::channel();
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, _route| {
        let config = Arc::clone(&config);
        let send = send.clone();
        Box::pin(async move {
            let outcome = tls_io::VmTlsHyperIo::handshake_until(
                stream,
                config,
                Instant::now() + Duration::from_millis(50),
            )
            .await;
            send.send(outcome.err().map(|error| error.kind())).unwrap();
            Ok(())
        })
    });
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0).unwrap();
    let mut server =
        start_protocol_tasks_with_topology(listener, factory, VmSchedulerTopology::new(1).unwrap())
            .unwrap();
    let client = std::net::TcpStream::connect(server.local_addr()).unwrap();
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        Some(std::io::ErrorKind::TimedOut)
    );
    drop(client);
    server.stop().unwrap();
}
