//! Bounded correlation between worker assignments and fixed-owner payloads.

use std::collections::BTreeMap;
use std::task::Waker;

use crate::runtime::vm::process::VmProcessId;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

use super::{
    VmCapabilityRequestContext, VmCapabilityWorkerCompletion, VmCapabilityWorkerIdentity,
    VmCapabilityWorkerParkedRequest, VmCapabilityWorkerPool,
};

/// One event emitted after polling a scheduler-local capability worker pool.
pub(crate) enum VmCapabilityWorkerEventPumpEvent<Payload> {
    /// A correlated worker reply and the exact retained owner payload.
    Completed {
        /// Exact assignment consumed by this completion.
        assignment: VmCapabilityWorkerParkedRequest,
        /// Capability and shard-epoch authority returned by the worker client.
        context: VmCapabilityRequestContext,
        /// Stable worker outcome.
        reply: NativeBoundaryReplyTerm,
        /// Scheduler-owned continuation envelope retained during external work.
        payload: Payload,
    },
    /// A worker process became terminal and all its retained payloads were returned.
    WorkerLost {
        /// Exact failed or stopped worker generation.
        worker: VmCapabilityWorkerIdentity,
        /// Stable failure detail suitable for actor cancellation diagnostics.
        reason: String,
        /// Every assignment and payload that can no longer complete on this worker.
        pending: Vec<(VmCapabilityWorkerParkedRequest, Payload)>,
    },
    /// A protocol event did not own a live scheduler payload.
    Ignored {
        _completion: VmCapabilityWorkerCompletion,
    },
}

/// VM-owned worker event pump retaining fixed-owner payloads under bounded credits.
pub(crate) struct VmCapabilityWorkerEventPump<Payload> {
    pool: VmCapabilityWorkerPool,
    pending: BTreeMap<(String, u64, u64), (VmCapabilityWorkerParkedRequest, Payload)>,
}

impl<Payload> VmCapabilityWorkerEventPump<Payload> {
    /// Creates one empty event pump around an already bounded worker pool.
    pub(crate) fn new(pool: VmCapabilityWorkerPool) -> Self {
        Self {
            pool,
            pending: BTreeMap::new(),
        }
    }

    /// Returns the number of owner payloads retained outside actor state.
    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Returns currently available pool credits.
    #[cfg(test)]
    pub(crate) fn available_capacity(&self) -> u64 {
        self.pool.available_capacity()
    }

    /// Arms the caller before it checks for a queued transport completion.
    pub(crate) fn register_event_waker(&self, waker: &Waker) {
        self.pool.register_event_waker(waker);
    }

    /// Submits one already-parked call and retains its owner payload atomically.
    pub(crate) fn submit(
        &mut self,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
        payload: Payload,
    ) -> Result<VmCapabilityWorkerParkedRequest, (String, Payload)> {
        let assignment = match self
            .pool
            .start_parked_call(owner, context, operation, arguments)
        {
            Ok(assignment) => assignment,
            Err(error) => return Err((error, payload)),
        };
        let key = assignment_key(&assignment);
        if self
            .pending
            .insert(key, (assignment.clone(), payload))
            .is_some()
        {
            panic!("capability worker reused a live assignment identity");
        }
        Ok(assignment)
    }

    /// Polls at most one worker event without acquiring an actor mutator lease.
    pub(crate) fn poll(
        &mut self,
    ) -> Result<Option<VmCapabilityWorkerEventPumpEvent<Payload>>, String> {
        let Some(completion) = self.pool.poll_parked()? else {
            return Ok(None);
        };
        let event = match completion {
            VmCapabilityWorkerCompletion::Reply {
                worker,
                request_id,
                context,
                reply,
            } => {
                let key = (
                    worker.id.as_str().to_string(),
                    worker.generation.as_u64(),
                    request_id.value,
                );
                let Some((assignment, payload)) = self.pending.remove(&key) else {
                    return Ok(Some(VmCapabilityWorkerEventPumpEvent::Ignored {
                        _completion: VmCapabilityWorkerCompletion::StaleReply {
                            worker,
                            request_id,
                        },
                    }));
                };
                VmCapabilityWorkerEventPumpEvent::Completed {
                    assignment,
                    context,
                    reply,
                    payload,
                }
            }
            VmCapabilityWorkerCompletion::TransportClosed { worker, .. } => {
                self.worker_lost(worker, "capability worker transport closed".to_string())
            }
            VmCapabilityWorkerCompletion::TransportFailed { worker, error, .. } => self
                .worker_lost(
                    worker,
                    format!("capability worker transport failed: {error}"),
                ),
            VmCapabilityWorkerCompletion::ShutdownAcknowledged { worker } => {
                self.worker_lost(worker, "capability worker stopped".to_string())
            }
            completion => VmCapabilityWorkerEventPumpEvent::Ignored {
                _completion: completion,
            },
        };
        Ok(Some(event))
    }

    /// Cancels one assignment and returns its retained owner payload exactly once.
    pub(crate) fn cancel(
        &mut self,
        assignment: &VmCapabilityWorkerParkedRequest,
    ) -> Result<Payload, (String, Payload)> {
        let payload = self
            .pending
            .remove(&assignment_key(assignment))
            .map(|(_, payload)| payload)
            .expect("an admitted capability assignment retains one owner payload");
        match self.pool.cancel_parked(assignment) {
            Ok(()) => Ok(payload),
            Err(error) => Err((error, payload)),
        }
    }

    /// Cancels and returns every retained payload before scheduler shutdown.
    pub(crate) fn shutdown(
        &mut self,
    ) -> (Vec<(VmCapabilityWorkerParkedRequest, Payload)>, Vec<String>) {
        let assignments = self
            .pending
            .values()
            .map(|(assignment, _)| assignment.clone())
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for assignment in &assignments {
            if let Err(error) = self.pool.cancel_parked(assignment) {
                errors.push(error);
            }
        }
        errors.extend(self.pool.shutdown_all());
        let pending = std::mem::take(&mut self.pending).into_values().collect();
        (pending, errors)
    }

    /// Drains payloads attributed to one exact worker generation after termination.
    fn worker_lost(
        &mut self,
        worker: VmCapabilityWorkerIdentity,
        reason: String,
    ) -> VmCapabilityWorkerEventPumpEvent<Payload> {
        let prefix = (worker.id.as_str().to_string(), worker.generation.as_u64());
        let keys = self
            .pending
            .keys()
            .filter(|(id, generation, _)| id == &prefix.0 && *generation == prefix.1)
            .cloned()
            .collect::<Vec<_>>();
        let pending = keys
            .into_iter()
            .filter_map(|key| self.pending.remove(&key))
            .collect();
        VmCapabilityWorkerEventPumpEvent::WorkerLost {
            worker,
            reason,
            pending,
        }
    }
}

/// Builds one generation-qualified correlation key from a pool assignment.
fn assignment_key(request: &VmCapabilityWorkerParkedRequest) -> (String, u64, u64) {
    (
        request.worker.id.as_str().to_string(),
        request.worker.generation.as_u64(),
        request.request_id.value,
    )
}
