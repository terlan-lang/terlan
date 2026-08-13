//! Production socket pumps for admitted HTTP channel sessions.

use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, Message, Role, WebSocket};
use tungstenite::Error as WebSocketError;

use crate::runtime::vm::http::{
    write_http1_stream_chunk, write_http1_stream_end, write_http1_stream_head,
};
use crate::runtime::vm::websocket::VmWebSocketFrame;

use super::handler::VmHttpChannelTransport;
use super::VmStreamHttp1Exchange;

/// Fallback heartbeat used when an SSE endpoint does not declare an interval.
#[cfg(test)]
const DEFAULT_SSE_KEEP_ALIVE_MS: u64 = 15_000;

/// Writes a finite response or transfers the socket to an admitted channel pump.
#[cfg(test)]
pub(super) fn serve_vm_stream_http1_exchange<S>(
    stream: &mut S,
    exchange: VmStreamHttp1Exchange,
) -> Result<(), String>
where
    S: Read + Write,
{
    match exchange.channel {
        None => write_buffered_response(stream, &exchange.response),
        Some(VmHttpChannelTransport::WebSocket(session)) => {
            write_buffered_response(stream, &exchange.response)?;
            pump_websocket(stream, session)
        }
        Some(VmHttpChannelTransport::Sse(session)) => pump_sse(stream, session),
    }
}

/// Writes and flushes one finite HTTP response before connection completion.
#[cfg(test)]
fn write_buffered_response(writer: &mut dyn Write, response: &[u8]) -> Result<(), String> {
    writer
        .write_all(response)
        .map_err(|error| format!("failed to write VM plain HTTP response: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("failed to flush VM plain HTTP response: {error}"))
}

/// Pumps maintained WebSocket messages into one bounded generated callback session.
#[cfg(test)]
fn pump_websocket<S>(
    stream: &mut S,
    mut session: super::handler::AotWebSocketCallbackSession,
) -> Result<(), String>
where
    S: Read + Write,
{
    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, None);
    notify_websocket_writable(&mut session)?;
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let result = session
                    .enqueue_inbound(VmWebSocketFrame::Text(text.to_string()))
                    .and_then(|()| drain_websocket_inbound(&mut session));
                if let Err(error) = result {
                    close_websocket_after_error(&mut socket, &mut session, &error);
                    return Err(error);
                }
            }
            Ok(Message::Ping(_)) => {
                socket.flush().map_err(render_websocket_transport_error)?;
                notify_websocket_writable(&mut session)?;
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                let close = session.close().map(|_| ());
                let flush = flush_websocket_close(&mut socket);
                return close.and(flush);
            }
            Ok(Message::Binary(_)) => {
                let error =
                    "error[serve.websocket.binary]: endpoint rejects binary payloads".to_string();
                close_websocket_after_error(&mut socket, &mut session, &error);
                return Err(error);
            }
            Ok(Message::Frame(_)) => {
                let error = "error[serve.websocket.frame]: raw frame escaped maintained decoding"
                    .to_string();
                close_websocket_after_error(&mut socket, &mut session, &error);
                return Err(error);
            }
            Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => {
                return session.close().map(|_| ());
            }
            Err(error) => {
                let reason = render_websocket_transport_error(error);
                session.cancel(reason.clone()).map(|_| ())?;
                return Err(reason);
            }
        }
    }
}

/// Drains every admitted text frame through callback entry or typed wake delivery.
#[cfg(test)]
fn drain_websocket_inbound(
    session: &mut super::handler::AotWebSocketCallbackSession,
) -> Result<(), String> {
    while session.dispatch_next_inbound()? {}
    Ok(())
}

/// Notifies generated code when transport write capacity is available and idle.
#[cfg(test)]
fn notify_websocket_writable(
    session: &mut super::handler::AotWebSocketCallbackSession,
) -> Result<(), String> {
    if !session.is_waiting() {
        session.writable()?;
    }
    Ok(())
}

/// Cancels generated work and asks tungstenite to emit a policy close frame.
#[cfg(test)]
fn close_websocket_after_error<S>(
    socket: &mut WebSocket<S>,
    session: &mut super::handler::AotWebSocketCallbackSession,
    reason: &str,
) where
    S: Read + Write,
{
    let _ = session.cancel(reason.to_string());
    let _ = socket.close(Some(CloseFrame {
        code: CloseCode::Unsupported,
        reason: "unsupported channel payload".into(),
    }));
    let _ = socket.flush();
}

/// Renders one stable production WebSocket transport diagnostic.
#[cfg(test)]
fn render_websocket_transport_error(error: WebSocketError) -> String {
    format!("error[serve.websocket.transport]: {error}")
}

/// Flushes a close reply while accepting tungstenite's terminal closed state.
#[cfg(test)]
fn flush_websocket_close<S>(socket: &mut WebSocket<S>) -> Result<(), String>
where
    S: Read + Write,
{
    match socket.flush() {
        Ok(()) | Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => Ok(()),
        Err(error) => Err(render_websocket_transport_error(error)),
    }
}

/// Pumps chunked SSE frames and heartbeats until disconnect or graceful drain.
#[cfg(test)]
fn pump_sse<S>(
    stream: &mut S,
    mut session: super::handler::AotSseCallbackSession,
) -> Result<(), String>
where
    S: Read + Write,
{
    let response = http::Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "text/event-stream")
        .header(http::header::CACHE_CONTROL, "no-cache")
        .header("x-content-type-options", "nosniff")
        .body(())
        .map_err(|error| format!("failed to build VM SSE stream response: {error}"))?;
    write_http1_stream_head(stream, &response, false)?;
    write_http1_stream_chunk(stream, b": connected\n\n")?;
    stream
        .flush()
        .map_err(|error| format!("failed to flush VM SSE stream head: {error}"))?;

    let keep_alive_ms = session
        .plan()
        .keep_alive_ms()
        .unwrap_or(DEFAULT_SSE_KEEP_ALIVE_MS);
    loop {
        if let Err(error) = flush_sse_events(stream, &mut session) {
            return cancel_sse_disconnect(&mut session, error);
        }
        if !session.is_open() {
            write_http1_stream_end(stream)?;
            return stream
                .flush()
                .map_err(|error| format!("failed to flush VM SSE drain: {error}"));
        }

        thread::sleep(Duration::from_millis(keep_alive_ms));
        if let Err(error) = write_http1_stream_chunk(stream, b": keep-alive\n\n").and_then(|_| {
            stream
                .flush()
                .map_err(|error| format!("failed to flush VM SSE keep-alive: {error}"))
        }) {
            return cancel_sse_disconnect(&mut session, error);
        }
        if !session.is_waiting() {
            if let Err(error) = session.keep_alive() {
                session.cancel(error.clone()).map(|_| ())?;
                return Err(error);
            }
        }
    }
}

/// Flushes all queued SSE application events through VM chunk framing.
#[cfg(test)]
fn flush_sse_events(
    writer: &mut dyn Write,
    session: &mut super::handler::AotSseCallbackSession,
) -> Result<(), String> {
    let mut wrote_event = false;
    while let Some(frame) = session.flush_next_event()? {
        write_http1_stream_chunk(writer, &frame)?;
        wrote_event = true;
    }
    if !wrote_event {
        return Ok(());
    }
    writer
        .flush()
        .map_err(|error| format!("failed to flush VM SSE events: {error}"))
}

/// Converts a terminal SSE write failure into generated cancellation cleanup.
#[cfg(test)]
fn cancel_sse_disconnect(
    session: &mut super::handler::AotSseCallbackSession,
    reason: String,
) -> Result<(), String> {
    session.cancel(reason).map(|_| ())
}

#[cfg(test)]
#[path = "channel_transport_test.rs"]
#[cfg(test)]
mod channel_transport_test;
