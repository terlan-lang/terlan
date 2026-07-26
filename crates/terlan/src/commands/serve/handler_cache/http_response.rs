//! Typed fixed-owner HTTP response calls with generic fallback.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::vm::protocol_task_executor::with_current_protocol_resource;
use crate::runtime::vm::{ReplValue, VmHttpCallResult};

use super::invocation::AotHandlerInvocationStep;
use super::{finish_immediate_step, AotHandlerRuntime, LocalImmediateShard};
use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;

impl AotHandlerRuntime {
    /// Executes a compiler-proven suspendable HTTP export on its protocol owner.
    pub(in crate::commands::serve) async fn execute_suspendable_http_response(
        &self,
        module: &str,
        function: &str,
        args: Vec<ReplValue>,
    ) -> Result<VmHttpCallResult, String> {
        self.require_module(module)?;
        let mut step = self.begin_request_invocation(module, function, args)?;
        loop {
            step = match step {
                AotHandlerInvocationStep::Complete(value) => {
                    return Ok(VmHttpCallResult::Generic(value));
                }
                AotHandlerInvocationStep::TimerWaiting(invocation) => {
                    invocation.resume_at_deadline().await?
                }
                AotHandlerInvocationStep::Waiting(invocation) => {
                    let boundary = invocation.wait()?.boundary_type().clone();
                    let reason = format!(
                        "error[serve.aot.http_io]: HTTP handler suspended on {boundary:?} without a protocol operation adapter"
                    );
                    invocation.cancel(reason.clone())?;
                    return Err(reason);
                }
                AotHandlerInvocationStep::CapabilityWaiting(invocation) => {
                    invocation.resume_from_worker().await?
                }
            };
        }
    }

    /// Uses the same compiler-proven scalar ingress for suspendable handlers.
    pub(in crate::commands::serve) async fn execute_suspendable_projected_http_request(
        &self,
        module: &str,
        function: &str,
        request: RequestParts,
        projection: RequestFieldProjection,
    ) -> Result<VmHttpCallResult, String> {
        if let Some((entry, field)) = self.scalar_request_ingress(module, function, 1) {
            let entry = entry.to_string();
            let argument = scalar_request_argument(request, field)?;
            return self
                .execute_suspendable_http_response(module, &entry, vec![argument])
                .await;
        }
        let request = vm_request_descriptor_owned(request, projection);
        self.execute_suspendable_http_response(module, function, vec![request])
            .await
    }

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
        if let Some((entry, field)) = self.scalar_request_ingress(module, function, 1) {
            let entry = entry.to_string();
            let argument = scalar_request_argument(request, field)?;
            let mut argument = Some(argument);
            if let Some(value) = with_current_protocol_resource(
                self.generation.identity,
                |scheduler| {
                    LocalImmediateShard::new(
                        self.generation.image.spawn_shard_on_scheduler(scheduler)?,
                        module,
                        &entry,
                        1,
                    )
                },
                |local: &mut LocalImmediateShard| {
                    local.call_one_http_response(
                        module,
                        &entry,
                        argument
                            .take()
                            .expect("scalar HTTP ingress consumes its argument once"),
                    )
                },
            )? {
                return Ok(value);
            }
            return finish_immediate_step(self.begin_request_invocation(
                module,
                &entry,
                vec![argument.expect("ambient scalar ingress retains its argument")],
            )?)
            .map(VmHttpCallResult::Generic);
        }
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

fn scalar_request_argument(request: RequestParts, field: usize) -> Result<ReplValue, String> {
    let value = match field {
        RequestFieldProjection::METHOD => request.method,
        RequestFieldProjection::PATH => request.path,
        RequestFieldProjection::BODY => request.body,
        RequestFieldProjection::QUERY_STRING => request.query_string,
        _ => {
            return Err(format!(
                "error[serve.aot.scalar_request_ingress]: field {field} is not a scalar string"
            ))
        }
    };
    if field == RequestFieldProjection::BODY && value.len() >= 1024 {
        Ok(ReplValue::StringBytes(bytes::Bytes::from(value)))
    } else {
        Ok(ReplValue::String(value))
    }
}
