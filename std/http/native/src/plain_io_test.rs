use super::*;
use hyper::rt::ReadBuf;
use std::collections::VecDeque;
use std::io::{Cursor, Read as _, Write as _};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Wake, Waker};

#[derive(Default)]
struct State {
    written: Vec<u8>,
    flushed: usize,
    closed: usize,
    dropped: usize,
}

struct Stream {
    state: Arc<Mutex<State>>,
    input: Cursor<Vec<u8>>,
    reads: VecDeque<io::Result<usize>>,
    writes: VecDeque<io::Result<usize>>,
    controls: VecDeque<io::Result<()>>,
}

impl Stream {
    fn new(input: &[u8]) -> Self {
        Self {
            state: Arc::default(),
            input: Cursor::new(input.to_vec()),
            reads: VecDeque::new(),
            writes: VecDeque::new(),
            controls: VecDeque::new(),
        }
    }
}

impl io::Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.reads
            .pop_front()
            .unwrap_or_else(|| self.input.read(bytes))
    }
}
impl io::Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.writes.pop_front().unwrap_or(Ok(bytes.len()))?;
        self.state
            .lock()
            .unwrap()
            .written
            .extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn write_vectored(&mut self, slices: &[IoSlice<'_>]) -> io::Result<usize> {
        self.write(
            &slices
                .iter()
                .flat_map(|slice| slice.iter().copied())
                .collect::<Vec<_>>(),
        )
    }
    fn flush(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().flushed += 1;
        self.controls.pop_front().unwrap_or(Ok(()))
    }
}
impl ShutdownWrite for Stream {
    fn shutdown_write(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().closed += 1;
        self.controls.pop_front().unwrap_or(Ok(()))
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        self.state.lock().unwrap().dropped += 1;
    }
}
#[derive(Default)]
struct Wakes(AtomicUsize);
impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn reads_preserve_bytes_partial_eof_and_zero_capacity() {
    let mut stream = PlainIo::new(Stream::new(b"abc"));
    let mut context = Context::from_waker(Waker::noop());
    let mut empty = [];
    let mut buffer = ReadBuf::new(&mut empty);
    assert!(Pin::new(&mut stream)
        .poll_read(&mut context, buffer.unfilled())
        .is_ready());
    for expected in [b"ab".as_slice(), b"c", b""] {
        let mut bytes = [0; 2];
        let mut buffer = ReadBuf::new(&mut bytes);
        assert!(matches!(
            Pin::new(&mut stream).poll_read(&mut context, buffer.unfilled()),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(buffer.filled(), expected);
    }
}

#[test]
fn untrusted_read_count_is_rejected_without_publishing_bytes() {
    let mut inner = Stream::new(b"");
    inner.reads = [Ok(usize::MAX)].into();
    let mut stream = PlainIo::new(inner);
    let mut context = Context::from_waker(Waker::noop());
    let mut bytes = [0; 2];
    let mut buffer = ReadBuf::new(&mut bytes);
    let Poll::Ready(Err(error)) = Pin::new(&mut stream).poll_read(&mut context, buffer.unfilled())
    else {
        panic!("invalid count accepted")
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(buffer.filled().is_empty());
}

#[test]
fn read_pressure_and_failures_preserve_cursor() {
    let mut inner = Stream::new(b"");
    inner.reads = [
        io::ErrorKind::Interrupted,
        io::ErrorKind::WouldBlock,
        io::ErrorKind::ConnectionReset,
    ]
    .map(|kind| Err(kind.into()))
    .into();
    let mut stream = PlainIo::new(inner);
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);
    let mut bytes = [0; 2];
    let mut buffer = ReadBuf::new(&mut bytes);
    assert!(Pin::new(&mut stream)
        .poll_read(&mut context, buffer.unfilled())
        .is_pending());
    assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
    assert!(
        matches!(Pin::new(&mut stream).poll_read(&mut context, buffer.unfilled()), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::ConnectionReset)
    );
    assert!(buffer.filled().is_empty());
}

#[test]
fn interrupted_operation_yields_after_bounded_work() {
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);
    let mut calls = 0;
    assert!(poll_io::<()>(&mut context, || {
        calls += 1;
        Err(io::ErrorKind::Interrupted.into())
    })
    .is_pending());
    assert_eq!(calls, 16);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
}

#[test]
fn every_async_operation_yields_then_preserves_pressure_failure_and_recovery() {
    fn outcomes() -> VecDeque<io::Result<usize>> {
        std::iter::repeat_n(io::ErrorKind::Interrupted, 16)
            .chain([io::ErrorKind::WouldBlock, io::ErrorKind::BrokenPipe])
            .map(|kind| Err(kind.into()))
            .chain([Ok(0)])
            .collect()
    }
    for lane in 0..5 {
        let wakes = Arc::new(Wakes::default());
        let waker = Waker::from(Arc::clone(&wakes));
        let mut context = Context::from_waker(&waker);
        let mut inner = Stream::new(b"");
        inner.reads = outcomes();
        inner.writes = outcomes();
        inner.controls = outcomes()
            .into_iter()
            .map(|result| result.map(|_| ()))
            .collect();
        let mut stream = PlainIo::new(inner);
        let mut poll = |context: &mut Context<'_>| {
            let mut bytes = [0; 1];
            let mut buffer = ReadBuf::new(&mut bytes);
            match lane {
                0 => Pin::new(&mut stream).poll_read(context, buffer.unfilled()),
                1 => Pin::new(&mut stream)
                    .poll_write(context, b"a")
                    .map(|result| result.map(|_| ())),
                2 => Pin::new(&mut stream)
                    .poll_write_vectored(context, &[IoSlice::new(b"a")])
                    .map(|result| result.map(|_| ())),
                3 => Pin::new(&mut stream).poll_flush(context),
                _ => Pin::new(&mut stream).poll_shutdown(context),
            }
        };
        assert!(poll(&mut context).is_pending(), "lane {lane}");
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert!(poll(&mut context).is_pending(), "lane {lane}");
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert!(
            matches!(poll(&mut context), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::BrokenPipe)
        );
        assert!(matches!(poll(&mut context), Poll::Ready(Ok(()))));
    }
}

#[test]
fn writes_preserve_partial_vectors_and_host_backpressure() {
    let mut inner = Stream::new(b"");
    inner.writes = [
        Err(io::ErrorKind::Interrupted.into()),
        Ok(2),
        Err(io::ErrorKind::WouldBlock.into()),
        Ok(3),
        Err(io::ErrorKind::BrokenPipe.into()),
    ]
    .into();
    let state = Arc::clone(&inner.state);
    let mut stream = PlainIo::new(inner);
    let mut context = Context::from_waker(Waker::noop());
    assert!(Write::is_write_vectored(&stream));
    assert!(matches!(
        Pin::new(&mut stream).poll_write(&mut context, b"abcd"),
        Poll::Ready(Ok(2))
    ));
    let slices = [IoSlice::new(b"cd"), IoSlice::new(b"ef")];
    assert!(Pin::new(&mut stream)
        .poll_write_vectored(&mut context, &slices)
        .is_pending());
    assert!(matches!(
        Pin::new(&mut stream).poll_write_vectored(&mut context, &slices),
        Poll::Ready(Ok(3))
    ));
    assert!(
        matches!(Pin::new(&mut stream).poll_write(&mut context, b"x"), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::BrokenPipe)
    );
    assert!(matches!(
        Pin::new(&mut stream).poll_flush(&mut context),
        Poll::Ready(Ok(()))
    ));
    assert!(matches!(
        Pin::new(&mut stream).poll_shutdown(&mut context),
        Poll::Ready(Ok(()))
    ));
    drop(stream);
    let state = state.lock().unwrap();
    assert_eq!(state.written, b"abcde");
    assert_eq!((state.flushed, state.closed, state.dropped), (1, 1, 1));
}

#[test]
fn synchronous_upgrade_io_delegates_without_an_executor() {
    let inner = Stream::new(b"request");
    let state = Arc::clone(&inner.state);
    let mut stream = PlainIo::new(inner);
    let mut bytes = [0; 7];
    stream.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"request");
    assert_eq!(stream.write(b"a").unwrap(), 1);
    assert_eq!(
        stream
            .write_vectored(&[IoSlice::new(b"b"), IoSlice::new(b"c")])
            .unwrap(),
        2
    );
    stream.flush().unwrap();
    drop(stream);
    let state = state.lock().unwrap();
    assert_eq!(state.written, b"abc");
    assert_eq!((state.flushed, state.dropped), (1, 1));
}

#[test]
fn maintained_hyper_parser_executes_handler_on_package_stream() {
    use std::future::Future;
    let mut inner =
        Stream::new(b"GET /sum HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    inner.writes = [Err(io::ErrorKind::WouldBlock.into())].into();
    let state = Arc::clone(&inner.state);
    let service = hyper::service::service_fn(
        |request: hyper::Request<hyper::body::Incoming>| async move {
            assert_eq!(request.uri().path(), "/sum");
            Ok::<_, std::convert::Infallible>(hyper::Response::new(http_body_util::Full::new(
                bytes::Bytes::from((2 + 3).to_string()),
            )))
        },
    );
    let mut connection = Box::pin(
        hyper::server::conn::http1::Builder::new()
            .half_close(true)
            .serve_connection(PlainIo::new(inner), service),
    );
    let mut context = Context::from_waker(Waker::noop());
    let mut completed = false;
    for _ in 0..32 {
        if let Poll::Ready(result) = connection.as_mut().poll(&mut context) {
            result.unwrap();
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "connection did not complete after readiness retries"
    );
    drop(connection);
    let state = state.lock().unwrap();
    let response = std::str::from_utf8(&state.written).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\n5"), "{response}");
    assert_eq!(state.dropped, 1);
}
