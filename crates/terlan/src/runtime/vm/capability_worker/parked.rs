//! Worker transport lifecycle for continuations already parked by a shard.

use std::collections::{BTreeMap, VecDeque};

use crate::runtime::vm::process::VmProcessId;
use crate::terlan_native_boundary::capability_wire::{
    validate_capability_term_budget, validate_protocol_version, CapabilityHandle,
    CapabilityOutcome, CapabilityRequest, CapabilityResponse, CapabilityValue,
    CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::NativeBoundaryTerm;

use super::{
    VmCapabilityId, VmCapabilityRequestContext, VmCapabilityWorkerClient,
    VmCapabilityWorkerCompletion, VmCapabilityWorkerTransportEvent,
};

/// Minimum authority retained after an actor makes a capability call terminal.
#[derive(Clone, Debug, Eq, PartialEq)]
struct VmCapabilityCancelledContext {
    /// Process that owns any resources produced by the cancelled request.
    owner: VmProcessId,
    /// Capability authorized to dispose resources from the late result.
    capability: VmCapabilityId,
}

/// One native resource awaiting bounded disposal after a late completion.
#[derive(Clone, Debug, Eq, PartialEq)]
struct VmCapabilityLateDisposal {
    /// Cancelled request that produced the unreachable resource.
    source_request_id: RequestId,
    /// Process that owned the cancelled operation.
    owner: VmProcessId,
    /// Capability authorized to dispose the resource.
    capability: VmCapabilityId,
    /// Exact worker-local resource generation to dispose.
    handle: CapabilityHandle,
}

/// Bounded cleanup state retained independently from actor continuations.
#[derive(Default)]
pub(super) struct VmCapabilityLateCleanupState {
    /// Cleanup authority retained until a cancelled request replies or the worker exits.
    cancelled: BTreeMap<u64, VmCapabilityCancelledContext>,
    /// Late result resources waiting for available worker disposal credit.
    queued: VecDeque<VmCapabilityLateDisposal>,
    /// Internal disposal request mapped back to the cancelled source request.
    in_flight: BTreeMap<u64, RequestId>,
}

impl VmCapabilityLateCleanupState {
    /// Returns internal disposal requests currently consuming worker credit.
    pub(super) fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }

    /// Clears every correlation after terminal worker transport loss.
    fn clear(&mut self) {
        self.cancelled.clear();
        self.queued.clear();
        self.in_flight.clear();
    }
}

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
            return Err(
                "error[capability_worker.credit]: worker has no available request credit"
                    .to_string(),
            );
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
    pub(crate) fn poll_parked(&mut self) -> Result<Option<VmCapabilityWorkerCompletion>, String> {
        #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
        if self.deadlines.pending_len() != 0 {
            return Err("error[capability_worker.lifecycle]: parked polling cannot share a worker with deadline-owned calls".to_string());
        }
        let Some(event) = self.transport.try_event()? else {
            return Ok(None);
        };
        match event {
            VmCapabilityWorkerTransportEvent::Response(response) => {
                match self.apply_parked_response(response) {
                    Ok(completion) => Ok(Some(completion)),
                    Err(error) => {
                        self.close_parked_state();
                        Ok(Some(VmCapabilityWorkerCompletion::TransportFailed {
                            worker: self.identity.clone(),
                            error,
                            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
                            cancelled: Vec::new(),
                        }))
                    }
                }
            }
            VmCapabilityWorkerTransportEvent::Closed => {
                self.close_parked_state();
                Ok(Some(VmCapabilityWorkerCompletion::TransportClosed {
                    worker: self.identity.clone(),
                    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
                    cancelled: Vec::new(),
                }))
            }
            VmCapabilityWorkerTransportEvent::Failed(error) => {
                self.close_parked_state();
                Ok(Some(VmCapabilityWorkerCompletion::TransportFailed {
                    worker: self.identity.clone(),
                    error,
                    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
            return Err(
                "error[capability_worker.owner]: parked request owner mismatch".to_string(),
            );
        }
        self.late_cleanup.cancelled.insert(
            request_id.value,
            VmCapabilityCancelledContext {
                owner,
                capability: context.capability.clone(),
            },
        );
        if let Err(error) = self.transport.try_send(CapabilityRequest::Cancel {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: request_id.value,
            owner_id: owner.as_u64(),
        }) {
            self.late_cleanup.cancelled.remove(&request_id.value);
            self.parked_contexts
                .insert(request_id.value, (owner, context));
            return Err(error);
        }
        Ok(())
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
                if let Some(source_request_id) =
                    self.late_cleanup.in_flight.remove(&request_id.value)
                {
                    self.validate_late_disposal_outcome(request_id, outcome)?;
                    self.schedule_late_disposals()?;
                    return Ok(VmCapabilityWorkerCompletion::StaleReply {
                        worker: self.identity.clone(),
                        request_id: source_request_id,
                    });
                }
                let Some((_, context)) = self.parked_contexts.remove(&request_id.value) else {
                    if let Some(cancelled) = self.late_cleanup.cancelled.remove(&request_id.value) {
                        self.queue_late_disposals(request_id, cancelled, &outcome);
                        self.schedule_late_disposals()?;
                    }
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

    /// Queues each unique owned handle from one cancelled successful result.
    fn queue_late_disposals(
        &mut self,
        source_request_id: RequestId,
        cancelled: VmCapabilityCancelledContext,
        outcome: &CapabilityOutcome,
    ) {
        let CapabilityOutcome::Ok { value } = outcome else {
            return;
        };
        let mut unique = std::collections::BTreeSet::new();
        for handle in value.owned_handles() {
            if unique.insert((handle.id, handle.generation)) {
                self.late_cleanup
                    .queued
                    .push_back(VmCapabilityLateDisposal {
                        source_request_id,
                        owner: cancelled.owner,
                        capability: cancelled.capability.clone(),
                        handle,
                    });
            }
        }
    }

    /// Publishes queued late-result disposals without exceeding worker credit.
    fn schedule_late_disposals(&mut self) -> Result<(), String> {
        while self.pending_len() < self.remote_credit_limit as usize {
            let Some(disposal) = self.late_cleanup.queued.pop_front() else {
                break;
            };
            let request_id = self.allocate_request_id()?;
            let request = CapabilityRequest::Dispose {
                version: CAPABILITY_PROTOCOL_VERSION,
                request_id: request_id.value,
                owner_id: disposal.owner.as_u64(),
                capability: disposal.capability.as_str().to_string(),
                handle: disposal.handle,
            };
            if let Err(error) = self.transport.try_send(request) {
                self.late_cleanup.queued.push_front(disposal);
                return Err(error);
            }
            self.late_cleanup
                .in_flight
                .insert(request_id.value, disposal.source_request_id);
        }
        Ok(())
    }

    /// Requires an internal late-result disposal to finish successfully.
    fn validate_late_disposal_outcome(
        &self,
        request_id: RequestId,
        outcome: CapabilityOutcome,
    ) -> Result<(), String> {
        match outcome {
            CapabilityOutcome::Ok {
                value: CapabilityValue::Unit,
            } => Ok(()),
            CapabilityOutcome::Ok { .. } => Err(format!(
                "error[capability_worker.cleanup]: disposal request {} returned a non-unit value",
                request_id.value
            )),
            CapabilityOutcome::Error { code, message, .. } => Err(format!(
                "error[capability_worker.cleanup]: disposal request {} failed with {code}: {message}",
                request_id.value
            )),
        }
    }

    /// Closes transport and releases all VM-side correlation state.
    fn close_parked_state(&mut self) {
        self.transport.close();
        self.parked_contexts.clear();
        self.late_cleanup.clear();
    }
}
