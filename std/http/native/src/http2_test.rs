use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::io;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::rt::{ReadBufCursor, Write};
use hyper::service::service_fn;

#[test]
fn http2_limits_bound_streams_flow_headers_frames_and_host_tasks() {
    assert_eq!(MAX_CONCURRENT_STREAMS, 256);
    assert_eq!(MAX_PENDING_RESET_STREAMS, 64);
    const { assert!(INITIAL_STREAM_WINDOW_BYTES < INITIAL_CONNECTION_WINDOW_BYTES) };
    assert_eq!(MAX_FRAME_BYTES, 16 * 1024);
    assert_eq!(MAX_HEADER_LIST_BYTES, 64 * 1024);
    assert_eq!(MAX_SEND_BUFFER_BYTES, 1024 * 1024);
    const { assert!(CONNECTION_TASK_CAPACITY >= MAX_CONCURRENT_STREAMS as usize) };
}

#[derive(Default)]
struct Pipe {
    bytes: VecDeque<u8>,
    closed: bool,
    reader: Option<Waker>,
}

struct MemoryIo {
    incoming: Rc<RefCell<Pipe>>,
    outgoing: Rc<RefCell<Pipe>>,
}

impl MemoryIo {
    fn pair() -> (Self, Self) {
        let first = Rc::new(RefCell::new(Pipe::default()));
        let second = Rc::new(RefCell::new(Pipe::default()));
        (
            Self {
                incoming: Rc::clone(&first),
                outgoing: Rc::clone(&second),
            },
            Self {
                incoming: second,
                outgoing: first,
            },
        )
    }
}

impl Read for MemoryIo {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        mut buffer: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let mut pipe = self.incoming.borrow_mut();
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if pipe.bytes.is_empty() && !pipe.closed {
            pipe.reader = Some(context.waker().clone());
            return Poll::Pending;
        }
        // Deliberately fragment protocol heads and data across transport reads.
        let count = buffer.remaining().min(pipe.bytes.len()).min(23);
        let bytes: Vec<_> = pipe.bytes.drain(..count).collect();
        buffer.put_slice(&bytes);
        Poll::Ready(Ok(()))
    }
}

impl Write for MemoryIo {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut pipe = self.outgoing.borrow_mut();
        if pipe.closed {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        let count = bytes.len().min(31);
        pipe.bytes.extend(&bytes[..count]);
        let reader = pipe.reader.take();
        drop(pipe);
        if let Some(reader) = reader {
            reader.wake();
        }
        Poll::Ready(Ok(count))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut pipe = self.outgoing.borrow_mut();
        pipe.closed = true;
        let reader = pipe.reader.take();
        drop(pipe);
        if let Some(reader) = reader {
            reader.wake();
        }
        Poll::Ready(Ok(()))
    }
}

#[test]
fn maintained_http2_executes_concurrent_streams_only_through_supplied_host() {
    let tasks = Rc::new(RefCell::new(Vec::<StreamTask>::new()));
    let client_tasks = Rc::downgrade(&tasks);
    let server_tasks = Rc::downgrade(&tasks);
    let (client_io, server_io) = MemoryIo::pair();
    let mut context = Context::from_waker(Waker::noop());
    let mut handshake = Box::pin(hyper::client::conn::http2::handshake(
        HostExecutor(move |task| client_tasks.upgrade().unwrap().borrow_mut().push(task)),
        client_io,
    ));
    let Poll::Ready(Ok((mut sender, client))) = handshake.as_mut().poll(&mut context) else {
        panic!("Hyper client handshake setup should not perform I/O");
    };
    let mut client = Box::pin(client);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let service_observed = Rc::clone(&observed);
    let service = service_fn(move |request: Request<Incoming>| {
        let observed = Rc::clone(&service_observed);
        async move {
            let path = request.uri().path().to_string();
            let bytes = request.into_body().collect().await.unwrap().to_bytes();
            observed.borrow_mut().push((path.clone(), bytes.clone()));
            Ok::<_, Infallible>(
                Response::builder()
                    .header("x-route", path)
                    .body(Full::new(bytes))
                    .unwrap(),
            )
        }
    });
    let mut server = Box::pin(serve_connection(server_io, service, move |task| {
        server_tasks.upgrade().unwrap().borrow_mut().push(task);
    }));
    let expected = [Bytes::from_static(b"first"), Bytes::from(vec![b'x'; 4096])];
    let mut responses = Vec::new();
    for (index, bytes) in expected.iter().enumerate() {
        let request = Request::builder()
            .method("POST")
            .uri(format!("https://localhost/{index}"))
            .body(Full::new(bytes.clone()))
            .unwrap();
        responses.push(sender.send_request(request));
    }
    assert!(observed.borrow().is_empty());
    for _ in 0..64 {
        assert!(server.as_mut().poll(&mut context).is_pending());
        assert!(client.as_mut().poll(&mut context).is_pending());
    }
    assert!(!tasks.borrow().is_empty());
    assert!(
        observed.borrow().is_empty(),
        "host has not polled any stream task"
    );
    let mut results = Box::pin(async move {
        let responses = futures_util::future::join_all(responses).await;
        let mut results = Vec::new();
        for response in responses {
            let response = response.unwrap();
            assert_eq!(response.version(), http::Version::HTTP_2);
            assert_eq!(response.status(), http::StatusCode::OK);
            let route = response.headers()["x-route"].to_str().unwrap().to_string();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            results.push((route, bytes));
        }
        results
    });
    for _ in 0..4096 {
        assert!(server.as_mut().poll(&mut context).is_pending());
        assert!(client.as_mut().poll(&mut context).is_pending());
        let ready = std::mem::take(&mut *tasks.borrow_mut());
        for mut task in ready {
            if task.as_mut().poll(&mut context).is_pending() {
                tasks.borrow_mut().push(task);
            }
        }
        if let Poll::Ready(actual) = results.as_mut().poll(&mut context) {
            assert_eq!(
                actual,
                vec![
                    ("/0".into(), expected[0].clone()),
                    ("/1".into(), expected[1].clone())
                ]
            );
            let mut observed = observed.borrow().clone();
            observed.sort_by(|first, second| first.0.cmp(&second.0));
            assert_eq!(observed, actual);
            return;
        }
    }
    panic!("HTTP/2 client/server did not complete within bounded host polling");
}
