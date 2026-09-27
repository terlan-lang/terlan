//! Server facade for the runtime-owned managed HTTP request projection.

pub(in crate::commands::serve) use crate::runtime::vm::http_request_value::{
    replace_vm_request_descriptor, vm_request_descriptor_owned, vm_source_request_tuple_owned,
};

#[cfg(test)]
#[path = "request_projection_test.rs"]
mod request_projection_test;
