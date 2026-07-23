//! Owner-local immediate generated-call reuse.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;
use crate::runtime::vm::VmHttpCallResult;

use super::invocation;
use crate::commands::serve::handler::request_materialization::{
    replace_vm_request_descriptor, vm_request_descriptor_owned,
};

pub(super) struct LocalImmediateShard {
    shard: PureNativeExecutionShard,
    owner: VmProcessId,
    function: String,
    export: String,
    #[allow(dead_code)] // Read by single-argument fast paths selected by code generation.
    argument_scratch: Vec<ReplValue>,
    request_scratch: Option<ReplValue>,
}

impl LocalImmediateShard {
    pub(super) fn new(
        mut shard: PureNativeExecutionShard,
        module: &str,
        function: &str,
        arity: usize,
    ) -> Result<Self, String> {
        let export = format!("{module}.{function}");
        let owner = shard.spawn_fixed_owner_actor(&export, arity)?;
        Ok(Self {
            shard,
            owner,
            function: function.to_owned(),
            export,
            argument_scratch: Vec::with_capacity(4),
            request_scratch: None,
        })
    }

    fn select_export(&mut self, module: &str, function: &str) {
        if self.function == function {
            return;
        }
        self.function.clear();
        self.function.push_str(function);
        self.export.clear();
        self.export.push_str(module);
        self.export.push('.');
        self.export.push_str(function);
    }

    pub(super) fn call(
        &mut self,
        module: &str,
        function: &str,
        args: &[ReplValue],
    ) -> Result<ReplValue, String> {
        self.select_export(module, function);
        self.call_selected(args)
    }

    #[allow(dead_code)] // Selected only for generated single-argument calls.
    pub(super) fn call_one(
        &mut self,
        module: &str,
        function: &str,
        argument: ReplValue,
    ) -> Result<ReplValue, String> {
        self.select_export(module, function);
        self.argument_scratch.clear();
        self.argument_scratch.push(argument);
        let result =
            self.shard
                .call_on_fixed_owner(self.owner, &self.export, &self.argument_scratch);
        self.argument_scratch.clear();
        self.finish_call(result, 1)
    }

    pub(super) fn call_http_response(
        &mut self,
        module: &str,
        function: &str,
        args: &[ReplValue],
    ) -> Result<VmHttpCallResult, String> {
        self.select_export(module, function);
        let result =
            self.shard
                .call_on_admitted_fixed_owner_http_response(self.owner, &self.export, args);
        self.finish_http_call(result, args.len())
    }

    #[allow(dead_code)] // Selected only for generated typed HTTP response calls.
    pub(super) fn call_one_http_response(
        &mut self,
        module: &str,
        function: &str,
        argument: ReplValue,
    ) -> Result<VmHttpCallResult, String> {
        self.select_export(module, function);
        self.argument_scratch.clear();
        self.argument_scratch.push(argument);
        let result = self.shard.call_on_admitted_fixed_owner_http_response(
            self.owner,
            &self.export,
            &self.argument_scratch,
        );
        self.argument_scratch.clear();
        self.finish_http_call(result, 1)
    }

    pub(super) fn call_projected_http_request_response(
        &mut self,
        module: &str,
        function: &str,
        request: RequestParts,
        projection: RequestFieldProjection,
    ) -> Result<VmHttpCallResult, String> {
        self.select_export(module, function);
        match self.request_scratch.as_mut() {
            Some(scratch) => replace_vm_request_descriptor(scratch, request, projection),
            None => {
                self.request_scratch = Some(vm_request_descriptor_owned(request, projection));
            }
        }
        let result = self.shard.call_on_admitted_fixed_owner_http_response(
            self.owner,
            &self.export,
            std::slice::from_ref(
                self.request_scratch
                    .as_ref()
                    .expect("projected request scratch is initialized"),
            ),
        );
        self.finish_http_call(result, 1)
    }

    fn call_selected(&mut self, args: &[ReplValue]) -> Result<ReplValue, String> {
        let result = self
            .shard
            .call_on_fixed_owner(self.owner, &self.export, args);
        self.finish_call(result, args.len())
    }

    fn finish_call(
        &mut self,
        result: Result<ReplValue, String>,
        arity: usize,
    ) -> Result<ReplValue, String> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.owner = self
                    .shard
                    .spawn_fixed_owner_actor(&self.export, arity)
                    .map_err(|restart| {
                        format!("{error}; error[serve.aot.fixed_owner_restart]: {restart}")
                    })?;
                Err(error)
            }
        }
    }

    fn finish_http_call(
        &mut self,
        result: Result<VmHttpCallResult, String>,
        arity: usize,
    ) -> Result<VmHttpCallResult, String> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.owner = self
                    .shard
                    .spawn_fixed_owner_actor(&self.export, arity)
                    .map_err(|restart| {
                        format!("{error}; error[serve.aot.fixed_owner_restart]: {restart}")
                    })?;
                Err(error)
            }
        }
    }
}

pub(super) fn finish_immediate_step(
    step: invocation::AotHandlerInvocationStep,
) -> Result<ReplValue, String> {
    match step {
        invocation::AotHandlerInvocationStep::Complete(value) => Ok(value),
        invocation::AotHandlerInvocationStep::Waiting(invocation) => {
            let boundary = invocation.wait()?.boundary_type().clone();
            let reason = format!(
                "error[serve.aot.async_io_unavailable]: immediate native callback suspended on {boundary:?}; use VM-owned asynchronous request orchestration"
            );
            match invocation.cancel(reason.clone()) {
                Ok(()) => Err(reason),
                Err(cleanup) => Err(format!(
                    "{reason}; error[serve.aot.invocation_shutdown]: {cleanup}"
                )),
            }
        }
        invocation::AotHandlerInvocationStep::CapabilityWaiting(invocation) => {
            let request = invocation.request()?;
            Err(format!(
                "error[serve.aot.capability_unavailable]: immediate native callback suspended on capability `{}` operation `{}`; use VM-owned asynchronous capability orchestration",
                request.capability, request.operation
            ))
        }
    }
}
