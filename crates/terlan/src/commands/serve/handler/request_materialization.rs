//! Server integration for package-owned HTTP ingress construction.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::vm::native_value::{from_native, replace_native};
use crate::runtime::vm::ReplValue;

pub(in crate::commands::serve) fn vm_request_descriptor_owned(
    request: RequestParts,
    projection: RequestFieldProjection,
) -> ReplValue {
    from_native(terlan_http_native::request_descriptor(request, projection))
}

pub(in crate::commands::serve) fn vm_source_request_tuple_owned(
    request: RequestParts,
) -> ReplValue {
    from_native(terlan_http_native::source_request_tuple(request))
}

pub(in crate::commands::serve) fn replace_vm_request_descriptor(
    value: &mut ReplValue,
    request: RequestParts,
    projection: RequestFieldProjection,
) {
    replace_native(
        value,
        terlan_http_native::request_descriptor(request, projection),
    );
}

#[cfg(test)]
#[path = "request_projection_test.rs"]
mod request_projection_test;
