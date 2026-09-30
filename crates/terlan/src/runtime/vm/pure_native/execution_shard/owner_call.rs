//! Native entry and completion on reusable fixed-owner actors.

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::process::{VmExitReason, VmProcessId};
use crate::runtime::vm::ReplValue;

use super::super::NativeCallTarget;
use super::{PureNativeExecution, PureNativeExecutionShard};

impl PureNativeExecutionShard {
    /// Enters one reusable service actor without routing a synchronous call
    /// through the coarse generation supervisor.
    pub(super) fn begin_fixed_owner_call(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.require_routable("fixed_owner_call")?;
        #[cfg(test)]
        self.trace
            .push(super::NativeShardDispatchEvent::Entry { owner });
        let execution = {
            let mut context = super::PureNativeExecutionContext::new(owner, &mut self.execution);
            let begin =
                self.boundary
                    .begin_call_for_actor(&mut self.actors, &mut context, function, args);
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

    pub(super) fn begin_routed_owner_call(
        &mut self,
        owner: VmProcessId,
        target: &NativeCallTarget<'_>,
        args: &[ReplValue],
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
            let begin = self
                .boundary
                .begin_target_for_actor(&mut self.actors, &mut context, target, args, false)
                .map_err(String::from);
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

    /// Runs a synchronous call on the service actor retained exclusively by
    /// an admitted owner-local shard.
    ///
    /// The caller creates the actor and replaces the whole local shard after
    /// any error, so neither actor-directory nor lifecycle state can change
    /// between calls. General actor entry retains the checked method above.
    pub(crate) fn call_on_admitted_fixed_owner(
        &mut self,
        owner: VmProcessId,
        function: &str,
        args: &[ReplValue],
    ) -> Result<ReplValue, String> {
        debug_assert!(self.actors.is_alive(owner));
        debug_assert!(self.supervisor.is_routable());
        let result = (|| {
            #[cfg(test)]
            self.trace
                .push(super::NativeShardDispatchEvent::Entry { owner });
            let mut context = super::PureNativeExecutionContext::new(owner, &mut self.execution);
            let mut execution = self.boundary.begin_admitted_call_for_actor(
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
                        return Ok(value);
                    }
                    PureNativeExecution::Suspended(suspension) => {
                        if matches!(
                            suspension.operation(),
                            TvmTransitionOperation::Capability
                                | TvmTransitionOperation::Receive
                                | TvmTransitionOperation::Timer
                        ) {
                            return Err(format!(
                                "error[execution_shard.external_wait]: synchronous call requires asynchronous {:?} orchestration",
                                suspension.operation()
                            ));
                        }
                        self.resume_call(owner, *suspension)?
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
}
