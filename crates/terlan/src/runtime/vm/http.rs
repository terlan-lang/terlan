#![allow(dead_code)]

#[path = "http/deadline.rs"]
mod deadline;
#[cfg(test)]
#[path = "http/deadline_test.rs"]
mod deadline_test;
#[cfg(test)]
#[path = "http_support_bundle_test.rs"]
mod http_support_bundle_test;
#[cfg(test)]
#[path = "http_test.rs"]
mod http_test;
#[path = "http/lifecycle.rs"]
mod lifecycle;
#[path = "http/lifecycle_hooks.rs"]
mod lifecycle_hooks;
#[cfg(test)]
#[path = "http/lifecycle_test.rs"]
mod lifecycle_test;
#[path = "http/overload.rs"]
mod overload;
#[cfg(test)]
#[path = "http/overload_test.rs"]
mod overload_test;
#[path = "http/request_read.rs"]
pub(crate) mod request_read;
#[path = "http/request_resources.rs"]
mod request_resources;
#[cfg(test)]
#[path = "http/request_resources_test.rs"]
mod request_resources_test;
#[path = "http/response_memory.rs"]
mod response_memory;
#[cfg(test)]
#[path = "http/response_memory_test.rs"]
mod response_memory_test;
#[path = "http/response_wire.rs"]
pub(crate) mod response_wire;
#[cfg(test)]
#[path = "http/response_wire_test.rs"]
mod response_wire_test;
#[path = "http/soak.rs"]
pub(crate) mod soak;
#[cfg(test)]
#[path = "http/soak_stability_test.rs"]
mod soak_stability_test;
#[cfg(test)]
#[path = "http/soak_test.rs"]
mod soak_test;
#[path = "http/template_response.rs"]
mod template_response;
#[cfg(test)]
#[path = "http/template_response_target_test.rs"]
mod template_response_target_test;
#[cfg(test)]
#[path = "http/template_response_test.rs"]
mod template_response_test;
#[cfg(test)]
#[path = "http/test_support_test.rs"]
pub(crate) mod test_support;
include!("http_part_001.rs");
include!("http_part_002.rs");
include!("http_part_003.rs");
