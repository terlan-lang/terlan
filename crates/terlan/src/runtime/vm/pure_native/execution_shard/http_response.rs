//! Typed direct HTTP completion on a reusable fixed-owner actor.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::vm::process::{VmExitReason, VmProcessId};
use crate::runtime::vm::{ReplValue, VmHttpCallResult};

use super::{NativeResultProjection, PureNativeExecution, PureNativeExecutionShard};

impl PureNativeExecutionShard {
    /// Enters one reusable service actor without routing a synchronous call
    /// through the coarse generation supervisor.
    pub(super) fn begin_fixed_owner_call_with_projection(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
        result_projection: NativeResultProjection,
    ) -> Result<PureNativeExecution, String> {
        self.require_routable("fixed_owner_call")?;
        #[cfg(test)]
        self.trace
            .push(super::NativeShardDispatchEvent::Entry { owner });
        let execution = {
            let mut context = super::PureNativeExecutionContext::new(owner, &mut self.execution);
            let begin = match result_projection {
                NativeResultProjection::PublicValue => self.boundary.begin_call_for_actor(
                    &mut self.actors,
                    &mut context,
                    function,
                    args,
                ),
                NativeResultProjection::HttpResponse => {
                    self.boundary.begin_http_response_call_for_actor(
                        &mut self.actors,
                        &mut context,
                        function,
                        args,
                    )
                }
            };
            match begin {
                Ok(execution) => execution,
                Err(error) => {
                    let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                    return match cleanup {
                        Ok(()) => Err(error),
                        Err(cleanup_error) => Err(format!(
                            "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                        )),
                    };
                }
            }
        };
        self.record_completion(owner, &execution);
        Ok(execution)
    }

    pub(super) fn begin_call_for_owner_with_projection(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
        result_projection: NativeResultProjection,
    ) -> Result<PureNativeExecution, String> {
        let operation = self.begin_internal_epoch_operation(
            "begin_call",
            super::VmShardOperationKind::ActorRoute,
            super::VmShardReplayPolicy::AtMostOnce,
        )?;
        #[cfg(test)]
        self.trace
            .push(super::NativeShardDispatchEvent::Entry { owner });
        let execution = {
            let mut context = super::PureNativeExecutionContext::new(owner, &mut self.execution);
            let begin = match result_projection {
                NativeResultProjection::PublicValue => self.boundary.begin_call_for_actor(
                    &mut self.actors,
                    &mut context,
                    function,
                    args,
                ),
                NativeResultProjection::HttpResponse => {
                    self.boundary.begin_http_response_call_for_actor(
                        &mut self.actors,
                        &mut context,
                        function,
                        args,
                    )
                }
            };
            match begin {
                Ok(execution) => execution,
                Err(error) => {
                    let _ = self.supervisor.abort_internal_operation(operation);
                    let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                    return match cleanup {
                        Ok(()) => Err(error),
                        Err(cleanup_error) => Err(format!(
                            "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                        )),
                    };
                }
            }
        };
        self.record_completion(owner, &execution);
        self.commit_internal_epoch_operation(operation)?;
        Ok(execution)
    }

    /// Runs one synchronous handler call and copies a non-file Response into a
    /// typed envelope before releasing the request heap.
    #[allow(dead_code)] // Retained for the typed HTTP response fast path.
    pub(crate) fn call_on_fixed_owner_http_response(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<VmHttpCallResult, String> {
        if !self.actors.is_alive(owner) {
            return Err(format!(
                "error[execution_shard.fixed_owner]: actor {} is not alive",
                owner.as_u64()
            ));
        }
        self.call_on_admitted_fixed_owner_http_response(owner, function, args)
    }

    /// Runs a synchronous call on the service actor retained exclusively by
    /// an admitted owner-local shard.
    ///
    /// The caller creates the actor and replaces the whole local shard after
    /// any error, so neither actor-directory nor lifecycle state can change
    /// between calls. General actor entry retains the checked method above.
    pub(crate) fn call_on_admitted_fixed_owner_http_response(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<VmHttpCallResult, String> {
        debug_assert!(self.actors.is_alive(owner));
        debug_assert!(self.supervisor.is_routable());
        let result = (|| {
            #[cfg(test)]
            self.trace
                .push(super::NativeShardDispatchEvent::Entry { owner });
            let mut context = super::PureNativeExecutionContext::new(owner, &mut self.execution);
            let mut execution = self.boundary.begin_http_response_call_for_actor(
                &mut self.actors,
                &mut context,
                function,
                args,
            )?;
            self.record_completion(owner, &execution);
            loop {
                execution = match execution {
                    PureNativeExecution::Complete(value) => {
                        self.reset_owner_heap(owner)?;
                        return Ok(VmHttpCallResult::Generic(value));
                    }
                    PureNativeExecution::HttpResponse(response) => {
                        self.reset_owner_heap(owner)?;
                        return Ok(VmHttpCallResult::Response(response));
                    }
                    PureNativeExecution::Suspended(suspension) => {
                        self.resume_call(owner, suspension)?
                    }
                };
            }
        })();
        if result.is_err() && self.actors.is_alive(owner) {
            let _ = self.finish_owner(
                owner,
                VmExitReason::Error("fixed-owner HTTP call failed".to_string()),
            );
        }
        result
    }

    /// Runs a compiler-projected Request directly through the admitted
    /// fixed-owner actor without first materializing a generic host aggregate.
    pub(crate) fn call_on_admitted_fixed_owner_projected_http_request(
        &mut self,
        owner: VmProcessId,
        function: &str,
        request: RequestParts,
        projection: RequestFieldProjection,
    ) -> Result<VmHttpCallResult, String> {
        debug_assert!(self.actors.is_alive(owner));
        debug_assert!(self.supervisor.is_routable());
        let result = (|| {
            #[cfg(test)]
            self.trace
                .push(super::NativeShardDispatchEvent::Entry { owner });
            let mut context = super::PureNativeExecutionContext::new(owner, &mut self.execution);
            let mut execution = self.boundary.begin_projected_http_request_call_for_actor(
                &mut self.actors,
                &mut context,
                function,
                request,
                projection,
            )?;
            self.record_completion(owner, &execution);
            loop {
                execution = match execution {
                    PureNativeExecution::Complete(value) => {
                        self.reset_owner_heap(owner)?;
                        return Ok(VmHttpCallResult::Generic(value));
                    }
                    PureNativeExecution::HttpResponse(response) => {
                        self.reset_owner_heap(owner)?;
                        return Ok(VmHttpCallResult::Response(response));
                    }
                    PureNativeExecution::Suspended(suspension) => {
                        self.resume_call(owner, suspension)?
                    }
                };
            }
        })();
        if result.is_err() && self.actors.is_alive(owner) {
            let _ = self.finish_owner(
                owner,
                VmExitReason::Error("fixed-owner projected HTTP call failed".to_string()),
            );
        }
        result
    }
}
