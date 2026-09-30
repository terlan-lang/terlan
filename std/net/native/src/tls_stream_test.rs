use super::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Context, Waker};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};

#[derive(Clone)]
struct Transport(Rc<RefCell<Peer>>);

struct Peer {
    client: ClientConnection,
    pending: VecDeque<u8>,
    reads: VecDeque<io::ErrorKind>,
    writes: VecDeque<io::ErrorKind>,
    write_limit: usize,
    eof: bool,
    shutdown: bool,
    dropped: Rc<Cell<bool>>,
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.0.borrow().dropped.set(true);
    }
}

impl Read for Transport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut peer = self.0.borrow_mut();
        if let Some(error) = peer.reads.pop_front() {
            return Err(error.into());
        }
        let mut wire = Vec::new();
        peer.client.write_tls(&mut wire)?;
        peer.pending.extend(wire);
        if peer.pending.is_empty() && !peer.eof {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(peer.pending.len());
        for byte in &mut bytes[..count] {
            *byte = peer.pending.pop_front().unwrap();
        }
        Ok(count)
    }
}

impl Write for Transport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut peer = self.0.borrow_mut();
        if let Some(error) = peer.writes.pop_front() {
            return Err(error.into());
        }
        let limit = bytes.len().min(peer.write_limit);
        let count = peer
            .client
            .read_tls(&mut io::Cursor::new(&bytes[..limit]))?;
        peer.client
            .process_new_packets()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ShutdownWrite for Transport {
    fn shutdown_write(&mut self) -> io::Result<()> {
        self.0.borrow_mut().shutdown = true;
        Ok(())
    }
}

fn fixture() -> (Arc<ServerConfig>, Transport) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config = crate::tls::server_config(
        vec![cert.cert.der().clone()],
        crate::tls::parse_private_key(cert.key_pair.serialize_pem().as_bytes()).unwrap(),
    )
    .unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(cert.cert.der().clone()).unwrap();
    let client =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    let client =
        ClientConnection::new(Arc::new(client), ServerName::try_from("localhost").unwrap())
            .unwrap();
    let peer = Peer {
        client,
        pending: VecDeque::new(),
        reads: VecDeque::new(),
        writes: VecDeque::new(),
        write_limit: usize::MAX,
        eof: false,
        shutdown: false,
        dropped: Rc::new(Cell::new(false)),
    };
    (Arc::new(config), Transport(Rc::new(RefCell::new(peer))))
}

fn ready<T>(poll: Poll<io::Result<T>>) -> T {
    match poll {
        Poll::Ready(result) => result.unwrap(),
        Poll::Pending => panic!("unexpected pending I/O"),
    }
}

fn connected() -> (TlsStream<Transport>, Rc<RefCell<Peer>>) {
    let (config, transport) = fixture();
    let peer = Rc::clone(&transport.0);
    let mut server = TlsStream::new(transport, config).unwrap();
    ready(server.poll_handshake());
    assert!(!peer.borrow().client.is_handshaking());
    (server, peer)
}

#[test]
fn fragmented_handshake_retries_interrupts_and_uses_caller_deadline() {
    let (config, transport) = fixture();
    transport.0.borrow_mut().write_limit = 7;
    transport
        .0
        .borrow_mut()
        .reads
        .push_back(io::ErrorKind::Interrupted);
    transport
        .0
        .borrow_mut()
        .writes
        .push_back(io::ErrorKind::Interrupted);
    let mut handshake = Box::pin(TlsStream::handshake(
        transport,
        config,
        std::future::pending(),
    ));
    let mut context = Context::from_waker(Waker::noop());
    let mut server = ready(handshake.as_mut().poll(&mut context));
    assert_eq!(server.alpn_protocol(), None);
    assert_eq!(server.write(b"reply").unwrap(), 5);
    server.flush().unwrap();
    let mut reply = [0; 5];
    server
        .stream
        .0
        .borrow_mut()
        .client
        .reader()
        .read_exact(&mut reply)
        .unwrap();
    assert_eq!(&reply, b"reply");
}

#[test]
fn timeout_and_cancellation_drop_transport_without_background_work() {
    for expires in [false, true] {
        let (config, transport) = fixture();
        let dropped = Rc::clone(&transport.0.borrow().dropped);
        transport
            .0
            .borrow_mut()
            .reads
            .push_back(io::ErrorKind::WouldBlock);
        let timeout = std::future::poll_fn(move |_| {
            if expires {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        });
        let mut handshake = Box::pin(TlsStream::handshake(transport, config, timeout));
        let result = handshake
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        if expires {
            assert!(
                matches!(result, Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::TimedOut)
            );
        } else {
            assert!(result.is_pending());
            assert!(!dropped.get());
        }
        drop(handshake);
        assert!(dropped.get());
    }
}

#[test]
fn synchronous_and_polled_reads_drain_before_authenticated_eof_or_truncation() {
    for synchronous in [false, true] {
        for clean in [false, true] {
            let (mut server, peer) = connected();
            peer.borrow_mut()
                .client
                .writer()
                .write_all(b"payload")
                .unwrap();
            if clean {
                peer.borrow_mut().client.send_close_notify();
            }
            peer.borrow_mut().eof = true;
            let mut received = Vec::new();
            let outcome = loop {
                let mut bytes = [0; 2];
                let read = if synchronous {
                    server.read(&mut bytes)
                } else {
                    nonblocking(server.poll_read(&mut bytes))
                };
                match read {
                    Ok(0) => break Ok(()),
                    Ok(count) => received.extend_from_slice(&bytes[..count]),
                    Err(error) => break Err(error.kind()),
                }
            };
            assert_eq!(received, b"payload");
            assert_eq!(
                outcome,
                if clean {
                    Ok(())
                } else {
                    Err(io::ErrorKind::UnexpectedEof)
                }
            );
        }
    }
}

#[test]
fn backpressure_never_reports_a_successful_zero_byte_write() {
    let (mut server, peer) = connected();
    assert_eq!(server.read(&mut []).unwrap(), 0);
    assert_eq!(server.write(&[]).unwrap(), 0);
    assert_eq!(
        server.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    peer.borrow_mut()
        .writes
        .push_back(io::ErrorKind::WouldBlock);
    assert_eq!(server.write(b"first").unwrap(), 5);
    peer.borrow_mut()
        .writes
        .push_back(io::ErrorKind::WouldBlock);
    assert!(server.poll_write(b"second").is_pending());
    peer.borrow_mut()
        .writes
        .push_back(io::ErrorKind::WouldBlock);
    assert_eq!(
        server.flush().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    ready(server.poll_flush());
    assert_eq!(server.write(b"second").unwrap(), 6);
    let mut plaintext = [0; 11];
    peer.borrow_mut()
        .client
        .reader()
        .read_exact(&mut plaintext)
        .unwrap();
    assert_eq!(&plaintext, b"firstsecond");
    peer.borrow_mut()
        .writes
        .push_back(io::ErrorKind::WouldBlock);
    assert!(server.poll_shutdown().is_pending());
    assert!(!peer.borrow().shutdown);
    ready(server.poll_shutdown());
    assert!(peer.borrow().shutdown);
    assert_eq!(peer.borrow_mut().client.reader().read(&mut [0]).unwrap(), 0);
}

#[test]
fn transport_failures_and_malformed_tls_are_not_panics_or_eof() {
    for error in [io::ErrorKind::ConnectionReset, io::ErrorKind::UnexpectedEof] {
        let (mut server, peer) = connected();
        peer.borrow_mut().reads.push_back(error);
        assert_eq!(server.read(&mut [0]).unwrap_err().kind(), error);
    }
    for error in [io::ErrorKind::BrokenPipe, io::ErrorKind::WriteZero] {
        let (mut server, peer) = connected();
        if error == io::ErrorKind::WriteZero {
            peer.borrow_mut().write_limit = 0;
        } else {
            peer.borrow_mut().writes.push_back(error);
        }
        assert_eq!(server.write(b"reply").unwrap_err().kind(), error);
    }
    let (config, transport) = fixture();
    transport.0.borrow_mut().pending.extend(b"not a TLS record");
    let mut server = TlsStream::new(transport, config).unwrap();
    assert!(
        matches!(server.poll_handshake(), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::InvalidData)
    );
    let (config, transport) = fixture();
    let mut server = TlsStream::new(transport, config).unwrap();
    // Consume the initial client hello, then expose a raw EOF before its finish.
    let mut discard = Vec::new();
    server
        .stream
        .0
        .borrow_mut()
        .client
        .write_tls(&mut discard)
        .unwrap();
    server.stream.0.borrow_mut().eof = true;
    assert!(
        matches!(server.poll_handshake(), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn invalid_configuration_and_handshake_io_failures_release_transport() {
    let (mut config, transport) = fixture();
    Arc::get_mut(&mut config).unwrap().max_fragment_size = Some(1);
    let dropped = Rc::clone(&transport.0.borrow().dropped);
    let mut handshake = Box::pin(TlsStream::handshake(
        transport,
        config,
        std::future::pending(),
    ));
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(handshake.as_mut().poll(&mut context), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(dropped.get());

    for read_failure in [false, true] {
        let (config, transport) = fixture();
        if read_failure {
            transport
                .0
                .borrow_mut()
                .reads
                .push_back(io::ErrorKind::ConnectionReset);
        } else {
            transport
                .0
                .borrow_mut()
                .writes
                .push_back(io::ErrorKind::BrokenPipe);
        }
        let dropped = Rc::clone(&transport.0.borrow().dropped);
        let mut handshake = Box::pin(TlsStream::handshake(
            transport,
            config,
            std::future::pending(),
        ));
        assert!(
            matches!(handshake.as_mut().poll(&mut context), Poll::Ready(Err(error)) if error.kind() == if read_failure { io::ErrorKind::ConnectionReset } else { io::ErrorKind::BrokenPipe })
        );
        assert!(dropped.get());
    }
}

#[test]
fn pending_handshake_flush_can_resume_and_write_or_shutdown_flush_errors_propagate() {
    let (config, transport) = fixture();
    transport
        .0
        .borrow_mut()
        .writes
        .push_back(io::ErrorKind::WouldBlock);
    let mut server = TlsStream::new(transport, config).unwrap();
    assert!(server.poll_handshake().is_pending());
    ready(server.poll_handshake());

    for shutdown in [false, true] {
        let (mut server, peer) = connected();
        peer.borrow_mut()
            .writes
            .push_back(io::ErrorKind::WouldBlock);
        assert_eq!(server.write(b"pending").unwrap(), 7);
        peer.borrow_mut()
            .writes
            .push_back(io::ErrorKind::BrokenPipe);
        let error = if shutdown {
            nonblocking(server.poll_shutdown()).unwrap_err()
        } else {
            server.write(b"later").unwrap_err()
        };
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(!peer.borrow().shutdown);
    }
    let (mut server, _) = connected();
    server.connection.set_buffer_limit(Some(0));
    assert_eq!(
        server.write(b"no capacity").unwrap_err().kind(),
        io::ErrorKind::WriteZero
    );
}
