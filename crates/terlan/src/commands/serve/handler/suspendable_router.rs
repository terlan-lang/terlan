//! Execution and resource authority for the package's suspendable router.

use super::*;

pub(in crate::commands::serve) async fn execute_suspendable_router(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    package_root: &Path,
) -> Result<HandlerResponse, String> {
    let module = &matched.handler.module;
    let router = vm.execute_http_router(module, "router", &mut |_| {})?;
    native_http::router_execution::execute_suspendable(
        &router,
        vm_route_method(&matched.handler.method)?,
        &matched.handler.route,
        request.path(),
        &RouterResponseRuntime {
            vm,
            module,
            request: &request,
            package_root,
        },
    )
    .await
}

impl native_http::router_execution::AsyncHost for RouterResponseRuntime<'_> {
    type Value = ReplValue;
    type Response = HandlerResponse;

    async fn invoke(&self, callable: ReplValue, args: Vec<ReplValue>) -> Result<ReplValue, String> {
        self.vm
            .execute_suspendable_callable(self.module, &callable, args)
            .await
            .map_err(|error| error.to_string())
    }

    fn arity(&self, callable: &ReplValue) -> Option<usize> {
        self.vm.callable_arity(callable)
    }

    fn request(&self, params: &[(String, String)]) -> ReplValue {
        vm_request_descriptor(self.request, params)
    }

    fn scalar(value: terlan_runtime_abi::NativeValue) -> ReplValue {
        crate::runtime::vm::native_value::from_native(value)
    }

    fn admit(&self, value: ReplValue) -> Result<HandlerResponse, String> {
        decode_owned_response(value, self.package_root)
    }
}
