use super::*;
use std::io::{Read as _, Write as _};
use std::task::Waker;

use crate::tls_test_support::{connected, exchange};

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
        transport.0.lock().unwrap().eof = true;
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
    transport.0.lock().unwrap().blocked = true;
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
    assert!(!transport.0.lock().unwrap().shutdown);
    transport.0.lock().unwrap().blocked = false;
    ready(Pin::new(&mut server).poll_shutdown(&mut context));
    assert!(transport.0.lock().unwrap().shutdown);
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
    transport.0.lock().unwrap().eof = true;
    assert_eq!(
        server.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}
