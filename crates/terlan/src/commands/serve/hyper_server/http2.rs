//! Connects package-owned HTTP/2 protocol work to the generic VM task group.

use std::future::Future;
use std::num::NonZeroUsize;

use hyper::body::Incoming;
use hyper::rt::{Read, Write};
use hyper::service::Service;
use hyper::{Request, Response};

use crate::runtime::vm::protocol_task_executor::task_group::{LocalTaskGroup, TaskGroupError};

pub(super) async fn serve_connection<I, S, B>(io: I, service: S) -> Result<(), String>
where
    I: Read + Write + Unpin + 'static,
    S: Service<Request<Incoming>, Response = Response<B>> + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: 'static,
    B: hyper::body::Body + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let capacity = NonZeroUsize::new(terlan_http_native::http2::CONNECTION_TASK_CAPACITY)
        .expect("HTTP package declares a positive connection capacity");
    drive_connection(capacity, |spawner| {
        terlan_http_native::http2::serve_connection(io, service, move |task| {
            // A rejection poisons the group, which fails and cancels all work
            // in the same owner poll. Hyper's Executor has no result channel.
            let _ = spawner.spawn(task);
        })
    })
    .await
}

async fn drive_connection<F, E>(
    capacity: NonZeroUsize,
    root: impl FnOnce(crate::runtime::vm::protocol_task_executor::task_group::LocalTaskSpawner) -> F,
) -> Result<(), String>
where
    F: Future<Output = Result<(), E>> + 'static,
    E: std::fmt::Display,
{
    LocalTaskGroup::new(capacity, root)
        .await
        .map_err(|error| match error {
            TaskGroupError::Root(error) => format!("Hyper HTTP/2 TLS connection failed: {error}"),
            TaskGroupError::CapacityExceeded => {
                "error[vm.http2.stream_pressure]: owner-local HTTP/2 task limit exceeded"
                    .to_string()
            }
        })
}

#[cfg(test)]
#[path = "http2_test.rs"]
mod tests;
