//! Worker transport lifecycle for continuations already parked by a shard.

use crate::runtime::vm::process::VmProcessId;
use crate::terlan_native_boundary::capability_wire::{
    validate_capability_term_budget, validate_protocol_version, CapabilityRequest,
    CapabilityResponse, CapabilityValue, CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::NativeBoundaryTerm;

use super::{
    VmCapabilityRequestContext, VmCapabilityWorkerClient, VmCapabilityWorkerCompletion,
    VmCapabilityWorkerTransportEvent,
};

impl VmCapabilityWorkerClient {
    /// Starts one operation for a generated continuation already parked by its shard.
    pub(crate) fn start_parked_call(
        &mut self,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
    ) -> Result<RequestId, String> {
        self.require_capability(&context.capability)?;
        let arguments = arguments
            .into_iter()
            .map(CapabilityValue::from_term)
            .collect::<Vec<_>>();
        validate_capability_term_budget(&arguments)?;
        if self.pending_len() as u64 >= self.remote_credit_limit {
            return Err("error[capability_worker.credit]: worker has no available request credit"
                .to_string());
        }
        let request_id = self.allocate_request_id()?;
        self.parked_contexts
            .insert(request_id.value, (owner, context.clone()));
        let request = CapabilityRequest::Call {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: request_id.value,
            owner_id: owner.as_u64(),
            capability: context.capability.as_str().to_string(),
            operation: operation.into(),
            arguments,
        };
        if let Err(error) = self.transport.try_send(request) {
            self.parked_contexts.remove(&request_id.value);
            return Err(error);
        }
        Ok(request_id)
    }

    /// Polls one response for an already parked generated continuation.
    pub(crate) fn poll_parked(
        &mut self,
    ) -> Result<Option<VmCapabilityWorkerCompletion>, String> {
        if self.deadlines.pending_len() != 0 {
            return Err("error[capability_worker.lifecycle]: parked polling cannot share a worker with deadline-owned calls".to_string());
        }
        let Some(event) = self.transport.try_event()? else {
            return Ok(None);
        };
        match event {
            VmCapabilityWorkerTransportEvent::Response(response) => {
                self.apply_parked_response(response).map(Some)
            }
            VmCapabilityWorkerTransportEvent::Closed => {
                self.transport.close();
                self.parked_contexts.clear();
                Ok(Some(VmCapabilityWorkerCompletion::TransportClosed {
                    worker: self.identity.clone(),
                    cancelled: Vec::new(),
                }))
            }
            VmCapabilityWorkerTransportEvent::Failed(error) => {
                self.transport.close();
                self.parked_contexts.clear();
                Ok(Some(VmCapabilityWorkerCompletion::TransportFailed {
                    worker: self.identity.clone(),
                    error,
                    cancelled: Vec::new(),
                }))
            }
        }
    }

    /// Cancels one exact already-parked request without mutating actor state.
    pub(crate) fn cancel_parked(
        &mut self,
        owner: VmProcessId,
        request_id: RequestId,
    ) -> Result<(), String> {
        let Some((expected_owner, context)) = self.parked_contexts.remove(&request_id.value) else {
            return Err(format!(
                "error[capability_worker.request_missing]: request {} is not pending",
                request_id.value
            ));
        };
        if expected_owner != owner {
            self.parked_contexts
                .insert(request_id.value, (expected_owner, context));
            return Err("error[capability_worker.owner]: parked request owner mismatch".to_string());
        }
        self.transport.try_send(CapabilityRequest::Cancel {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: request_id.value,
            owner_id: owner.as_u64(),
        })
    }

    /// Correlates one worker response without touching scheduler-owned actor tables.
    fn apply_parked_response(
        &mut self,
        response: CapabilityResponse,
    ) -> Result<VmCapabilityWorkerCompletion, String> {
        match response {
            CapabilityResponse::Reply {
                version,
                request_id,
                reserved_credits,
                available_credits,
                outcome,
            } => {
                validate_protocol_version(version)?;
                self.validate_remote_credits(reserved_credits, available_credits)?;
                let request_id = RequestId { value: request_id };
                let Some((_, context)) = self.parked_contexts.remove(&request_id.value) else {
                    return Ok(VmCapabilityWorkerCompletion::StaleReply {
                        worker: self.identity.clone(),
                        request_id,
                    });
                };
                Ok(VmCapabilityWorkerCompletion::Reply {
                    worker: self.identity.clone(),
                    request_id,
                    context,
                    reply: outcome.into_reply(),
                })
            }
            CapabilityResponse::CancelAck {
                version,
                request_id,
                accepted,
            } => {
                validate_protocol_version(version)?;
                Ok(VmCapabilityWorkerCompletion::CancelAcknowledged {
                    worker: self.identity.clone(),
                    request_id: RequestId { value: request_id },
                    accepted,
                })
            }
            CapabilityResponse::ShutdownAck { version } => {
                validate_protocol_version(version)?;
                Ok(VmCapabilityWorkerCompletion::ShutdownAcknowledged {
                    worker: self.identity.clone(),
                })
            }
        }
    }
}
