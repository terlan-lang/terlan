//! Maintained WebSocket protocol state over caller-owned, possibly nonblocking I/O.
//!
//! This codec never waits, spawns tasks, or invokes application callbacks.

use std::io::{self, Read, Write};

use tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Role, WebSocketConfig};
use tungstenite::WebSocket;

pub use tungstenite::Message;
pub mod client;

/// Opening-handshake response metadata, independent of socket and actor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

/// Builds protocol-switch metadata after the host has admitted the upgrade request.
pub fn upgrade_response(sec_websocket_key: &str) -> Result<UpgradeResponse, crate::HttpError> {
    let key = sec_websocket_key.trim();
    if key.is_empty() {
        return Err(crate::HttpError::new(
            "http.websocket.key",
            "missing Sec-WebSocket-Key",
            400,
        ));
    }
    Ok(UpgradeResponse {
        status: 101,
        headers: vec![
            ("upgrade".into(), "websocket".into()),
            ("connection".into(), "Upgrade".into()),
            (
                "sec-websocket-accept".into(),
                tungstenite::handshake::derive_accept_key(key.as_bytes()),
            ),
        ],
    })
}

/// Transport progress categories that do not expose a protocol-library error layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    WouldBlock,
    Interrupted,
    Closed,
    Disconnected,
    Failed,
}

/// A maintained codec failure, retaining its original diagnostic and source.
#[derive(Debug)]
pub struct Error(tungstenite::Error);

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match &self.0 {
            tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
                ErrorKind::Closed
            }
            tungstenite::Error::Io(error) => match error.kind() {
                io::ErrorKind::WouldBlock => ErrorKind::WouldBlock,
                io::ErrorKind::Interrupted => ErrorKind::Interrupted,
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::NotConnected
                | io::ErrorKind::UnexpectedEof => ErrorKind::Disconnected,
                _ => ErrorKind::Failed,
            },
            _ => ErrorKind::Failed,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// Server-side protocol state. The caller owns readiness and cancellation.
pub struct Server<S> {
    socket: WebSocket<S>,
}

impl<S: Read + Write> Server<S> {
    /// Opens an already-upgraded stream with the endpoint's inbound byte limit.
    /// Both individual frames and reassembled messages are bounded by this limit.
    pub fn new(stream: S, max_inbound_bytes: usize) -> Self {
        let config = WebSocketConfig::default()
            .max_frame_size(Some(max_inbound_bytes))
            .max_message_size(Some(max_inbound_bytes));
        Self {
            socket: WebSocket::from_raw_socket(stream, Role::Server, Some(config)),
        }
    }

    /// Reads a maintained message, preserving partial input across WouldBlock.
    pub fn read(&mut self) -> Result<Message, Error> {
        self.socket.read().map_err(Error)
    }

    /// Queues text without waiting. WouldBlock means the message was accepted;
    /// resume with `flush`, never by resubmitting the same text.
    pub fn write_text(&mut self, text: String) -> Result<(), Error> {
        self.socket.write(Message::Text(text.into())).map_err(Error)
    }

    /// Flushes queued output, including automatic pong and close replies.
    pub fn flush(&mut self) -> Result<(), Error> {
        self.socket.flush().map_err(Error)
    }

    /// Queues the existing unsupported-payload close policy without waiting.
    pub fn close_unsupported(&mut self) -> Result<(), Error> {
        self.socket
            .close(Some(CloseFrame {
                code: CloseCode::Unsupported,
                reason: "unsupported channel payload".into(),
            }))
            .map_err(Error)
    }
}

#[cfg(test)]
#[path = "websocket_test.rs"]
mod tests;
