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
            VmCapabilityId::new(self.request.capability.clone())?,
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
    /// Claims one spawned actor capability suspension for helper dispatch.
    pub(crate) fn take_resident_capability_call(
        &mut self,
    ) -> Result<Option<(VmProcessId, PureNativeSuspension, PureNativeCapabilityWait)>, String> {
        let Some(suspension) = self.execution.take_resident_capability_suspension() else {
            return Ok(None);
        };
        let owner = VmProcessId::from_native_owner(suspension.owner_id())?;
        let wait = match self.begin_capability_call(owner, &suspension) {
            Ok(wait) => wait,
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
        Ok(Some((owner, suspension, wait)))
    }

    /// Decodes and admits one generated capability suspension under the active epoch.
    pub(crate) fn begin_capability_call(
        &mut self,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<PureNativeCapabilityWait, String> {
        let epoch = self.require_active_epoch("begin_capability_call")?;
        let request = {
            let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
            let request = self
                .boundary
                .capability_request_for_actor(&context, suspension)?;
            context.collect_parked_owner_at_safepoint()?;
            request
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
        self.resume_capability_value_call(owner, suspension, wait, value)
    }

    /// Applies one package-helper value without routing it through the closed
    /// built-in capability term codec.
    pub(crate) fn resume_capability_value_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        value: ReplValue,
    ) -> Result<PureNativeExecution, String> {
        let epoch = self.require_active_epoch("resume_capability_value_call")?;
        wait.validate(self.supervisor.shard_id(), epoch, owner, &suspension)?;
        let result_type = wait.request.result_type.clone();
        let execution = {
            let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
            self.boundary.resume_capability_value_for_actor(
                &mut self.actors,
                &mut context,
                suspension,
                &result_type,
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

    /// Resumes a spawned actor capability and drives it until it parks or exits.
    pub(crate) fn resume_resident_capability_value_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        value: ReplValue,
    ) -> Result<bool, String> {
        let execution = self.resume_capability_value_call(owner, suspension, wait, value)?;
        self.drive_resident_capability_execution(owner, execution)
    }

    /// Resumes a spawned built-in capability and drives it until it parks or exits.
    pub(crate) fn resume_resident_capability_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        reply: NativeBoundaryReplyTerm,
    ) -> Result<bool, String> {
        let execution = self.resume_capability_call(owner, suspension, wait, reply)?;
        self.drive_resident_capability_execution(owner, execution)
    }

    /// Settles one capability completion inside its spawned actor lifecycle.
    fn drive_resident_capability_execution(
        &mut self,
        owner: VmProcessId,
        execution: PureNativeExecution,
    ) -> Result<bool, String> {
        let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
        self.boundary
            .drive_resident_execution(&mut self.actors, &mut context, owner, execution)
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
        (TvmBoundaryType::Managed(_), term @ NativeBoundaryTerm::OptionalText(_))
        | (TvmBoundaryType::Managed(_), term @ NativeBoundaryTerm::Record { .. })
        | (TvmBoundaryType::Managed(_), term @ NativeBoundaryTerm::List(_)) => {
            managed_capability_term(term)?
        }
        (expected, actual) => {
            return Err(format!(
                "error[execution_shard.capability_type]: expected {expected:?}, received {actual:?}"
            ))
        }
    };
    Ok(value)
}

fn managed_capability_term(term: NativeBoundaryTerm) -> Result<ReplValue, String> {
    match term {
        NativeBoundaryTerm::Unit => Ok(ReplValue::Unit),
        NativeBoundaryTerm::Text(value) => Ok(ReplValue::String(value)),
        NativeBoundaryTerm::Bytes(value) => Ok(ReplValue::Bytes(value.into())),
        NativeBoundaryTerm::Int(value) => Ok(ReplValue::Int(value)),
        NativeBoundaryTerm::Float(value) => Ok(ReplValue::Float(value.to_string())),
        NativeBoundaryTerm::Bool(value) => Ok(ReplValue::Bool(value)),
        NativeBoundaryTerm::Atom(value) => Ok(ReplValue::Atom(value)),
        NativeBoundaryTerm::OptionalText(value) => Ok(match value {
            Some(value) => ReplValue::Record {
                name: "Some".to_string(),
                fields: vec![("value".to_string(), ReplValue::String(value))],
            },
            None => ReplValue::Record {
                name: "None".to_string(),
                fields: Vec::new(),
            },
        }),
        NativeBoundaryTerm::Record { name, fields } => Ok(ReplValue::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| managed_capability_term(value).map(|value| (name, value)))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryTerm::List(values) => Ok(ReplValue::List(
            values
                .into_iter()
                .map(managed_capability_term)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        unsupported => Err(format!(
            "error[execution_shard.capability_type]: managed result cannot contain `{unsupported:?}`"
        )),
    }
}
