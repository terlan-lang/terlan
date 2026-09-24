//! Drives root and resident AOT calls through VM-owned capability services.

use super::*;
use crate::runtime::vm::capability_worker::VmCapabilityId;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{PureNativeCapabilityWait, PureNativeSuspension};
use std::sync::Arc;
use std::task::{Wake, Waker};

/// Transport notifications wake the VM owner, never execute generated code themselves.
struct OwnerWake(std::thread::Thread);
impl Wake for OwnerWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Releases actor-owned helper resources on success, error, or unwinding.
struct OwnerResources<'a> {
    helpers: &'a mut VmPackageNativeHelpers,
    owner: VmProcessId,
}

impl Drop for OwnerResources<'_> {
    fn drop(&mut self) {
        self.helpers.close_owner(self.owner.as_u64());
    }
}

/// Executes one shard call while servicing package-native capabilities through
/// one lazily started helper process.
pub(crate) fn execute_call(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
    function: &str,
    arguments: &[ReplValue],
) -> VmRuntimeResult<ReplValue> {
    let (owner, execution) = shard.begin_call(function, arguments)?;
    let resources = OwnerResources { helpers, owner };
    execute_root(shard, &mut *resources.helpers, owner, execution)
}

/// Drives the root while its helper-resource lifetime is guarded by the caller.
fn execute_root(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
    owner: VmProcessId,
    execution: PureNativeExecution,
) -> VmRuntimeResult<ReplValue> {
    let mut execution = Some(execution);
    let waker = Waker::from(Arc::new(OwnerWake(std::thread::current())));
    loop {
        helpers.storage_workers.register_waker(&waker);
        let completion = match helpers.storage_workers.poll() {
            Ok(completion) => completion,
            Err(error) => return cancel_with_error(shard, owner, error),
        };
        let storage_progress = completion.is_some();
        if let Some(completion) = completion {
            let mut pending = completion.pending;
            let value = completion.reply.and_then(|reply| {
                helpers.distributed_storage.complete_step(
                    pending.owner.as_u64(),
                    pending.projection,
                    reply,
                )
            });
            let value = match value {
                Ok(distributed_storage::Step::Complete(value)) => Ok(value),
                Ok(distributed_storage::Step::Continue(request)) => {
                    let pending_owner = pending.owner;
                    pending.projection = request.pending;
                    if let Err(error) = helpers.storage_workers.submit(
                        &request.backend,
                        request.operation,
                        request.arguments,
                        pending,
                    ) {
                        if pending_owner == owner {
                            return cancel_with_error(shard, owner, error);
                        }
                        return Err(
                            fail_resident_capability(shard, helpers, pending_owner, error).into(),
                        );
                    }
                    continue;
                }
                Err(error) => Err(error),
            };
            if pending.owner == owner {
                let value = match value {
                    Ok(value) => value,
                    Err(error) => return cancel_with_error(shard, owner, error),
                };
                execution = Some(shard.resume_capability_value_call(
                    owner,
                    pending.suspension,
                    pending.wait,
                    value,
                )?);
            } else {
                let value = value.map_err(|error| {
                    fail_resident_capability(shard, helpers, pending.owner, error)
                })?;
                if shard
                    .resume_resident_capability_value_call(
                        pending.owner,
                        pending.suspension,
                        pending.wait,
                        value,
                    )
                    .map_err(|error| {
                        fail_resident_capability(shard, helpers, pending.owner, error)
                    })?
                {
                    helpers.close_owner(pending.owner.as_u64());
                }
            }
        }
        let resident_progress = match service_resident_capability(shard, helpers) {
            Ok(progress) => progress,
            Err(error) => return cancel_with_error(shard, owner, error),
        };
        let Some(current) = execution.take() else {
            if !resident_progress && !storage_progress {
                std::thread::park_timeout(helpers.storage_workers.idle_duration());
            }
            continue;
        };
        execution = Some(match current {
            PureNativeExecution::Complete(value) => {
                shard.finish_completed_call(owner)?;
                return Ok(value);
            }
            PureNativeExecution::HttpResponse(_) => {
                shard.cancel_call(owner, "package call returned an HTTP response")?;
                return Err(
                    "error[execution_shard.result_projection]: package call returned an HTTP response"
                        .into(),
                );
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Capability =>
            {
                let wait = match shard.begin_capability_call(owner, &suspension) {
                    Ok(wait) => wait,
                    Err(error) => return cancel_with_error(shard, owner, error),
                };
                let storage_request = match helpers
                    .distributed_storage
                    .prepare(owner.as_u64(), wait.request())
                {
                    Ok(request) => request,
                    Err(error) => return cancel_with_error(shard, owner, error),
                };
                if let Some(request) = storage_request {
                    if let Err(error) = submit_storage(helpers, owner, *suspension, wait, request) {
                        return cancel_with_error(shard, owner, error);
                    }
                    continue;
                }
                if wait.request().package_arguments.is_none() {
                    let reply = match dispatch_vm_capability_with_program_arguments(
                        wait.request(),
                        &helpers.program_arguments,
                    ) {
                        Ok(reply) => reply,
                        Err(error) => return cancel_with_error(shard, owner, error),
                    };
                    shard.resume_capability_call(owner, *suspension, wait, reply)?
                } else {
                    let operation = wait.request().operation.clone();
                    let value =
                        match helpers.call(owner.as_u64(), wait.request(), wait.admitted_atoms()) {
                            Ok(value) => value,
                            Err(error) => return cancel_with_error(shard, owner, error),
                        };
                    let handles = match accelerator_resource_handles(&value) {
                        Ok(handles) => handles,
                        Err(error) => return cancel_with_error(shard, owner, error),
                    };
                    if let Err(error) = shard.register_accelerator_resources(owner, handles) {
                        return cancel_with_error(shard, owner, error);
                    }
                    shard
                        .resume_capability_value_call(owner, *suspension, wait, value)
                        .map_err(|error| {
                            format!(
                                "{error}; error[native_helper_resume]: operation `{operation}` returned an incompatible value"
                            )
                        })?
                }
            }
            PureNativeExecution::Suspended(suspension) => shard.resume_call(owner, *suspension)?,
        });
    }
}

#[cfg(test)]
#[path = "execution_test.rs"]
mod tests;

/// Services one runnable capability wait owned by a spawned actor.
fn service_resident_capability(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
) -> VmRuntimeResult<bool> {
    let Some((owner, suspension, wait)) = shard.take_resident_capability_call()? else {
        return Ok(false);
    };
    if let Some(request) = helpers
        .distributed_storage
        .prepare(owner.as_u64(), wait.request())
        .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?
    {
        submit_storage(helpers, owner, suspension, wait, request)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        return Ok(true);
    }
    let completed = if wait.request().package_arguments.is_none() {
        let reply = dispatch_vm_capability_with_program_arguments(
            wait.request(),
            &helpers.program_arguments,
        )
        .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        shard
            .resume_resident_capability_call(owner, suspension, wait, reply)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?
    } else {
        let value = helpers
            .call(owner.as_u64(), wait.request(), wait.admitted_atoms())
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        let handles = accelerator_resource_handles(&value)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        shard
            .register_accelerator_resources(owner, handles)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        shard
            .resume_resident_capability_value_call(owner, suspension, wait, value)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?
    };
    if completed {
        helpers.close_owner(owner.as_u64());
    }
    Ok(true)
}

/// Retains the exact shard epoch while replacing package authority with a checked storage grant.
fn submit_storage(
    helpers: &mut VmPackageNativeHelpers,
    owner: VmProcessId,
    suspension: PureNativeSuspension,
    wait: PureNativeCapabilityWait,
    request: distributed_storage::Request,
) -> VmRuntimeResult<()> {
    let mut context = wait.worker_context()?;
    context.capability = VmCapabilityId::new("storage")?;
    helpers.storage_workers.submit(
        &request.backend,
        request.operation,
        request.arguments,
        storage_transport::StorageWait {
            owner,
            suspension,
            wait,
            projection: request.pending,
            context,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
        },
    )
}

/// Closes helper state and commits a resident capability failure to actor exit.
fn fail_resident_capability(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
    owner: crate::runtime::vm::process::VmProcessId,
    error: impl Into<String>,
) -> String {
    let error = error.into();
    helpers.close_owner(owner.as_u64());
    match shard.cancel_call(owner, error.clone()) {
        Ok(()) => error,
        Err(cleanup) => format!("{error}; error[execution_shard.cleanup]: {cleanup}"),
    }
}
