//! Hyper HTTP/2 stream futures driven on one VM protocol owner.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use hyper::body::Incoming;
use hyper::rt::{Executor, Read, Write};
use hyper::server::conn::http2;
use hyper::service::Service;
use hyper::{Request, Response};

type LocalTask = Pin<Box<dyn Future<Output = ()> + 'static>>;
type LocalTasks = Rc<RefCell<Vec<LocalTask>>>;
type HyperConnection = Pin<Box<dyn Future<Output = Result<(), hyper::Error>> + 'static>>;

const MAX_CONCURRENT_STREAMS: u32 = 256;
const MAX_PENDING_RESET_STREAMS: usize = 64;
const INITIAL_STREAM_WINDOW_BYTES: u32 = 1024 * 1024;
const INITIAL_CONNECTION_WINDOW_BYTES: u32 = 4 * 1024 * 1024;
const MAX_FRAME_BYTES: u32 = 16 * 1024;
const MAX_HEADER_LIST_BYTES: u32 = 64 * 1024;
const MAX_SEND_BUFFER_BYTES: usize = 1024 * 1024;
const MAX_OWNER_LOCAL_HTTP2_TASKS: usize = MAX_CONCURRENT_STREAMS as usize + 16;

#[derive(Clone)]
struct VmHttp2Executor {
    tasks: LocalTasks,
    overflowed: Rc<Cell<bool>>,
}

impl<F> Executor<F> for VmHttp2Executor
where
    F: Future<Output = ()> + 'static,
{
    fn execute(&self, future: F) {
        let mut tasks = self.tasks.borrow_mut();
        if tasks.len() >= MAX_OWNER_LOCAL_HTTP2_TASKS {
            self.overflowed.set(true);
            return;
        }
        tasks.push(Box::pin(future));
    }
}

/// Drives Hyper's connection future and every stream future on one VM owner.
struct VmHttp2Connection {
    connection: Option<HyperConnection>,
    tasks: LocalTasks,
    overflowed: Rc<Cell<bool>>,
}

impl Future for VmHttp2Connection {
    type Output = Result<(), String>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.overflowed.get() {
            return Poll::Ready(Err(
                "error[vm.http2.stream_pressure]: owner-local HTTP/2 task limit exceeded"
                    .to_string(),
            ));
        }
        if let Some(connection) = self.connection.as_mut() {
            match connection.as_mut().poll(context) {
                Poll::Ready(Ok(())) => self.connection = None,
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(format!("Hyper HTTP/2 TLS connection failed: {error}")))
                }
                Poll::Pending => {}
            }
        }

        let mut pending = Vec::new();
        let mut ready = std::mem::take(&mut *self.tasks.borrow_mut());
        for mut task in ready.drain(..) {
            if task.as_mut().poll(context).is_pending() {
                pending.push(task);
            }
        }
        self.tasks.borrow_mut().extend(pending);

        if self.connection.is_none() && self.tasks.borrow().is_empty() {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

pub(super) fn serve_connection<I, S, B>(
    io: I,
    service: S,
) -> impl Future<Output = Result<(), String>>
where
    I: Read + Write + Unpin + 'static,
    S: Service<Request<Incoming>, Response = Response<B>> + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: 'static,
    B: hyper::body::Body + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let tasks = Rc::new(RefCell::new(Vec::new()));
    let overflowed = Rc::new(Cell::new(false));
    let executor = VmHttp2Executor {
        tasks: Rc::clone(&tasks),
        overflowed: Rc::clone(&overflowed),
    };
    let mut builder = http2::Builder::new(executor);
    builder
        .max_concurrent_streams(MAX_CONCURRENT_STREAMS)
        .max_pending_accept_reset_streams(MAX_PENDING_RESET_STREAMS)
        .initial_stream_window_size(INITIAL_STREAM_WINDOW_BYTES)
        .initial_connection_window_size(INITIAL_CONNECTION_WINDOW_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .max_header_list_size(MAX_HEADER_LIST_BYTES)
        .max_send_buf_size(MAX_SEND_BUFFER_BYTES);
    let connection = builder.serve_connection(io, service);
    VmHttp2Connection {
        connection: Some(Box::pin(connection)),
        tasks,
        overflowed,
    }
}

#[cfg(test)]
#[path = "http2_test.rs"]
mod http2_test;
