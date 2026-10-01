use super::*;
use std::collections::VecDeque;
use std::io::{self, IoSlice, Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    registrations: usize,
    registration_failures: usize,
    written: Vec<Vec<Vec<u8>>>,
    replies: VecDeque<io::Result<usize>>,
    reads: VecDeque<io::Result<usize>>,
    dropped: usize,
    shutdowns: usize,
}

struct Stream(Arc<Mutex<State>>);

impl Source for Stream {
    fn register(&mut self, _: &mio::Registry, _: mio::Token, _: mio::Interest) -> io::Result<()> {
        panic!("readiness belongs to the host")
    }
    fn reregister(&mut self, _: &mio::Registry, _: mio::Token, _: mio::Interest) -> io::Result<()> {
        panic!("readiness belongs to the host")
    }
    fn deregister(&mut self, _: &mio::Registry) -> io::Result<()> {
        panic!("readiness belongs to the host")
    }
}

impl ReadinessStream for Stream {
    fn shutdown_write(&self) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.shutdowns += 1;
        if state.shutdowns == 1 {
            Ok(())
        } else {
            Err(io::ErrorKind::NotConnected.into())
        }
    }
}

#[test]
fn boxed_package_stream_half_close_delegates_success_and_failure() {
    let state = Arc::new(Mutex::new(State::default()));
    let boxed: Box<dyn ReadinessStream> = Box::new(Stream(Arc::clone(&state)));
    let stream = ReadyStream::new(boxed, ());
    stream.shutdown_write().unwrap();
    assert_eq!(
        stream.shutdown_write().unwrap_err().kind(),
        io::ErrorKind::NotConnected
    );
    drop(stream);
    let state = state.lock().unwrap();
    assert_eq!((state.shutdowns, state.dropped), (2, 1));
}

impl Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let read = self.0.lock().unwrap().reads.pop_front().unwrap()?;
        bytes[..read].fill(42);
        Ok(read)
    }
}

impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_vectored(&[IoSlice::new(bytes)])
    }

    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        state
            .written
            .push(buffers.iter().map(|buffer| buffer.to_vec()).collect());
        state.replies.pop_front().unwrap()
    }

    fn flush(&mut self) -> io::Result<()> {
        panic!("unbuffered facade must not flush the underlying socket")
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.0.lock().unwrap().dropped += 1;
    }
}

struct Interest(Arc<Mutex<State>>);

impl WriteInterest<Stream> for Interest {
    fn arm_writable(&mut self, _stream: &mut Stream) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.registrations += 1;
        if state.registration_failures > 0 {
            state.registration_failures -= 1;
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }
}

fn fixture(
    replies: impl IntoIterator<Item = io::Result<usize>>,
) -> (ReadyStream<Stream, Interest>, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State {
        replies: replies.into_iter().collect(),
        ..State::default()
    }));
    (
        ReadyStream::new(Stream(Arc::clone(&state)), Interest(Arc::clone(&state))),
        state,
    )
}

#[test]
fn partial_and_vectored_writes_preserve_results_without_retrying_or_registering() {
    let (mut stream, state) = fixture([Ok(2), Ok(1), Ok(0)]);
    assert_eq!(stream.write(b"abc").unwrap(), 2);
    assert_eq!(
        stream
            .write_vectored(&[IoSlice::new(b"de"), IoSlice::new(b"fg")])
            .unwrap(),
        1
    );
    assert_eq!(stream.write(b"").unwrap(), 0);
    stream.flush().unwrap();
    assert_eq!(state.lock().unwrap().registrations, 0);
    assert_eq!(
        state.lock().unwrap().written,
        vec![
            vec![b"abc".to_vec()],
            vec![b"de".to_vec(), b"fg".to_vec()],
            vec![vec![]]
        ]
    );
    drop(stream);
    assert_eq!(state.lock().unwrap().dropped, 1);
}

#[test]
fn write_pressure_arms_once_across_scalar_and_vectored_writes() {
    let stalled = || Err(io::Error::from(io::ErrorKind::WouldBlock));
    let (mut stream, state) = fixture([stalled(), stalled(), Ok(3), stalled()]);
    assert_eq!(
        stream.write(b"abc").unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        stream
            .write_vectored(&[IoSlice::new(b"abc")])
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(stream.write(b"abc").unwrap(), 3);
    assert_eq!(
        stream.write(b"def").unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state.lock().unwrap().registrations, 1);
    assert_eq!(state.lock().unwrap().written.len(), 4);
}

#[test]
fn failed_registration_can_retry_and_other_write_errors_do_not_register() {
    let kinds = [
        io::ErrorKind::Interrupted,
        io::ErrorKind::BrokenPipe,
        io::ErrorKind::WouldBlock,
        io::ErrorKind::WouldBlock,
    ];
    let (mut stream, state) = fixture(kinds.map(|kind| Err(kind.into())));
    state.lock().unwrap().registration_failures = 1;
    for kind in [io::ErrorKind::Interrupted, io::ErrorKind::BrokenPipe] {
        assert_eq!(stream.write(b"data").unwrap_err().kind(), kind);
    }
    assert_eq!(state.lock().unwrap().registrations, 0);
    assert_eq!(
        stream.write(b"data").unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        stream.write(b"data").unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state.lock().unwrap().registrations, 2);
    assert_eq!(state.lock().unwrap().written.len(), 4);
}

#[test]
fn reads_preserve_partial_eof_and_errors_without_changing_write_interest() {
    let (mut stream, state) = fixture([]);
    state.lock().unwrap().reads = [
        Err(io::ErrorKind::WouldBlock.into()),
        Err(io::ErrorKind::Interrupted.into()),
        Ok(2),
        Ok(0),
    ]
    .into();
    let mut bytes = [0; 4];
    assert_eq!(
        stream.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        stream.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(stream.read(&mut bytes).unwrap(), 2);
    assert_eq!(bytes, [42, 42, 0, 0]);
    assert_eq!(stream.read(&mut bytes).unwrap(), 0);
    assert_eq!(state.lock().unwrap().registrations, 0);
}
