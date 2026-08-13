//! Owner-local immediate generated-call reuse.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::process::VmProcessId;
#[cfg(test)]
use crate::runtime::vm::pure_native::PureNativeIoWake;
use crate::runtime::vm::pure_native::{
    PureNativeCapabilityWait, PureNativeExecution, PureNativeExecutionShard, PureNativeSuspension,
    PureNativeTimerWait,
};
use crate::runtime::vm::scheduler_topology::VmFixedActorRoute;
use crate::runtime::vm::ReplValue;
use crate::runtime::vm::VmHttpCallResult;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;
use std::time::{Duration, Instant};

use super::invocation;
use super::shard_owner::OwnedInvocationStep;
use crate::commands::serve::handler::request_materialization::{
    replace_vm_request_descriptor, vm_request_descriptor_owned,
};

pub(super) struct LocalImmediateShard {
    shard: PureNativeExecutionShard,
    owner: VmProcessId,
    function: String,
    export: String,
    argument_scratch: Vec<ReplValue>,
    request_scratch: Option<ReplValue>,
    timer_origin: Instant,
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
            timer_origin: Instant::now(),
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
        self.finish_call(result.map_err(String::from), args.len())
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

    /// Starts a suspendable actor directly on the protocol owner.
    pub(super) fn begin(
        &mut self,
        route: VmFixedActorRoute,
        export: String,
        args: Vec<ReplValue>,
    ) -> Result<OwnedInvocationStep, String> {
        let (owner, execution) = self.shard.begin_call(&export, &args)?;
        self.advance(route, owner, execution)
    }

    /// Resumes one protocol-owned actor from its exact typed I/O wake.
    #[cfg(test)]
    pub(super) fn resume(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wake: PureNativeIoWake,
    ) -> Result<OwnedInvocationStep, String> {
        let execution = self.shard.resume_io_call(owner, suspension, wake)?;
        self.advance(route, owner, execution)
    }

    /// Resumes one protocol-owned actor from an isolated capability result.
    pub(super) fn resume_capability(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        outcome: NativeBoundaryReplyTerm,
    ) -> Result<OwnedInvocationStep, String> {
        let execution = self
            .shard
            .resume_capability_call(owner, suspension, wait, outcome)?;
        self.advance(route, owner, execution)
    }

    /// Resumes one protocol-owned actor at its generation-qualified deadline.
    pub(super) fn resume_timer(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeTimerWait,
    ) -> Result<OwnedInvocationStep, String> {
        let execution = self.shard.resume_timer_call(owner, suspension, wait)?;
        self.advance(route, owner, execution)
    }

    /// Releases a parked protocol-owned actor without crossing a thread.
    pub(super) fn cancel(&mut self, owner: VmProcessId, reason: String) -> Result<(), String> {
        self.shard.cancel_call(owner, reason)
    }

    fn advance(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        mut execution: PureNativeExecution,
    ) -> Result<OwnedInvocationStep, String> {
        loop {
            match execution {
                PureNativeExecution::Complete(value) => {
                    self.shard.finish_completed_call(owner)?;
                    return Ok(OwnedInvocationStep::Complete { route, value });
                }
                PureNativeExecution::HttpResponse(_) => {
                    self.shard.cancel_call(
                        owner,
                        "typed HTTP response entered suspendable invocation path",
                    )?;
                    return Err("error[serve.aot.result_projection]: typed HTTP response entered the asynchronous invocation path".to_string());
                }
                PureNativeExecution::Suspended(suspension)
                    if suspension.operation() == TvmTransitionOperation::Receive =>
                {
                    let wait = self.shard.io_wait(owner, &suspension)?;
                    return Ok(OwnedInvocationStep::Waiting {
                        route,
                        owner,
                        suspension: *suspension,
                        wait,
                    });
                }
                PureNativeExecution::Suspended(suspension)
                    if suspension.operation() == TvmTransitionOperation::Capability =>
                {
                    let wait = self.shard.begin_capability_call(owner, &suspension)?;
                    return Ok(OwnedInvocationStep::CapabilityWaiting {
                        route,
                        owner,
                        suspension: *suspension,
                        wait,
                    });
                }
                PureNativeExecution::Suspended(suspension)
                    if suspension.operation() == TvmTransitionOperation::Timer =>
                {
                    let observed_tick =
                        u64::try_from(self.timer_origin.elapsed().as_millis()).unwrap_or(u64::MAX);
                    let wait = self
                        .shard
                        .begin_timer_call(owner, &suspension, observed_tick)?;
                    let due = self
                        .timer_origin
                        .checked_add(Duration::from_millis(wait.deadline_tick()))
                        .ok_or_else(|| {
                            "error[serve.aot.protocol_timer]: host deadline overflow".to_string()
                        })?;
                    return Ok(OwnedInvocationStep::TimerWaiting {
                        route,
                        owner,
                        suspension: *suspension,
                        wait,
                        due,
                    });
                }
                PureNativeExecution::Suspended(suspension) => {
                    execution = self.shard.resume_call(owner, *suspension)?;
                }
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
        invocation::AotHandlerInvocationStep::TimerWaiting(invocation) => {
            let reason =
                "error[serve.aot.timer_unavailable]: immediate native callback suspended on a timer; use VM-owned asynchronous request orchestration"
                    .to_string();
            match invocation.cancel(reason.clone()) {
                Ok(()) => Err(reason),
                Err(cleanup) => Err(format!(
                    "{reason}; error[serve.aot.invocation_shutdown]: {cleanup}"
                )),
            }
        }
    }
}
