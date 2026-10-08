//! Server integration for package-owned HTTP ingress construction.

use crate::runtime::vm::native_value::{from_native, replace_native};
use crate::runtime::vm::ReplValue;
use terlan_http_native::{RequestFieldProjection, RequestParts};

/// All direct-handler paths use the same admitted request ABI and capture order.
pub(super) fn direct_handler_arguments(
    vm: &super::AotHandlerRuntime,
    matched: &super::MatchedWebPackageHandler,
    request: RequestParts,
    projection: RequestFieldProjection,
) -> Result<Vec<ReplValue>, String> {
    let handler = &matched.handler;
    let positional_params = if handler.arity == 1 {
        &[][..]
    } else {
        matched.params.as_slice()
    };
    let supplied = 1 + positional_params.len();
    if supplied != handler.arity {
        return Err(format!(
            "error[serve_handler]: handler `{}.{}/{}` received {} VM argument(s)",
            handler.module, handler.function, handler.arity, supplied,
        ));
    }
    let request = if vm.uses_source_request(&handler.module, &handler.function, handler.arity) {
        vm_request_descriptor_owned(request, projection)
    } else {
        vm_source_request_tuple_owned(request)
    };
    let mut args = Vec::with_capacity(supplied);
    args.push(request);
    for (name, value) in positional_params {
        args.push(super::route::route_param_argument(
            &handler.route,
            name,
            value,
        )?);
    }
    Ok(args)
}

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
