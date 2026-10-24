//! Opening remains an admission operation until its source callback completes.

use super::*;

impl<I: CallbackInvocation<WebSocketEvent>> WebSocketCallbacks<I> {
    /// Awaits source initialization before the HTTP upgrade can be admitted.
    /// Dropping a polled opening future cancels its session after dropping the
    /// source invocation. No transport or room lease has been acquired yet.
    pub async fn open_suspendable(
        invocation: I,
        live: Session<I::Value>,
    ) -> Result<Self, ServiceError> {
        let mut session = Self { live, invocation };
        if !session.is_open() {
            return Err("error[serve.websocket.closed]: connection is closed".into());
        }
        let mut admission = Opening {
            session: &mut session,
            cancel_on_drop: true,
        };
        let result = admission
            .session
            .notify_suspendable(WebSocketEvent::Open)
            .await;
        admission.cancel_on_drop = false;
        drop(admission);
        match result {
            Ok(()) => Ok(session),
            Err(error) => Err(session.failed_open(error)),
        }
    }
}

struct Opening<'a, I: CallbackInvocation<WebSocketEvent>> {
    session: &'a mut WebSocketCallbacks<I>,
    cancel_on_drop: bool,
}

impl<I: CallbackInvocation<WebSocketEvent>> Drop for Opening<'_, I> {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            let _ = self.session.cancel("websocket opening task dropped".into());
        }
    }
}

#[cfg(test)]
#[path = "opening_test.rs"]
mod tests;
