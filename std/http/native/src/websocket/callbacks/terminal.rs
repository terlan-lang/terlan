//! Graceful source cleanup remains owned by the closing connection task.

use super::*;

impl<I: CallbackInvocation<WebSocketEvent>> WebSocketCallbacks<I> {
    /// Ends admission and cancels prior work before awaiting graceful cleanup.
    /// A failed or dropped close never reopens the session or sends cancellation
    /// after the terminal callback has begun. Repeated close is a no-op.
    pub async fn close_suspendable(&mut self) -> Result<(), ServiceError> {
        if !self.is_open() {
            return Ok(());
        }
        self.live.close();
        self.invocation
            .cancel_pending("websocket transport closed".into())?;
        self.notify_suspendable(WebSocketEvent::Close).await
    }
}

#[cfg(test)]
#[path = "terminal_test.rs"]
mod tests;
