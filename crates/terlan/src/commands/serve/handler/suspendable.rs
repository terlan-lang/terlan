//! Suspendable direct-handler response materialization.

use std::path::Path;

use crate::runtime::vm::VmHttpCallResult;
use crate::terlan_native::http as native_http;

use super::super::handler_cache::AotHandlerRuntime;
use super::request_materialization::vm_request_descriptor_owned;
use super::route::route_param_argument;
use super::{HandlerResponse, MatchedWebPackageHandler};

pub(in crate::commands::serve) async fn execute_suspendable_vm_handler_with_package_root_projected(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    projection: native_http::RequestFieldProjection,
    package_root: &Path,
) -> Result<HandlerResponse, String> {
    let result = if matched.handler.arity == 1 {
        vm.execute_suspendable_projected_http_request(
            &matched.handler.module,
            &matched.handler.function,
            request.into_parts(),
            projection,
        )
        .await?
    } else {
        let request = vm_request_descriptor_owned(request.into_parts(), projection);
        let mut args = vec![request];
        args.extend(
            matched
                .params
                .iter()
                .map(|(name, value)| route_param_argument(&matched.handler.route, name, value))
                .collect::<Result<Vec<_>, _>>()?,
        );
        if args.len() != matched.handler.arity {
            return Err(format!(
                "error[serve_handler]: handler `{}.{}/{}` received {} VM argument(s)",
                matched.handler.module,
                matched.handler.function,
                matched.handler.arity,
                args.len()
            ));
        }
        vm.execute_suspendable_http_response(
            &matched.handler.module,
            &matched.handler.function,
            args,
        )
        .await?
    };
    match result {
        VmHttpCallResult::Response(response) => HandlerResponse::from_aot_http_response(response),
        VmHttpCallResult::Generic(value) => {
            HandlerResponse::from_owned_vm_response_with_package_root(value, package_root)
        }
    }
}
