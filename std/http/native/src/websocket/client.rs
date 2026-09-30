//! Maintained blocking client used by explicit host-side integration scenarios.
//! This is not the VM's asynchronous socket lane.

use std::net::TcpStream;
use std::time::Duration;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::WebSocket;

use crate::HttpError;

/// The maintained client's socket type; no parallel client protocol model.
pub type Client = WebSocket<MaybeTlsStream<TcpStream>>;

/// Connects using the maintained handshake and sets post-connect I/O deadlines.
/// The timeout does not cover DNS resolution or the opening handshake.
pub fn connect(url: &str, io_timeout: Duration) -> Result<Client, HttpError> {
    if io_timeout.is_zero() {
        return Err(HttpError::new(
            "http.websocket.client_timeout",
            "WebSocket I/O timeout must be greater than zero",
            400,
        ));
    }
    let (mut socket, _) = tungstenite::connect(url)
        .map_err(|error| HttpError::new("http.websocket.client_connect", error.to_string(), 502))?;
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream.set_read_timeout(Some(io_timeout)).map_err(|error| {
            HttpError::new("http.websocket.client_read_timeout", error.to_string(), 500)
        })?;
        stream
            .set_write_timeout(Some(io_timeout))
            .map_err(|error| {
                HttpError::new(
                    "http.websocket.client_write_timeout",
                    error.to_string(),
                    500,
                )
            })?;
    }
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_timeout_and_urls_fail_before_network_access() {
        let error = connect("ws://localhost/socket", Duration::ZERO).unwrap_err();
        assert_eq!(error.code(), "http.websocket.client_timeout");
        assert_eq!(error.status(), 400);
        for url in ["", "not a url", "http://localhost/socket", "ws://"] {
            let error = connect(url, Duration::from_secs(5)).unwrap_err();
            assert_eq!(error.code(), "http.websocket.client_connect");
            assert_eq!(error.status(), 502);
            assert!(!error.message().is_empty());
        }
    }
}
