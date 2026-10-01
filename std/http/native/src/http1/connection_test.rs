use super::*;
use crate::http_test_io::{complete, MemoryIo, State};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use std::convert::Infallible;
use std::io;
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};

fn input(bytes: &[u8]) -> Arc<Mutex<State>> {
    Arc::new(Mutex::new(State {
        incoming: bytes.to_vec().into(),
        read_limit: Some(13),
        ..State::default()
    }))
}

#[test]
fn fragmented_chunked_body_and_pipeline_are_driven_only_by_host() {
    let state = input(b"POST /first HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\nGET /last HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\nGET /ignored HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&observed);
    let service = service_fn(move |request: Request<Incoming>| {
        let calls = Arc::clone(&calls);
        async move {
            let path = request.uri().path().to_string();
            let body = request.into_body().collect().await.unwrap().to_bytes();
            calls.lock().unwrap().push((path.clone(), body.to_vec()));
            Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(path))))
        }
    });
    let connection = serve_connection(MemoryIo::<0>(Arc::clone(&state)), service);
    assert!(observed.lock().unwrap().is_empty());
    assert_eq!(state.lock().unwrap().reads, 0);
    complete(connection).unwrap();
    assert_eq!(
        *observed.lock().unwrap(),
        vec![
            ("/first".into(), b"abcde".to_vec()),
            ("/last".into(), Vec::new())
        ]
    );
    let state = state.lock().unwrap();
    let response = std::str::from_utf8(&state.outgoing).unwrap();
    assert_eq!(response.matches("HTTP/1.1 200 OK").count(), 2);
    assert!(response.ends_with("/last"));
    assert!(state.reads > 2);
    assert_eq!(state.drops, 1);
}

#[test]
fn malformed_framing_never_dispatches_handler() {
    for request in [
        b"BAD REQUEST\r\n\r\n".as_slice(),
        b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nxx",
        b"GET / HTTP/1.1\r\nHost: localhost\r\nBad Header: value\r\n\r\n",
        b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: nope\r\n\r\n",
    ] {
        let state = input(request);
        let calls = Arc::new(Mutex::new(0));
        let observed = Arc::clone(&calls);
        let service = service_fn(move |_: Request<Incoming>| {
            *observed.lock().unwrap() += 1;
            async { Ok::<_, Infallible>(Response::new(Full::new(Bytes::new()))) }
        });
        let error =
            complete(serve_connection(MemoryIo::<0>(Arc::clone(&state)), service)).unwrap_err();
        assert!(error.is_parse(), "{error}");
        assert_eq!(
            *calls.lock().unwrap(),
            0,
            "invalid request reached application code"
        );
        let state = state.lock().unwrap();
        assert!(state.outgoing.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
        assert_eq!(state.drops, 1);
    }
}

#[test]
fn handler_failure_closes_connection_without_a_success_response() {
    let state = input(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let service = service_fn(|_: Request<Incoming>| async {
        Err::<Response<Full<Bytes>>, _>(io::Error::other("handler failed"))
    });
    let error = complete(serve_connection(MemoryIo::<0>(Arc::clone(&state)), service)).unwrap_err();
    assert!(error.is_user(), "{error}");
    let state = state.lock().unwrap();
    assert!(state.outgoing.is_empty());
    assert_eq!(state.drops, 1);
}

#[test]
fn cancelling_a_parked_handler_releases_transport_and_handler_future() {
    struct DropNotice(Arc<Mutex<usize>>);
    impl Drop for DropNotice {
        fn drop(&mut self) {
            *self.0.lock().unwrap() += 1;
        }
    }
    let state = input(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let dropped = Arc::new(Mutex::new(0));
    let captures = Arc::clone(&dropped);
    let service = service_fn(move |_: Request<Incoming>| {
        let notice = DropNotice(Arc::clone(&captures));
        async move {
            std::future::pending::<()>().await;
            drop(notice);
            Ok::<_, Infallible>(Response::new(Full::new(Bytes::new())))
        }
    });
    let mut connection = Box::pin(serve_connection(MemoryIo::<0>(Arc::clone(&state)), service));
    let mut context = Context::from_waker(Waker::noop());
    assert!(connection.as_mut().poll(&mut context).is_pending());
    assert_eq!(*dropped.lock().unwrap(), 0);
    drop(connection);
    assert_eq!(*dropped.lock().unwrap(), 1);
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn truncated_request_head_and_transport_errors_propagate() {
    let state = input(b"GET / HTTP/1.1\r\nHost:");
    state.lock().unwrap().eof = true;
    let service = service_fn(|_: Request<Incoming>| async {
        Ok::<_, Infallible>(Response::new(Full::new(Bytes::new())))
    });
    assert!(complete(serve_connection(MemoryIo::<0>(Arc::clone(&state)), service)).is_err());
    assert_eq!(state.lock().unwrap().drops, 1);
    for flush in [false, true] {
        let state = input(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        if flush {
            state.lock().unwrap().flush_error = Some(io::ErrorKind::BrokenPipe);
        } else {
            state.lock().unwrap().write_error = Some(io::ErrorKind::BrokenPipe);
        }
        let service = service_fn(|_: Request<Incoming>| async {
            Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"hello"))))
        });
        assert!(complete(serve_connection(MemoryIo::<0>(Arc::clone(&state)), service)).is_err());
        assert_eq!(state.lock().unwrap().drops, 1);
    }
}
