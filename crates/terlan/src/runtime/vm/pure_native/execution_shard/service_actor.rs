//! Entry and reusable fixed-owner service actor calls.

use crate::runtime::vm::process::{VmExitReason, VmProcessId};
use crate::runtime::vm::ReplValue;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::super::{NativeResultProjection, PureNativeExecution};
use super::{call_source, PureNativeExecutionShard};

fn service_actor_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::VmRuntime,
        "execute fixed-owner service actor",
        rendered,
    )
}

impl PureNativeExecutionShard {
    /// Starts one generated entry under a newly allocated local actor.
    pub(crate) fn begin_call(
        &mut self,
        function: &str,
        args: &[ReplValue],
    ) -> Result<(VmProcessId, PureNativeExecution), BoundaryError> {
        self.require_routable("begin_call")
            .map_err(service_actor_error)?;
        let owner = self
            .actors
            .spawn_fixed_owner_root(call_source(function, args.len()));
        self.begin_call_for_owner(owner, function, args)
            .map(|execution| (owner, execution))
    }

    /// Creates one long-lived service actor on this shard's fixed owner.
    pub(crate) fn spawn_fixed_owner_actor(
        &mut self,
        function: &str,
        arity: usize,
    ) -> Result<VmProcessId, BoundaryError> {
        self.require_routable("spawn_fixed_owner_actor")
            .map_err(service_actor_error)?;
        Ok(self
            .actors
            .spawn_fixed_owner_root(call_source(function, arity)))
    }

    /// Starts an externally controlled debugger call on an existing actor.
    pub(crate) fn begin_debug_call(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, BoundaryError> {
        if !self.actors.is_alive(owner) {
            return Err(service_actor_error(format!(
                "error[execution_shard.debug_owner]: actor {} is not alive",
                owner.as_u64()
            )));
        }
        self.begin_call_for_owner(owner, function, args)
    }

    /// Runs one complete synchronous call on an existing fixed-owner actor.
    pub(crate) fn call_on_fixed_owner(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<ReplValue, BoundaryError> {
        if !self.actors.is_alive(owner) {
            return Err(service_actor_error(format!(
                "error[execution_shard.fixed_owner]: actor {} is not alive",
                owner.as_u64()
            )));
        }
        let result = (|| {
            let mut execution = self
                .begin_fixed_owner_call_with_projection(
                    owner,
                    function,
                    args,
                    NativeResultProjection::PublicValue,
                )
                .map_err(service_actor_error)?;
            loop {
                execution = match execution {
                    PureNativeExecution::Complete(value) => {
                        self.reset_owner_heap(owner).map_err(service_actor_error)?;
                        return Ok(value);
                    }
                    PureNativeExecution::HttpResponse(_) => {
                        return Err(service_actor_error("error[execution_shard.result_projection]: HTTP response returned through a public-value call"))
                    }
                    PureNativeExecution::Suspended(suspension) => {
                        self.resume_call(owner, *suspension)
                            .map_err(service_actor_error)?
                    }
                };
            }
        })();
        if result.is_err() && self.actors.is_alive(owner) {
            let _ = self.finish_owner(
                owner,
                VmExitReason::Error("fixed-owner call failed".to_string()),
            );
        }
        result
    }

    fn begin_call_for_owner(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, BoundaryError> {
        self.begin_call_for_owner_with_projection(
            owner,
            function,
            args,
            NativeResultProjection::PublicValue,
        )
        .map_err(service_actor_error)
    }
}
