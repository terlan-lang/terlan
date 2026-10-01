//! Maintained HTTP/2 protocol configuration with host-supplied task execution.

use std::future::Future;
use std::pin::Pin;

use hyper::body::Incoming;
use hyper::rt::{Executor, Read, Write};
use hyper::server::conn::http2;
use hyper::service::Service;
use hyper::{Request, Response};

pub type StreamTask = Pin<Box<dyn Future<Output = ()> + 'static>>;

const MAX_CONCURRENT_STREAMS: u32 = 256;
const MAX_PENDING_RESET_STREAMS: usize = 64;
const INITIAL_STREAM_WINDOW_BYTES: u32 = 1024 * 1024;
const INITIAL_CONNECTION_WINDOW_BYTES: u32 = 4 * 1024 * 1024;
const MAX_FRAME_BYTES: u32 = 16 * 1024;
const MAX_HEADER_LIST_BYTES: u32 = 64 * 1024;
const MAX_SEND_BUFFER_BYTES: usize = 1024 * 1024;

/// Host admission includes bounded headroom for maintained protocol work.
pub const CONNECTION_TASK_CAPACITY: usize = MAX_CONCURRENT_STREAMS as usize + 16;

#[derive(Clone)]
struct HostExecutor<E>(E);

impl<F, E> Executor<F> for HostExecutor<E>
where
    F: Future<Output = ()> + 'static,
    E: Fn(StreamTask),
{
    fn execute(&self, future: F) {
        (self.0)(Box::pin(future));
    }
}

/// Builds a connection without spawning threads or selecting a runtime.
/// The host must drive admitted tasks and fail the connection on admission loss.
pub fn serve_connection<I, S, B, E>(
    io: I,
    service: S,
    execute: E,
) -> impl Future<Output = Result<(), hyper::Error>>
where
    I: Read + Write + Unpin + 'static,
    S: Service<Request<Incoming>, Response = Response<B>> + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: 'static,
    B: hyper::body::Body + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    E: Fn(StreamTask) + Clone + 'static,
{
    let mut builder = http2::Builder::new(HostExecutor(execute));
    builder
        .max_concurrent_streams(MAX_CONCURRENT_STREAMS)
        .max_pending_accept_reset_streams(MAX_PENDING_RESET_STREAMS)
        .initial_stream_window_size(INITIAL_STREAM_WINDOW_BYTES)
        .initial_connection_window_size(INITIAL_CONNECTION_WINDOW_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .max_header_list_size(MAX_HEADER_LIST_BYTES)
        .max_send_buf_size(MAX_SEND_BUFFER_BYTES);
    builder.serve_connection(io, service)
}

#[cfg(test)]
#[path = "http2_test.rs"]
mod tests;
