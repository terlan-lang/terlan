//! Drives package connection futures and child tasks on one generic VM owner.

use std::future::Future;
use std::num::NonZeroUsize;

use crate::runtime::vm::protocol_task_executor::task_group::{LocalTaskGroup, TaskGroupError};

pub(super) async fn drive_connection<F, E>(
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
            TaskGroupError::Root(error) => error.to_string(),
            TaskGroupError::CapacityExceeded => {
                terlan_http_native::http2::task_capacity_error().to_string()
            }
        })
}

#[cfg(test)]
#[path = "connection_tasks_test.rs"]
mod tests;
