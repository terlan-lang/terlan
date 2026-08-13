use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::{Condvar, Mutex};
use std::time::Instant;

use super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable};
use super::support_bundle::{
    VmSupportBundleReplayMetadata, VmSupportBundleReplayRecorder, VmSupportBundleReplayResource,
    VmSupportBundleReplayResourceKind,
};
use super::tcp::{VmTcpListener, VmTcpListenerInfo, VmTcpRuntime, VmTcpStream};
use super::tls::{VmTlsRuntime, VmTlsTcpPoll, VmTlsTcpServerStream, VmTlsTransportMode};
pub(crate) use request_read::read_http1_request;
pub(crate) use response_wire::write_http1_response;
pub(crate) use response_wire::{
    write_http1_stream_chunk, write_http1_stream_end, write_http1_stream_head,
};
pub(crate) use template_response::{render_http_template_response, VmHttpTemplateResponse};

#[cfg(test)]
#[path = "http/deadline_test.rs"]
#[cfg(test)]
mod deadline_test;
#[cfg(test)]
#[path = "http_support_bundle_test.rs"]
#[cfg(test)]
mod http_support_bundle_test;
#[cfg(test)]
#[path = "http_test.rs"]
#[cfg(test)]
mod http_test;
#[cfg(test)]
#[path = "http/lifecycle_test.rs"]
#[cfg(test)]
mod lifecycle_test;
#[cfg(test)]
#[path = "http/overload_test.rs"]
#[cfg(test)]
mod overload_test;
#[path = "http/request_read.rs"]
pub(crate) mod request_read;
#[cfg(test)]
#[path = "http/request_resources_test.rs"]
#[cfg(test)]
mod request_resources_test;
#[cfg(test)]
#[path = "http/response_memory_test.rs"]
#[cfg(test)]
mod response_memory_test;
#[path = "http/response_wire.rs"]
pub(crate) mod response_wire;
#[cfg(test)]
#[path = "http/response_wire_test.rs"]
#[cfg(test)]
mod response_wire_test;
#[path = "http/soak.rs"]
pub(crate) mod soak;
#[cfg(test)]
#[path = "http/soak_stability_test.rs"]
#[cfg(test)]
mod soak_stability_test;
#[cfg(test)]
#[path = "http/soak_test.rs"]
#[cfg(test)]
mod soak_test;
#[path = "http/template_response.rs"]
mod template_response;
#[cfg(test)]
#[path = "http/template_response_target_test.rs"]
#[cfg(test)]
mod template_response_target_test;
#[cfg(test)]
#[path = "http/template_response_test.rs"]
#[cfg(test)]
mod template_response_test;
#[cfg(test)]
#[path = "http/test_support_test.rs"]
pub(crate) mod test_support;

#[path = "http/protocol.rs"]
mod protocol;

pub(crate) use protocol::*;
pub(crate) use protocol::{deadline, request_resources};
pub(crate) use protocol::{lifecycle, overload};
