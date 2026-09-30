use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{Read as _, Write as _};
use std::rc::Rc;
use std::sync::Arc;
use std::task::Waker;

use rustls::{ClientConfig, ClientConnection, RootCertStore};

#[derive(Default)]
struct Wire {
    incoming: VecDeque<u8>,
    outgoing: Vec<u8>,
    blocked: bool,
    eof: bool,
    shutdown: bool,
}

#[derive(Clone, Default)]
struct Transport(Rc<RefCell<Wire>>);

impl io::Read for Transport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut wire = self.0.borrow_mut();
        if wire.incoming.is_empty() && !wire.eof {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(wire.incoming.len());
        for byte in &mut bytes[..count] {
            *byte = wire.incoming.pop_front().unwrap();
        }
        Ok(count)
    }
}

impl io::Write for Transport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut wire = self.0.borrow_mut();
        if wire.blocked {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        wire.outgoing.extend_from_slice(bytes);
        Ok(bytes.len())
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

fn exchange(client: &mut ClientConnection, transport: &Transport) {
    let wire = std::mem::take(&mut transport.0.borrow_mut().outgoing);
    if !wire.is_empty() {
        client.read_tls(&mut io::Cursor::new(wire)).unwrap();
        client.process_new_packets().unwrap();
    }
    let mut wire = Vec::new();
    client.write_tls(&mut wire).unwrap();
    transport.0.borrow_mut().incoming.extend(wire);
}

fn connected(protocol: Option<&[u8]>) -> (TlsIo<Transport>, ClientConnection, Transport) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let mut server = terlan_net_native::tls::server_config(
        vec![cert.cert.der().clone()],
        terlan_net_native::tls::parse_private_key(cert.key_pair.serialize_pem().as_bytes())
            .unwrap(),
    )
    .unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(cert.cert.der().clone()).unwrap();
    let mut client =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    server.alpn_protocols = protocol.into_iter().map(<[u8]>::to_vec).collect();
    client.alpn_protocols = server.alpn_protocols.clone();
    let mut client =
        ClientConnection::new(Arc::new(client), "localhost".try_into().unwrap()).unwrap();
    let transport = Transport::default();
    let mut server = TlsIo::new(TlsStream::new(transport.clone(), Arc::new(server)).unwrap());
    for _ in 0..10 {
        exchange(&mut client, &transport);
        match server.0.poll_handshake() {
            Poll::Ready(result) => {
                result.unwrap();
                exchange(&mut client, &transport);
                assert!(!client.is_handshaking());
                return (server, client, transport);
            }
            Poll::Pending => {}
        }
    }
    panic!("handshake failed to make progress");
}

fn ready<T>(poll: Poll<io::Result<T>>) -> T {
    match poll {
        Poll::Ready(result) => result.unwrap(),
        Poll::Pending => panic!("unexpected pending I/O"),
    }
}

#[test]
fn negotiated_alpn_is_http_package_policy_not_vm_policy() {
    for (alpn, expected) in [
        (None, HttpProtocol::Http1),
        (Some(b"http/1.1".as_slice()), HttpProtocol::Http1),
        (Some(b"h2".as_slice()), HttpProtocol::Http2),
    ] {
        let (server, _, _) = connected(alpn);
        assert_eq!(server.negotiated_protocol().unwrap(), expected);
    }
    let (server, _, _) = connected(Some(b"unknown"));
    let error = server.negotiated_protocol().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("error[serve_tls.alpn]"));
}

#[test]
fn hyper_reads_split_plaintext_and_preserve_authenticated_closure() {
    for clean in [false, true] {
        let (mut server, mut client, transport) = connected(Some(b"h2"));
        let mut context = Context::from_waker(Waker::noop());
        let mut empty = hyper::rt::ReadBuf::new(&mut []);
        ready(Pin::new(&mut server).poll_read(&mut context, empty.unfilled()));
        let mut bytes = [0; 3];
        let mut buffer = hyper::rt::ReadBuf::new(&mut bytes);
        assert!(Pin::new(&mut server)
            .poll_read(&mut context, buffer.unfilled())
            .is_pending());
        client.writer().write_all(b"request").unwrap();
        if clean {
            client.send_close_notify();
        }
        exchange(&mut client, &transport);
        transport.0.borrow_mut().eof = true;
        let mut received = Vec::new();
        loop {
            let mut bytes = [0; 3];
            let mut buffer = hyper::rt::ReadBuf::new(&mut bytes);
            match Pin::new(&mut server).poll_read(&mut context, buffer.unfilled()) {
                Poll::Ready(Ok(())) if buffer.filled().is_empty() => {
                    assert!(clean);
                    break;
                }
                Poll::Ready(Ok(())) => received.extend_from_slice(buffer.filled()),
                Poll::Ready(Err(error)) => {
                    assert!(!clean);
                    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
                    break;
                }
                Poll::Pending => panic!("all wire bytes already delivered"),
            }
        }
        assert_eq!(received, b"request");
    }
}

#[test]
fn hyper_write_flush_shutdown_and_sync_upgrade_share_transport_backpressure() {
    let (mut server, mut client, transport) = connected(None);
    let mut context = Context::from_waker(Waker::noop());
    assert!(!hyper::rt::Write::is_write_vectored(&server));
    transport.0.borrow_mut().blocked = true;
    assert_eq!(
        ready(Pin::new(&mut server).poll_write(&mut context, b"one")),
        3
    );
    assert!(Pin::new(&mut server)
        .poll_write(&mut context, b"two")
        .is_pending());
    assert!(Pin::new(&mut server).poll_flush(&mut context).is_pending());
    assert!(Pin::new(&mut server)
        .poll_shutdown(&mut context)
        .is_pending());
    assert!(!transport.0.borrow().shutdown);
    transport.0.borrow_mut().blocked = false;
    ready(Pin::new(&mut server).poll_shutdown(&mut context));
    assert!(transport.0.borrow().shutdown);
    exchange(&mut client, &transport);
    let mut bytes = [0; 3];
    client.reader().read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"one");
    assert_eq!(client.reader().read(&mut bytes).unwrap(), 0);

    let (mut server, mut client, transport) = connected(None);
    assert_eq!(server.write(b"upgrade").unwrap(), 7);
    server.flush().unwrap();
    exchange(&mut client, &transport);
    let mut bytes = [0; 7];
    client.reader().read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"upgrade");
    client.writer().write_all(b"inbound").unwrap();
    exchange(&mut client, &transport);
    server.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"inbound");
    transport.0.borrow_mut().eof = true;
    assert_eq!(
        server.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}
