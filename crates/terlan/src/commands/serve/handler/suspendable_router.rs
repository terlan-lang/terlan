//! Serving adapter for the package-owned asynchronous callback chain.

use super::*;
use terlan_http_native::source_descriptor::HandlerPipeline;
use terlan_runtime_abi::NativeAdapterError;

pub(in crate::commands::serve) async fn execute_suspendable_router(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    package_root: &Path,
) -> Result<HandlerResponse, String> {
    let module = &matched.handler.module;
    let router = vm.execute_http_router(module, "router", &mut |_| {})?;
    let method = vm_route_method(&matched.handler.method)?;
    let VmHttpRouterOutcome::Matched(dispatch) = router.dispatch(method, request.path())? else {
        return Err("error[serve_router]: source router did not match request".into());
    };
    if dispatch.method != method || dispatch.route_pattern != matched.handler.route {
        return Err("error[serve_router]: manifest and source router routes disagree".into());
    }
    let VmHttpRouteTarget::Handler(handler) = &dispatch.target else {
        return Err("error[serve_router]: expected source HTTP handler".into());
    };
    let request = vm_request_descriptor(&request, &dispatch.route_params);
    let mut args = vec![request.clone()];
    if vm.callable_arity(handler).is_some_and(|arity| arity > 1) {
        args.extend(
            dispatch
                .route_params
                .iter()
                .map(|(name, value)| route_param_argument(&dispatch.route_pattern, name, value))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let pipeline = HandlerPipeline {
        handler,
        middleware: &dispatch.middleware,
        response_middleware: &dispatch.response_middleware,
        recovery: router.error_handler(),
    };
    let mut invoke = |callback, args| async move {
        vm.execute_suspendable_callable(module, &callback, args)
            .await
            .map_err(|error| NativeAdapterError::new("http.callback", error.to_string(), 0))
    };
    let error_value = |error: &NativeAdapterError| ReplValue::String(error.message().into());
    let response = pipeline
        .execute(request, args, &mut invoke, error_value)
        .await
        .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?;
    match HandlerResponse::from_owned_vm_response_with_package_root(response, package_root) {
        Ok(response) => Ok(response),
        Err(error) => {
            let recovered = pipeline
                .recover(
                    NativeAdapterError::new("http.response", error, 0),
                    &mut invoke,
                    &error_value,
                )
                .await
                .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?;
            HandlerResponse::from_owned_vm_response_with_package_root(recovered, package_root)
        }
    }
}
