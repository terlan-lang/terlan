//! Suspendable direct-handler response materialization.

use std::path::Path;

use terlan_http_native as native_http;

use super::super::handler_cache::AotHandlerRuntime;
use super::request_materialization::direct_handler_arguments;
use super::{HandlerResponse, MatchedWebPackageHandler};

pub(in crate::commands::serve) async fn execute_suspendable_vm_handler_with_package_root_projected(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    projection: native_http::RequestFieldProjection,
    package_root: &Path,
) -> Result<HandlerResponse, String> {
    let result = if matched.handler.arity == 1
        && vm.uses_source_request(&matched.handler.module, &matched.handler.function, 1)
    {
        vm.execute_suspendable_projected_http_request(
            &matched.handler.module,
            &matched.handler.function,
            request.into_parts(),
            projection,
        )
        .await?
    } else {
        let args = direct_handler_arguments(vm, matched, request.into_parts(), projection)?;
        vm.execute_suspendable_http_response(
            &matched.handler.module,
            &matched.handler.function,
            args,
        )
        .await?
    };
    crate::commands::serve::handler::decode_owned_response(result, package_root)
}
