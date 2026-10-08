//! Live protocol dispatch; source callbacks and executor waits are supplied by the host.

use std::future::Future;
use std::io::{Read, Write};
use std::sync::Arc;

use crate::channel_plan::WebSocketEndpointPlan;
use crate::source_descriptor::{PairContext, PairedTransition};

use super::hub::{AdmissionCallbacks, WebSocketHub, WebSocketHubLease};
use super::{output, ErrorKind, Message, Server, Utf8Bytes};

/// Invokes opaque source callbacks without exposing compiler or VM value types.
pub trait Callbacks: AdmissionCallbacks {
    type Callback: Clone;

    fn plan(&self) -> &WebSocketEndpointPlan<Self::Callback>;
    fn identity(
        &mut self,
        target: String,
    ) -> impl Future<Output = Result<Option<(String, i64)>, crate::ServiceError>>;
    fn waiting(&mut self) -> Result<String, crate::ServiceError>;
    fn peer_left(&mut self) -> Result<String, crate::ServiceError>;
    fn enqueue(&mut self, text: Utf8Bytes) -> Result<(), crate::ServiceError>;
    fn next_inbound(&mut self) -> Result<(bool, Option<String>), crate::ServiceError>;
    fn next_paired(
        &mut self,
        context: Option<PairContext>,
    ) -> Result<Option<PairedTransition>, crate::ServiceError>;
    /// Notify source code only when no callback is parked.
    fn writable(&mut self) -> Result<(), crate::ServiceError>;
    fn close(&mut self) -> Result<(), crate::ServiceError>;
    fn cancel(&mut self, reason: String) -> Result<(), crate::ServiceError>;
}

/// Acquires an upgraded transport, then drives the maintained codec. Once polled,
/// failed/cancelled acquisition also cancels the admitted source session. Runtime
/// waits must suspend on pressure; no executor, timer, or task is created here.
pub async fn serve<S, C, W, F>(
    io: impl Future<Output = Result<S, String>>,
    callbacks: &mut C,
    hub: &Arc<WebSocketHub>,
    route: String,
    target: String,
    wait: W,
) -> Result<(), crate::ServiceError>
where
    S: Read + Write,
    C: Callbacks,
    W: FnMut() -> F,
    F: Future<Output = ()>,
{
    let mut live = Lifecycle {
        callbacks,
        terminal: false,
    };
    let result = async {
        let io = io.await?;
        drive(io, &mut live, hub, route, target, wait).await
    }
    .await;
    if let Err(error) = &result {
        if !live.terminal {
            live.terminal = true;
            if let Err(cleanup) = live.callbacks.cancel(error.to_string()) {
                return Err(format!("{error}; cancellation failed: {cleanup}").into());
            }
        }
    }
    result
}

struct Lifecycle<'a, C: Callbacks> {
    callbacks: &'a mut C,
    terminal: bool,
}

impl<C: Callbacks> Lifecycle<'_, C> {
    fn close(&mut self) -> Result<(), crate::ServiceError> {
        self.terminal = true;
        self.callbacks.close()
    }
}

impl<C: Callbacks> Drop for Lifecycle<'_, C> {
    fn drop(&mut self) {
        if !self.terminal {
            let _ = self
                .callbacks
                .cancel("websocket connection task dropped".into());
        }
    }
}

async fn drive<S, C, W, F>(
    io: S,
    live: &mut Lifecycle<'_, C>,
    hub: &Arc<WebSocketHub>,
    route: String,
    target: String,
    mut wait: W,
) -> Result<(), crate::ServiceError>
where
    S: Read + Write,
    C: Callbacks,
    W: FnMut() -> F,
    F: Future<Output = ()>,
{
    let mut socket = Server::new(io, live.callbacks.plan().max_frame_bytes());
    let capacity = live.callbacks.plan().max_pending_frames();
    let mut pairing = live.callbacks.plan().pairing().cloned();
    let mut identity = None;
    if let Some(pairing) = &mut pairing {
        if pairing.restoration.is_some() {
            identity = live.callbacks.identity(target.clone()).await?;
            pairing.waiting = live.callbacks.waiting()?;
            pairing.peer_left = live.callbacks.peer_left()?;
        }
    }
    let mut lease = pairing
        .as_ref()
        .map(|pairing| hub.join(route, target, capacity, pairing, identity))
        .transpose()?;
    if let Some(lease) = &mut lease {
        lease.dispatch_admission(live.callbacks)?;
    }
    live.callbacks.writable()?;
    let mut received = 0;
    loop {
        if let Some(lease) = &lease {
            if !output::drain(&mut socket, &lease.outbound, &mut wait).await? {
                return live.close();
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                live.callbacks.enqueue(text)?;
                dispatch_inbound(live.callbacks, lease.as_ref())?;
            }
            Ok(Message::Ping(_)) => {
                if !output::flush(&mut socket, &mut wait).await? {
                    return live.close();
                }
                live.callbacks.writable()?;
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                let callback = live.close();
                let flush = output::flush(&mut socket, &mut wait).await.map(|_| ());
                return callback.and(flush);
            }
            Ok(Message::Binary(_)) => {
                return Err(
                    "error[serve.websocket.binary]: endpoint rejects binary payloads".into(),
                )
            }
            Ok(Message::Frame(_)) => {
                return Err(
                    "error[serve.websocket.frame]: raw frame escaped maintained decoding".into(),
                )
            }
            Err(error) => match error.kind() {
                ErrorKind::WouldBlock | ErrorKind::Interrupted => {
                    wait().await;
                }
                ErrorKind::Closed | ErrorKind::Disconnected => return live.close(),
                ErrorKind::Failed => {
                    return Err(format!("error[serve.websocket.transport]: {error}").into())
                }
            },
        }
        received += 1;
        if received == 32 {
            received = 0;
            output::yield_turn().await;
        }
    }
}

fn dispatch_inbound<C: Callbacks>(
    callbacks: &mut C,
    lease: Option<&WebSocketHubLease>,
) -> Result<(), crate::ServiceError> {
    if let Some(lease) = lease {
        loop {
            let mut dispatched = false;
            lease.transition(|context| {
                let state = context
                    .as_ref()
                    .map(|(state, _, _, _)| state.clone())
                    .unwrap_or_default();
                match callbacks.next_paired(context)? {
                    Some(transition) => {
                        dispatched = true;
                        Ok(transition)
                    }
                    None => Ok((state, None, None)),
                }
            })?;
            if !dispatched {
                return Ok(());
            }
        }
    }
    loop {
        let (dispatched, _) = callbacks.next_inbound()?;
        if !dispatched {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "connection_test.rs"]
mod tests;
