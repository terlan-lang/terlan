//! Request-owned entry and resume lifecycle for native HTTP handlers.

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{
    PureNativeExecution, PureNativeExecutionShard, PureNativeIoWait, PureNativeIoWake,
    PureNativeSuspension,
};
use crate::runtime::vm::ReplValue;

use super::AotHandlerRuntime;

/// One observable step from a request-owned native handler invocation.
#[derive(Debug)]
pub(in crate::commands::serve) enum AotHandlerInvocationStep {
    /// Generated handler returned and released all request-owned VM state.
    Complete(ReplValue),
    /// Generated handler is parked until its exact typed VM I/O wake arrives.
    Waiting(AotHandlerInvocation),
}

/// Linear ownership of one generated HTTP handler while it is parked.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotHandlerInvocation {
    /// Independently mutable execution shard owned by this request.
    shard: PureNativeExecutionShard,
    /// Exact VM actor executing the generated handler.
    owner: VmProcessId,
    /// Owned generated continuation retained outside native stack memory.
    suspension: PureNativeSuspension,
}

impl AotHandlerRuntime {
    /// Enters one generated handler under a new request-owned execution shard.
    pub(in crate::commands::serve) fn begin_request_invocation(
        &self,
        module: &str,
        function: &str,
        args: Vec<ReplValue>,
    ) -> Result<AotHandlerInvocationStep, String> {
        if module != self.module {
            return Err(format!(
                "error[serve.aot.module_missing]: native handler image `{}` does not own module `{module}`",
                self.module
            ));
        }
        let mut shard = self.image.spawn_shard()?;
        let (owner, execution) = shard.begin_call(&format!("{module}.{function}"), &args)?;
        finish_invocation_step(shard, owner, execution)
    }
}

impl AotHandlerInvocation {
    /// Returns the exact typed VM I/O wait currently owned by this request.
    pub(in crate::commands::serve) fn wait(&self) -> Result<PureNativeIoWait, String> {
        self.shard.io_wait(self.owner, &self.suspension)
    }

    /// Resumes generated code through execution-shard authority after one wake.
    pub(in crate::commands::serve) fn resume(
        mut self,
        wake: PureNativeIoWake,
    ) -> Result<AotHandlerInvocationStep, String> {
        let execution = match self.shard.resume_io_call(self.owner, self.suspension, wake) {
            Ok(execution) => execution,
            Err(error) => {
                return match self.shard.shutdown() {
                    Ok(()) => Err(error),
                    Err(shutdown) => Err(format!(
                        "{error}; error[serve.aot.invocation_shutdown]: {shutdown}"
                    )),
                };
            }
        };
        finish_invocation_step(self.shard, self.owner, execution)
    }

    /// Cancels a parked request and releases its actor and execution shard.
    pub(in crate::commands::serve) fn cancel(mut self, reason: String) -> Result<(), String> {
        self.shard.cancel_call(self.owner, reason)?;
        self.shard.shutdown()
    }
}

/// Converts a shard execution result into the request-owned invocation state.
fn finish_invocation_step(
    mut shard: PureNativeExecutionShard,
    owner: VmProcessId,
    mut execution: PureNativeExecution,
) -> Result<AotHandlerInvocationStep, String> {
    loop {
        match execution {
            PureNativeExecution::Complete(value) => {
                shard.finish_completed_call(owner)?;
                shard.shutdown()?;
                return Ok(AotHandlerInvocationStep::Complete(value));
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Receive =>
            {
                let invocation = AotHandlerInvocation {
                    shard,
                    owner,
                    suspension,
                };
                invocation.wait()?;
                return Ok(AotHandlerInvocationStep::Waiting(invocation));
            }
            PureNativeExecution::Suspended(suspension) => {
                execution = shard.resume_call(owner, suspension)?;
            }
        }
    }
}

#[cfg(test)]
#[path = "invocation_test.rs"]
mod invocation_test;
