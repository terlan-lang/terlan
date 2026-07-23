//! Typed fixed-owner HTTP response calls with generic fallback.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::vm::protocol_task_executor::with_current_protocol_resource;
use crate::runtime::vm::{ReplValue, VmHttpCallResult};

use super::{finish_immediate_step, AotHandlerRuntime, LocalImmediateShard};
use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;

impl AotHandlerRuntime {
    pub(in crate::commands::serve) fn execute_immediate_http_response(
        &self,
        module: &str,
        function: &str,
        args: Vec<ReplValue>,
        _output: &mut dyn FnMut(&str),
    ) -> Result<VmHttpCallResult, String> {
        self.require_module(module)?;
        if let Some(value) = with_current_protocol_resource(
            self.generation.identity,
            |scheduler| {
                LocalImmediateShard::new(
                    self.generation.image.spawn_shard_on_scheduler(scheduler)?,
                    module,
                    function,
                    args.len(),
                )
            },
            |local: &mut LocalImmediateShard| local.call_http_response(module, function, &args),
        )? {
            return Ok(value);
        }
        finish_immediate_step(self.begin_request_invocation(module, function, args)?)
            .map(VmHttpCallResult::Generic)
    }

    #[allow(dead_code)] // Retained for single-argument HTTP response call sites.
    pub(in crate::commands::serve) fn execute_immediate_http_response_one(
        &self,
        module: &str,
        function: &str,
        argument: ReplValue,
        _output: &mut dyn FnMut(&str),
    ) -> Result<VmHttpCallResult, String> {
        self.require_module(module)?;
        let mut argument = Some(argument);
        if let Some(value) = with_current_protocol_resource(
            self.generation.identity,
            |scheduler| {
                LocalImmediateShard::new(
                    self.generation.image.spawn_shard_on_scheduler(scheduler)?,
                    module,
                    function,
                    1,
                )
            },
            |local: &mut LocalImmediateShard| {
                local.call_one_http_response(
                    module,
                    function,
                    argument
                        .take()
                        .expect("one HTTP argument is consumed exactly once"),
                )
            },
        )? {
            return Ok(value);
        }
        finish_immediate_step(self.begin_request_invocation(
            module,
            function,
            vec![argument.expect("ambient fallback retains its HTTP argument")],
        )?)
        .map(VmHttpCallResult::Generic)
    }

    pub(in crate::commands::serve) fn execute_projected_http_request(
        &self,
        module: &str,
        function: &str,
        request: RequestParts,
        projection: RequestFieldProjection,
        _output: &mut dyn FnMut(&str),
    ) -> Result<VmHttpCallResult, String> {
        self.require_module(module)?;
        let mut request = Some(request);
        if let Some(value) = with_current_protocol_resource(
            self.generation.identity,
            |scheduler| {
                LocalImmediateShard::new(
                    self.generation.image.spawn_shard_on_scheduler(scheduler)?,
                    module,
                    function,
                    1,
                )
            },
            |local: &mut LocalImmediateShard| {
                local.call_projected_http_request_response(
                    module,
                    function,
                    request
                        .take()
                        .expect("projected HTTP request is consumed exactly once"),
                    projection,
                )
            },
        )? {
            return Ok(value);
        }
        let request = vm_request_descriptor_owned(
            request.expect("ambient fallback retains its projected HTTP request"),
            projection,
        );
        finish_immediate_step(self.begin_request_invocation(module, function, vec![request])?)
            .map(VmHttpCallResult::Generic)
    }

    fn require_module(&self, module: &str) -> Result<(), String> {
        if module == self.module {
            Ok(())
        } else {
            Err(format!(
                "error[serve.aot.module_missing]: native handler image `{}` does not own module `{module}`",
                self.module
            ))
        }
    }
}
