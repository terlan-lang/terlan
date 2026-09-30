//! Actor-scoped resource workers retain continuations outside generated code.
use super::VmRuntimeResult;
use crate::runtime::vm::{capability_worker::*, process::VmProcessId};
use crate::terlan_native_boundary::{
    metadata::NativeBoundaryExecutionProfile,
    term::{NativeBoundaryReplyTerm, NativeBoundaryTerm},
};
use std::{
    collections::BTreeMap,
    task::Waker,
    time::{Duration, Instant},
};

pub(super) struct Pending<T, P> {
    pub(super) owner: VmProcessId,
    pub(super) payload: T,
    pub(super) projection: P,
    pub(super) context: VmCapabilityRequestContext,
}
pub(super) struct Completion<T, P> {
    pub(super) pending: Pending<T, P>,
    pub(super) reply: NativeBoundaryReplyTerm,
}
struct Worker<T, P> {
    pump: VmCapabilityWorkerEventPump<Pending<T, P>>,
    deadline: Option<Instant>,
}
impl<T, P> Drop for Worker<T, P> {
    fn drop(&mut self) {
        let _ = self.pump.shutdown();
    }
}
/// A worker never changes owners or restarts with live source handles.
pub(super) struct Workers<T, P> {
    owners: BTreeMap<u64, Option<Worker<T, P>>>,
    executable: Option<std::path::PathBuf>,
    policy: Policy,
}
impl<T, P> Workers<T, P> {
    pub(super) fn new(policy: Policy) -> Self {
        Self {
            owners: BTreeMap::new(),
            executable: None,
            policy,
        }
    }
}
impl<T, P> Workers<T, P> {
    pub(super) fn select_worker(&mut self, executable: std::path::PathBuf) {
        self.executable.get_or_insert(executable);
    }

    pub(super) fn submit(
        &mut self,
        operation: &str,
        arguments: Vec<NativeBoundaryTerm>,
        pending: Pending<T, P>,
    ) -> VmRuntimeResult<()> {
        let owner = pending.owner.as_u64();
        if !self.owners.contains_key(&owner) {
            if self.owners.len() >= 16 {
                return Err(format!(
                    "error[{}]: at most 16 resource worker owners are admitted",
                    self.policy.capacity_error_code
                )
                .into());
            }
            let path = match &self.executable {
                Some(path) => path.clone(),
                None => {
                    let executable = std::env::current_exe().map_err(|_| {
                        "error[resource_worker.path]: cannot locate installed worker"
                    })?;
                    super::storage_transport::installed_worker_path(&executable)?
                }
            };
            let policy =
                VmCapabilityWorkerPolicy::new(path, NativeBoundaryExecutionProfile::CrashIsolated)?
                    .allow(self.policy.capability)
                    .admit_worker_class(self.policy.worker_class)
                    .with_credit_limit(1)?;
            let identity = VmCapabilityWorkerIdentity::new(
                VmCapabilityWorkerId::new(format!("{}-{owner}", self.policy.identity))?,
                VmCapabilityWorkerGeneration::new(1)?,
            );
            let client = VmCapabilityWorkerClient::spawn(identity, policy)?;
            let pool =
                VmCapabilityWorkerPool::new(vec![VmCapabilityWorkerPoolSlot::new(client, 1)?])?;
            self.owners.insert(
                owner,
                Some(Worker {
                    pump: VmCapabilityWorkerEventPump::new(pool),
                    deadline: None,
                }),
            );
        }
        let worker = self
            .owners
            .get_mut(&owner)
            .and_then(Option::as_mut)
            .ok_or_else(|| {
                format!(
                    "error[{}]: worker was lost; actor resources are revoked",
                    self.policy.error_code
                )
            })?;
        if worker.deadline.is_some() {
            return Err(
                "error[resource_worker.pending]: owner already has a pending resource request"
                    .into(),
            );
        }
        worker
            .pump
            .submit(
                pending.owner,
                pending.context.clone(),
                operation,
                arguments,
                pending,
            )
            .map_err(|(error, _)| error)?;
        worker.deadline = Some(Instant::now() + Duration::from_secs(30));
        Ok(())
    }
    pub(super) fn register_owner_waker(&self, owner: VmProcessId, waker: &Waker) {
        if let Some(Some(worker)) = self.owners.get(&owner.as_u64()) {
            worker.pump.register_event_waker(waker);
        }
    }
    pub(super) fn register_waker(&self, waker: &Waker) {
        for worker in self
            .owners
            .values()
            .filter_map(Option::as_ref)
            .filter(|worker| worker.deadline.is_some())
        {
            worker.pump.register_event_waker(waker);
        }
    }
    pub(super) fn poll(&mut self) -> VmRuntimeResult<Option<Completion<T, P>>> {
        'owners: for slot in self.owners.values_mut() {
            let Some(worker) = slot.as_mut() else {
                continue;
            };
            if worker
                .deadline
                .is_some_and(|deadline| deadline <= Instant::now())
            {
                let (pending, _) = worker.pump.shutdown();
                // Dropping the transport kills and reaps the child, revoking every handle.
                *slot = None;
                if let Some((_, pending)) = pending.into_iter().next() {
                    return Ok(Some(Completion {
                        pending,
                        reply: self.policy.lost("resource deadline expired"),
                    }));
                }
                continue;
            }
            while let Some(event) = worker.pump.poll()? {
                match event {
                    VmCapabilityWorkerEventPumpEvent::Completed {
                        assignment,
                        context,
                        reply,
                        payload,
                        ..
                    } => {
                        worker.deadline = None;
                        let mismatch =
                            context != payload.context || assignment.owner != payload.owner;
                        if mismatch {
                            *slot = None;
                        }
                        return Ok(Some(Completion {
                            reply: if mismatch {
                                self.policy.lost("resource completion authority mismatch")
                            } else {
                                reply
                            },
                            pending: payload,
                        }));
                    }
                    VmCapabilityWorkerEventPumpEvent::WorkerLost { pending, .. } => {
                        *slot = None;
                        if let Some((_, pending)) = pending.into_iter().next() {
                            return Ok(Some(Completion {
                                pending,
                                reply: self.policy.lost("resource worker lost"),
                            }));
                        }
                        // An idle worker's loss must not hide another owner's ready reply.
                        continue 'owners;
                    }
                    VmCapabilityWorkerEventPumpEvent::Ignored { .. } => {}
                }
            }
        }
        Ok(None)
    }
    pub(super) fn idle_duration(&self) -> Duration {
        self.owners
            .values()
            .filter_map(Option::as_ref)
            .filter_map(|worker| worker.deadline)
            .min()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(30))
    }
    pub(super) fn close_owner(&mut self, owner: u64) {
        self.owners.remove(&owner);
    }

    pub(super) fn revoke_owner(&mut self, owner: u64) {
        if let Some(slot) = self.owners.get_mut(&owner) {
            *slot = None;
        }
    }
}
/// Closed worker policy selected by the host, not by source call arguments.
pub(super) struct Policy {
    pub(super) capability: &'static str,
    pub(super) worker_class: &'static str,
    pub(super) identity: &'static str,
    pub(super) error_code: &'static str,
    pub(super) capacity_error_code: &'static str,
    pub(super) loss_context: &'static str,
}
impl Policy {
    fn lost(&self, reason: &str) -> NativeBoundaryReplyTerm {
        NativeBoundaryReplyTerm::Error {
            code: self.error_code.into(),
            message: format!("{reason}; {}", self.loss_context),
            offset: 0,
        }
    }
}

#[cfg(test)]
#[path = "resource_transport_test.rs"]
mod tests;
