//! Exact application-granted contexts on the ordinary capability boundary.

use super::*;
use crate::runtime::vm::native_value::{from_native, to_native};

impl PureNativeExecutionShard {
    pub(crate) fn has_native_service(&self, wait: &PureNativeCapabilityWait) -> bool {
        self.execution
            .managed_ref()
            .native_services()
            .is_some_and(|services| services.contains(&wait.request.operation))
    }

    /// Executes a host-granted synchronous context on its owner. These grants
    /// are not an escape hatch for blocking I/O or isolated worker operations.
    pub(crate) fn resume_native_service_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
    ) -> Result<PureNativeExecution, String> {
        let epoch = self.require_active_epoch("resume_native_service_call")?;
        wait.validate(self.supervisor.shard_id(), epoch, owner, &suspension)?;
        self.execution.validate_continuation(
            owner.as_u64(),
            suspension.request_id(),
            suspension.continuation_id(),
        )?;
        // Consume the at-most-once operation before invoking package code.
        // Callback failures must not make the mutation replayable.
        self.commit_epoch_operation(wait.completion)?;
        let result = (|| {
            self.actors.validate_native_continuation_owner(
                owner.as_u64(),
                suspension.request_id(),
                suspension.continuation_id(),
            )?;
            let services = self.execution.managed_ref().native_services().ok_or(
                "error[native_service.unavailable]: image has no granted service contexts",
            )?;
            let arguments = wait.request.package_arguments.as_ref().ok_or(
                "error[native_service.arguments]: service call has no decoded package arguments",
            )?;
            services
                .validate_arity(&wait.request.operation, arguments.len())
                .map_err(|error| error.to_string())?;
            let arguments = arguments
                .iter()
                .map(to_native)
                .collect::<Result<Vec<_>, _>>()
                .map_err(String::from)?;
            services
                .call(&wait.request.operation, &arguments)
                .map(from_native)
                .map_err(|error| error.to_string())
        })();
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                return match self.finish_owner(owner, VmExitReason::Error(error.clone())) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(format!(
                        "{error}; error[execution_shard.cleanup]: {cleanup}"
                    )),
                };
            }
        };
        self.complete_capability_value(owner, suspension, &wait.request.result_type, &value)
    }

    pub(crate) fn resume_resident_native_service_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
    ) -> Result<bool, String> {
        let execution = self.resume_native_service_call(owner, suspension, wait)?;
        self.drive_resident_capability_execution(owner, execution)
    }
}

#[cfg(test)]
#[path = "native_service_ingress_test.rs"]
mod tests;
