//! Bounded output over the maintained codec. The caller supplies runtime waits.

use std::future::{poll_fn, Future};
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::task::Poll;

use super::{ErrorKind, Server};

const MAX_MESSAGES_PER_TURN: usize = 32;

/// Flush accepted output without resubmitting it. False means the peer is closed.
/// `wait` must suspend on transport pressure; it owns readiness/timer policy.
pub async fn flush<S, W, F>(
    socket: &mut Server<S>,
    mut wait: W,
) -> Result<bool, crate::ServiceError>
where
    S: Read + Write,
    W: FnMut() -> F,
    F: Future<Output = ()>,
{
    loop {
        match socket.flush() {
            Ok(()) => return Ok(true),
            Err(error) => match error.kind() {
                ErrorKind::Closed | ErrorKind::Disconnected => return Ok(false),
                ErrorKind::WouldBlock | ErrorKind::Interrupted => wait().await,
                ErrorKind::Failed => {
                    return Err(format!("error[serve.websocket.transport]: {error}").into());
                }
            },
        }
    }
}

/// Flush each accepted message before removing another from the bounded queue.
/// A full turn yields once, allowing the caller to service inbound/control frames.
/// Dropping this future retains accepted bytes in the codec, not in a lost local
/// buffer; a subsequent drain flushes those bytes before taking new messages.
pub async fn drain<S, W, F>(
    socket: &mut Server<S>,
    outbound: &Receiver<String>,
    mut wait: W,
) -> Result<bool, crate::ServiceError>
where
    S: Read + Write,
    W: FnMut() -> F,
    F: Future<Output = ()>,
{
    if !flush(socket, &mut wait).await? {
        return Ok(false);
    }
    for _ in 0..MAX_MESSAGES_PER_TURN {
        let payload = match outbound.try_recv() {
            Ok(payload) => payload,
            Err(TryRecvError::Empty) => return Ok(true),
            Err(TryRecvError::Disconnected) => {
                return Err("error[serve.websocket.transport]: outbound hub disconnected".into());
            }
        };
        if let Err(error) = socket.write_text(payload) {
            match error.kind() {
                // The maintained codec retains bytes after either I/O failure.
                ErrorKind::WouldBlock | ErrorKind::Interrupted => {}
                ErrorKind::Closed | ErrorKind::Disconnected => return Ok(false),
                ErrorKind::Failed => {
                    return Err(format!("error[serve.websocket.transport]: {error}").into());
                }
            }
        }
        if !flush(socket, &mut wait).await? {
            return Ok(false);
        }
    }
    yield_turn().await;
    Ok(true)
}

pub(super) async fn yield_turn() {
    let mut yielded = false;
    poll_fn(|context| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}

#[cfg(test)]
#[path = "output_test.rs"]
mod tests;
