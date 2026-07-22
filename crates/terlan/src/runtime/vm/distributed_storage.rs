#![allow(dead_code)]

#[path = "distributed_storage/deadline.rs"]
pub(crate) mod deadline;

#[cfg(test)]
#[path = "distributed_storage_test.rs"]
mod distributed_storage_test;
include!("distributed_storage_part_001.rs");
include!("distributed_storage_part_002.rs");
