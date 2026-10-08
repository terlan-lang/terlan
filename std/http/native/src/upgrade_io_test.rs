use super::*;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use http_body_util::Full;
use hyper::service::service_fn;

use crate::http_test_io::{complete, MemoryIo, State};

fn upgrade<const KIND: u8>(prefix: &[u8]) -> (hyper::upgrade::Upgraded, Arc<Mutex<State>>) {
    let mut incoming = b"GET /stream HTTP/1.1\r\nHost: localhost\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n".to_vec();
    incoming.extend_from_slice(prefix);
    let state = Arc::new(Mutex::new(State {
        incoming: incoming.into(),
        ..State::default()
    }));
    let pending = Arc::new(Mutex::new(None));
    let service_pending = Arc::clone(&pending);
    let service = service_fn(move |mut request| {
        *service_pending.lock().unwrap() = Some(hyper::upgrade::on(&mut request));
        async {
            Ok::<_, Infallible>(
                http::Response::builder()
                    .status(101)
                    .header("connection", "upgrade")
                    .header("upgrade", "websocket")
                    .body(Full::new(Bytes::new()))
                    .unwrap(),
            )
        }
    });
    complete(crate::http1::serve_connection(
        MemoryIo::<KIND>(Arc::clone(&state)),
        service,
    ))
    .unwrap();
    let pending = pending.lock().unwrap().take().unwrap();
    let upgraded = complete(pending).unwrap();
    assert!(state.lock().unwrap().outgoing.starts_with(b"HTTP/1.1 101"));
    (upgraded, state)
}

type Adapter = UpgradeIo<MemoryIo<1>>;

fn assert_transfer<const KIND: u8>(prefix: &[u8]) {
    let (upgraded, state) = upgrade::<KIND>(prefix);
    let mut io = UpgradeIo::<MemoryIo<KIND>>::from_upgraded(upgraded).unwrap();
    let reads = state.lock().unwrap().reads;
    assert_eq!(io.read(&mut []).unwrap(), 0);
    let mut actual = Vec::new();
    for _ in 0..prefix.len() {
        let mut byte = [0];
        assert_eq!(io.read(&mut byte).unwrap(), 1);
        actual.extend(byte);
    }
    assert_eq!(actual, prefix);
    if !prefix.is_empty() {
        assert_eq!(state.lock().unwrap().reads, reads);
    }
    let mut buffer = [0; 8];
    assert_eq!(
        io.read(&mut buffer).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    state.lock().unwrap().incoming.extend(b"tail");
    assert_eq!(io.read(&mut buffer).unwrap(), 4);
    assert_eq!(&buffer[..4], b"tail");
    state.lock().unwrap().eof = true;
    assert_eq!(io.read(&mut buffer).unwrap(), 0);

    state.lock().unwrap().outgoing.clear();
    io.write_all(b"response after upgrade").unwrap();
    io.flush().unwrap();
    assert_eq!(state.lock().unwrap().outgoing, b"response after upgrade");
    for error in [
        io::ErrorKind::WouldBlock,
        io::ErrorKind::Interrupted,
        io::ErrorKind::BrokenPipe,
    ] {
        state.lock().unwrap().write_error = Some(error);
        assert_eq!(io.write(b"x").unwrap_err().kind(), error);
        state.lock().unwrap().flush_error = Some(error);
        assert_eq!(io.flush().unwrap_err().kind(), error);
    }
    assert_eq!(state.lock().unwrap().drops, 0);
    drop(io);
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn both_admitted_transports_preserve_read_ahead_partial_io_errors_and_lifetime() {
    for prefix in [b"".as_slice(), b"x", b"\0binary\xff\r\n"] {
        assert_transfer::<1>(prefix);
        assert_transfer::<2>(prefix);
    }
}

#[test]
fn cancelling_before_read_ahead_is_consumed_releases_transport() {
    let (upgraded, state) = upgrade::<1>(b"unread");
    let io = Adapter::from_upgraded(upgraded).unwrap();
    drop(io);
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn foreign_transport_is_rejected_and_released() {
    let (upgraded, state) = upgrade::<3>(b"unread");
    let error = Adapter::from_upgraded(upgraded).err().unwrap();
    assert_eq!(error.code(), "serve.websocket.upgrade");
    assert_eq!(
        error.to_string(),
        "error[serve.websocket.upgrade]: Hyper returned an unexpected transport type"
    );
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn maintained_websocket_decodes_masked_frame_buffered_by_hyper() {
    let frame = [0x81, 0x82, 1, 2, 3, 4, b'o' ^ 1, b'k' ^ 2];
    let (upgraded, state) = upgrade::<1>(&frame);
    let io = Adapter::from_upgraded(upgraded).unwrap();
    let reads = state.lock().unwrap().reads;
    let mut socket = crate::websocket::Server::new(io, 1024);
    assert_eq!(
        socket.read().unwrap(),
        crate::websocket::Message::Text("ok".into())
    );
    assert_eq!(state.lock().unwrap().reads, reads);
}

#[test]
fn finite_stream_uses_maintained_http1_chunk_framing_over_partial_writes() {
    let state = Arc::new(Mutex::new(State {
        incoming: b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            .iter()
            .copied()
            .collect(),
        ..State::default()
    }));
    let service = service_fn(|_| async {
        let mut response = http::Response::new(Bytes::new());
        response.extensions_mut().insert(
            crate::HttpResponseChunks::new(vec![Bytes::from_static(b"abcde")], 2, 1).unwrap(),
        );
        Ok::<_, Infallible>(crate::response_body::ResponseBody::from_response(response))
    });
    complete(
        hyper::server::conn::http1::Builder::new()
            .serve_connection(MemoryIo::<1>(Arc::clone(&state)), service),
    )
    .unwrap();
    let state = state.lock().unwrap();
    let wire = String::from_utf8(state.outgoing.clone()).unwrap();
    assert!(wire.contains("transfer-encoding: chunked\r\n"), "{wire}");
    assert!(
        wire.ends_with("\r\n\r\n2\r\nab\r\n2\r\ncd\r\n1\r\ne\r\n0\r\n\r\n"),
        "{wire}"
    );
    assert_eq!(state.drops, 1);
}
