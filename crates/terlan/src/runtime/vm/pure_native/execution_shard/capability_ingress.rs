//! Generation-fenced capability completion ingress for generated continuations.

use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::capability_worker::{VmCapabilityId, VmCapabilityRequestContext};
use crate::runtime::vm::execution_shard_epoch::{
    VmShardEpochOperation, VmShardOperationKind, VmShardReplayPolicy,
};
use crate::runtime::vm::execution_shard_protocol::{VmExecutionShardId, VmShardEpoch};
use crate::runtime::vm::process::{VmExitReason, VmProcessId};
use crate::runtime::vm::pure_native::{
    PureNativeCapabilityRequest, PureNativeExecution, PureNativeExecutionContext,
    PureNativeSuspension,
};
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

use super::PureNativeExecutionShard;

/// One generated capability wait tied to an exact shard generation and continuation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PureNativeCapabilityWait {
    shard: VmExecutionShardId,
    epoch: VmShardEpoch,
    owner: VmProcessId,
    request_id: u64,
    continuation_id: u64,
    request: PureNativeCapabilityRequest,
    completion: VmShardEpochOperation,
}

impl PureNativeCapabilityWait {
    /// Returns the decoded worker request without exposing the parked continuation.
    pub(crate) fn request(&self) -> &PureNativeCapabilityRequest {
        &self.request
    }

    /// Builds worker correlation authority from this exact epoch operation.
    pub(crate) fn worker_context(&self) -> Result<VmCapabilityRequestContext, String> {
        VmCapabilityRequestContext::new(
            VmCapabilityId::new(self.request.capability)?,
            self.completion,
        )
    }

    /// Validates this wait against the current shard and supplied suspension.
    fn validate(
        &self,
        shard: &VmExecutionShardId,
        epoch: VmShardEpoch,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<(), String> {
        if &self.shard != shard || self.epoch != epoch {
            return Err(
                "error[execution_shard.capability_generation]: stale capability wait".to_string(),
            );
        }
        if self.owner != owner
            || suspension.owner_id() != owner.as_u64()
            || self.request_id != suspension.request_id()
            || self.continuation_id != suspension.continuation_id()
        {
            return Err("error[execution_shard.capability_continuation]: capability wait does not match the parked continuation".to_string());
        }
        Ok(())
    }
}

impl PureNativeExecutionShard {
    /// Decodes and admits one generated capability suspension under the active epoch.
    pub(crate) fn begin_capability_call(
        &mut self,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<PureNativeCapabilityWait, String> {
        let epoch = self.require_active_epoch("begin_capability_call")?;
        let request = {
            let context = PureNativeExecutionContext::new(owner, &mut self.execution);
            self.boundary
                .capability_request_for_actor(&context, suspension)?
        };
        let completion = self.begin_epoch_operation(
            "capability completion",
            VmShardOperationKind::CapabilityCompletion,
            VmShardReplayPolicy::AtMostOnce,
        )?;
        Ok(PureNativeCapabilityWait {
            shard: self.supervisor.shard_id().clone(),
            epoch,
            owner,
            request_id: suspension.request_id(),
            continuation_id: suspension.continuation_id(),
            request,
            completion,
        })
    }

    /// Applies one worker reply and resumes generated code only on this shard owner.
    pub(crate) fn resume_capability_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        reply: NativeBoundaryReplyTerm,
    ) -> Result<PureNativeExecution, String> {
        let epoch = self.require_active_epoch("resume_capability_call")?;
        wait.validate(self.supervisor.shard_id(), epoch, owner, &suspension)?;
        let value = match capability_reply_value(&wait.request.result_type, reply) {
            Ok(value) => value,
            Err(error) => {
                let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                    )),
                };
            }
        };
        let execution = {
            let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
            self.boundary.resume_capability_for_actor(
                &mut self.actors,
                &mut context,
                suspension,
                &value,
            )
        };
        let execution = match execution {
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
        };
        self.record_completion(owner, &execution);
        self.commit_epoch_operation(wait.completion)?;
        Ok(execution)
    }
}

/// Converts the closed worker term set into the generated boundary value requested by code.
fn capability_reply_value(
    expected: &TvmBoundaryType,
    reply: NativeBoundaryReplyTerm,
) -> Result<ReplValue, String> {
    let term = match reply {
        NativeBoundaryReplyTerm::Ok(term) => term,
        NativeBoundaryReplyTerm::Error {
            code,
            message,
            offset,
        } => {
            return Err(format!(
                "error[capability.{code}]: {message} at byte {offset}"
            ))
        }
    };
    let value = match (expected, term) {
        (TvmBoundaryType::Unit, NativeBoundaryTerm::Unit) => ReplValue::Unit,
        (TvmBoundaryType::Bool, NativeBoundaryTerm::Bool(value)) => ReplValue::Bool(value),
        (TvmBoundaryType::Int, NativeBoundaryTerm::Int(value)) => ReplValue::Int(value),
        (TvmBoundaryType::Float, NativeBoundaryTerm::Float(value)) => {
            ReplValue::Float(value.to_string())
        }
        (TvmBoundaryType::String | TvmBoundaryType::Json, NativeBoundaryTerm::Text(value)) => {
            ReplValue::String(value)
        }
        (TvmBoundaryType::Binary | TvmBoundaryType::Bytes, NativeBoundaryTerm::Bytes(value)) => {
            ReplValue::Bytes(value.into())
        }
        (expected, actual) => {
            return Err(format!(
                "error[execution_shard.capability_type]: expected {expected:?}, received {actual:?}"
            ))
        }
    };
    Ok(value)
}
