#[path = "distributed_storage/deadline.rs"]
pub(crate) mod deadline;

#[cfg(test)]
#[path = "distributed_storage_test.rs"]
#[cfg(test)]
mod distributed_storage_test;

#[path = "distributed_storage/adapter.rs"]
mod adapter;
#[path = "distributed_storage/model.rs"]
mod model;

#[cfg(test)]
pub(crate) use adapter::*;
#[cfg(test)]
pub(crate) use model::*;
