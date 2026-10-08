//! In-memory channel integration harness; WebSocket dispatch uses the live package loop.

use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

use std::future::{pending, ready, Future};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use terlan_http_native::websocket::{connection, hub::WebSocketHub};

use terlan_http_native::http1::{
    write_http1_stream_chunk, write_http1_stream_end, write_http1_stream_head,
};

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
    let hub = Arc::new(WebSocketHub::default());
    let mut future = Box::pin(connection::serve(
        ready(Ok(stream)),
        &mut session,
        &hub,
        "/test".into(),
        "/test".into(),
        pending,
    ));
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..1000 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return Ok(result?);
        }
    }
    Err("in-memory WebSocket connection did not complete".into())
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
    write_http1_stream_head(stream, &response, false).map_err(|failure| failure.message)?;
    write_http1_stream_chunk(stream, b": connected\n\n").map_err(|failure| failure.message)?;
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
            write_http1_stream_end(stream).map_err(|failure| failure.message)?;
            return stream
                .flush()
                .map_err(|error| format!("failed to flush VM SSE drain: {error}"));
        }

        thread::sleep(Duration::from_millis(keep_alive_ms));
        if let Err(error) = write_http1_stream_chunk(stream, b": keep-alive\n\n")
            .map_err(|failure| failure.message)
            .and_then(|_| {
                stream
                    .flush()
                    .map_err(|error| format!("failed to flush VM SSE keep-alive: {error}"))
            })
        {
            return cancel_sse_disconnect(&mut session, error);
        }
        if !session.is_waiting() {
            if let Err(error) = session.keep_alive() {
                let error = error.to_string();
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
    while let Some(frame) = session.flush_next_event() {
        write_http1_stream_chunk(writer, &frame).map_err(|failure| failure.message)?;
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
    session.cancel(reason).map(|_| ()).map_err(String::from)
}

#[cfg(test)]
#[path = "channel_transport_test.rs"]
#[cfg(test)]
mod channel_transport_test;
