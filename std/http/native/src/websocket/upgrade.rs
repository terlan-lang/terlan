//! Owns admitted source sessions while Hyper finishes the HTTP/1 upgrade.

use std::cell::RefCell;
use std::future::Future;
use std::io::{Read, Write};
use std::sync::Arc;

use super::connection::{self, Callbacks};
use super::hub::WebSocketHub;
use crate::upgrade_io::UpgradeIo;
use crate::{HttpError, ServiceError};

pub struct UpgradeSlot<C: Callbacks>(RefCell<Option<PendingUpgrade<C>>>);

impl<C: Callbacks> Default for UpgradeSlot<C> {
    fn default() -> Self {
        Self(RefCell::new(None))
    }
}

impl<C: Callbacks> UpgradeSlot<C> {
    pub fn take(&self) -> Option<PendingUpgrade<C>> {
        self.0.borrow_mut().take()
    }

    /// A retained upgrade cannot be completed by an ordinary HTTP response.
    pub fn validate_response(&self, status: u16) -> Result<(), HttpError> {
        if status != 101 {
            if let Some(pending) = self.take() {
                return Err(pending.reject(HttpError::new(
                    "serve_http.upgrade_response",
                    "admitted WebSocket session requires HTTP status 101",
                    500,
                )));
            }
        }
        Ok(())
    }
}

/// Takes ownership even on rejection: an admitted source session must receive
/// cancellation if no transport can take responsibility for its lifecycle.
pub fn admit<C: Callbacks>(
    slot: Option<&UpgradeSlot<C>>,
    on_upgrade: Option<hyper::upgrade::OnUpgrade>,
    callbacks: C,
    route: String,
    target: String,
) -> Result<(), HttpError> {
    let pending = PendingUpgrade {
        on_upgrade,
        callbacks,
        route,
        target,
        cancel_on_drop: true,
    };
    let error = match slot {
        None => HttpError::new(
            "serve_http.upgrade_adapter_missing",
            "maintained async Hyper adapter is required for WebSocket",
            501,
        ),
        Some(slot) => {
            if pending.on_upgrade.is_none() {
                HttpError::new(
                    "serve_http.upgrade_state",
                    "Hyper upgrade future was not retained",
                    500,
                )
            } else {
                let mut slot = slot.0.borrow_mut();
                if slot.is_none() {
                    *slot = Some(pending);
                    return Ok(());
                }
                HttpError::new(
                    "serve_http.upgrade_state",
                    "connection already owns an upgrade",
                    500,
                )
            }
        }
    };
    Err(pending.reject(error))
}

pub struct PendingUpgrade<C: Callbacks> {
    on_upgrade: Option<hyper::upgrade::OnUpgrade>,
    callbacks: C,
    route: String,
    target: String,
    cancel_on_drop: bool,
}

impl<C: Callbacks> PendingUpgrade<C> {
    fn reject(mut self, error: HttpError) -> HttpError {
        self.cancel_on_drop = false;
        match self
            .callbacks
            .cancel(format!("error[{}]: {}", error.code(), error.message()))
        {
            Ok(()) => error,
            Err(cleanup) => HttpError::new(
                error.code(),
                format!("{}; cancellation failed: {cleanup}", error.message()),
                error.status(),
            ),
        }
    }

    /// Transfer lifecycle ownership to the maintained connection driver on first
    /// poll, never when merely constructing a future that might be discarded.
    pub async fn serve<I, W, F>(
        mut self,
        hub: &Arc<WebSocketHub>,
        wait: W,
    ) -> Result<(), ServiceError>
    where
        I: hyper::rt::Read + hyper::rt::Write + Read + Write + Unpin + Send + 'static,
        W: FnMut() -> F,
        F: Future<Output = ()>,
    {
        let on_upgrade = self.on_upgrade.take().expect("admitted Hyper upgrade");
        let io = async {
            let upgraded = on_upgrade.await.map_err(|error| {
                format!("error[serve.websocket.upgrade]: Hyper upgrade failed: {error}")
            })?;
            UpgradeIo::<I>::from_upgraded(upgraded).map_err(String::from)
        };
        self.cancel_on_drop = false;
        connection::serve(
            io,
            &mut self.callbacks,
            hub,
            std::mem::take(&mut self.route),
            std::mem::take(&mut self.target),
            wait,
        )
        .await
    }
}

impl<C: Callbacks> Drop for PendingUpgrade<C> {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            let _ = self
                .callbacks
                .cancel("websocket upgrade abandoned before transport handoff".into());
        }
    }
}

#[cfg(test)]
#[path = "upgrade_test.rs"]
mod tests;
