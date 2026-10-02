use super::*;
use crate::http_test_io::{complete, MemoryIo, State};
use crate::websocket::Message;
use std::cell::Cell;
use std::future::{pending, ready};
use std::io::{self, Cursor};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::task::{Context, Wake, Waker};
use tungstenite::protocol::{Role, WebSocket};

// Exercise every fault scenario through the same runtime-wait specialization.
// LLVM's generic-function summary does not union coverage across specializations.
async fn drain<W, F>(
    socket: &mut Server<MemoryIo<0>>,
    receiver: &Receiver<String>,
    mut wait: W,
) -> Result<bool, String>
where
    W: FnMut() -> F,
    F: Future<Output = ()> + 'static,
{
    let mut wait = || -> Pin<Box<dyn Future<Output = ()>>> { Box::pin(wait()) };
    let wait: &mut dyn FnMut() -> Pin<Box<dyn Future<Output = ()>>> = &mut wait;
    super::drain(socket, receiver, wait)
        .await
        .map_err(String::from)
}

fn socket() -> (Server<MemoryIo<0>>, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    (Server::new(MemoryIo(state.clone()), 1024), state)
}

fn messages(state: &Arc<Mutex<State>>, expected: &[String]) {
    let wire = state.lock().unwrap().outgoing.clone();
    let mut client = WebSocket::from_raw_socket(Cursor::new(wire), Role::Client, None);
    for text in expected {
        assert_eq!(client.read().unwrap(), Message::text(text.clone()));
    }
    assert!(client.read().is_err(), "duplicate or unexpected frame");
}

#[test]
fn partial_write_cancellation_retains_accepted_bytes_and_leaves_queue_bounded() {
    for budget in [0, 1, 2, 7, 13] {
        let (mut socket, state) = socket();
        state.lock().unwrap().write_budget = Some(budget);
        let (sender, receiver) = mpsc::sync_channel(2);
        let first = "first message with partial writes".to_string();
        sender.send(first.clone()).unwrap();
        sender.send("second".into()).unwrap();
        let mut future = Box::pin(drain(&mut socket, &receiver, pending));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        sender.try_send("third".into()).unwrap();
        assert!(matches!(
            sender.try_send("overflow".into()),
            Err(mpsc::TrySendError::Full(_))
        ));
        assert_eq!(state.lock().unwrap().outgoing.len(), budget);
        drop(future);
        state.lock().unwrap().write_budget = None;
        assert!(complete(drain(&mut socket, &receiver, || ready(()))).unwrap());
        messages(&state, &[first, "second".into(), "third".into()]);
    }
}

#[test]
fn retries_of_large_accepted_messages_never_duplicate_or_spin() {
    for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::Interrupted] {
        let (mut socket, state) = socket();
        state.lock().unwrap().write_error = Some(kind);
        let (sender, receiver) = mpsc::sync_channel(1);
        let payload = "x".repeat(128 * 1024);
        sender.send(payload.clone()).unwrap();
        let waits = Cell::new(0);
        let mut future = Box::pin(drain(&mut socket, &receiver, || {
            waits.set(waits.get() + 1);
            pending::<()>()
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(waits.get(), 1, "transport retry must suspend");
        assert!(state.lock().unwrap().outgoing.is_empty());
        drop(future);
        state.lock().unwrap().write_error = None;
        assert!(complete(drain(&mut socket, &receiver, || ready(()))).unwrap());
        messages(&state, &[payload]);
    }
}

#[test]
fn flush_resumes_after_runtime_wait_and_precedes_new_admission() {
    for kind in [io::ErrorKind::Interrupted, io::ErrorKind::WouldBlock] {
        let (mut socket, state) = socket();
        state.lock().unwrap().flush_error = Some(kind);
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send("pending".into()).unwrap();
        let mut future = Box::pin(drain(&mut socket, &receiver, pending));
        assert!(future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert!(matches!(
            sender.try_send("overflow".into()),
            Err(mpsc::TrySendError::Full(_))
        ));
        drop(future);
        let waits = Cell::new(0);
        assert!(complete(drain(&mut socket, &receiver, || {
            waits.set(waits.get() + 1);
            state.lock().unwrap().flush_error = None;
            ready(())
        }))
        .unwrap());
        assert_eq!(waits.get(), 1);
        messages(&state, &["pending".into()]);
    }
}

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn saturated_output_yields_after_a_bounded_turn_and_preserves_order() {
    let (mut socket, state) = socket();
    let (sender, receiver) = mpsc::sync_channel(MAX_MESSAGES_PER_TURN + 1);
    let payloads: Vec<_> = (0..=MAX_MESSAGES_PER_TURN).map(|n| n.to_string()).collect();
    for payload in &payloads {
        sender.send(payload.clone()).unwrap();
    }
    let wakes = Arc::new(WakeCount::default());
    let waker = Waker::from(wakes.clone());
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(drain(
        &mut socket,
        &receiver,
        || -> std::future::Ready<()> { panic!("no pressure") },
    ));
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
    messages(&state, &payloads[..MAX_MESSAGES_PER_TURN]);
    assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(Ok(true)));
    drop(future);
    assert!(complete(drain(&mut socket, &receiver, || ready(()))).unwrap());
    messages(&state, &payloads);
}

#[test]
fn empty_and_disconnected_queues_are_distinct_and_empty_text_is_delivered() {
    let (mut socket, state) = socket();
    let (sender, receiver) = mpsc::sync_channel(1);
    assert!(complete(drain(&mut socket, &receiver, || ready(()))).unwrap());
    sender.send(String::new()).unwrap();
    drop(sender);
    assert_eq!(
        complete(drain(&mut socket, &receiver, || ready(()))),
        Err("error[serve.websocket.transport]: outbound hub disconnected".into())
    );
    messages(&state, &[String::new()]);
}

#[test]
fn peer_disconnect_and_fatal_errors_stop_without_dequeuing_later_messages() {
    for length in [1, 128 * 1024] {
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::PermissionDenied] {
            let (mut socket, state) = socket();
            state.lock().unwrap().write_error = Some(kind);
            let (sender, receiver) = mpsc::sync_channel(2);
            sender.send("x".repeat(length)).unwrap();
            sender.send("later".into()).unwrap();
            let result = complete(drain(&mut socket, &receiver, || ready(())));
            if kind == io::ErrorKind::BrokenPipe {
                assert_eq!(result, Ok(false));
            } else {
                assert!(result.unwrap_err().contains("serve.websocket.transport"));
            }
            assert_eq!(receiver.try_recv().unwrap(), "later");
            assert!(state.lock().unwrap().outgoing.is_empty());
        }
    }
    for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::PermissionDenied] {
        let (mut socket, state) = socket();
        state.lock().unwrap().flush_error = Some(kind);
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send("untouched".into()).unwrap();
        let result = complete(drain(&mut socket, &receiver, || ready(())));
        assert_eq!(result == Ok(false), kind == io::ErrorKind::BrokenPipe);
        if kind == io::ErrorKind::PermissionDenied {
            assert!(result.unwrap_err().contains("serve.websocket.transport"));
        }
        assert_eq!(receiver.try_recv().unwrap(), "untouched");
    }
}

#[test]
fn closed_codec_does_not_consume_new_output() {
    let (mut socket, state) = socket();
    let mut client = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Client, None);
    client.close(None).unwrap();
    state
        .lock()
        .unwrap()
        .incoming
        .extend(client.into_inner().into_inner());
    assert_eq!(socket.read().unwrap(), Message::Close(None));
    assert!(!complete(flush(&mut socket, || ready(()))).unwrap());
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.send("late".into()).unwrap();
    assert!(!complete(drain(&mut socket, &receiver, || ready(()))).unwrap());
    assert_eq!(receiver.try_recv().unwrap(), "late");
    drop(socket);
    assert_eq!(state.lock().unwrap().drops, 1);
}
