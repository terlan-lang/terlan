//! Code execution and authorized response access for the package router.

use super::*;

pub(super) struct RouterHost<'a, 'o> {
    pub(super) runtime: RouterResponseRuntime<'a>,
    pub(super) output: &'o mut dyn FnMut(&str),
}

impl native_http::router_execution::Host for RouterHost<'_, '_> {
    type Value = ReplValue;
    type Response = HandlerResponse;

    fn invoke(&mut self, callable: &ReplValue, args: Vec<ReplValue>) -> Result<ReplValue, String> {
        self.runtime
            .vm
            .execute_callable(self.runtime.module, callable, args, self.output)
            .map_err(String::from)
    }

    fn arity(&self, callable: &ReplValue) -> Option<usize> {
        self.runtime.vm.callable_arity(callable)
    }

    fn request(&self, params: &[(String, String)]) -> ReplValue {
        vm_request_descriptor(self.runtime.request, params)
    }

    fn scalar(value: terlan_runtime_abi::NativeValue) -> ReplValue {
        crate::runtime::vm::native_value::from_native(value)
    }

    fn admit(&mut self, value: ReplValue) -> Result<HandlerResponse, String> {
        decode_owned_response(value, self.runtime.package_root)
    }
}
