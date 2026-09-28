//! Actor-scoped database workers retain continuations outside generated code.
use super::{postgres, VmRuntimeResult};
use crate::runtime::vm::{
    capability_worker::*,
    process::VmProcessId,
    pure_native::{PureNativeCapabilityWait, PureNativeSuspension},
};
use crate::terlan_native_boundary::{
    metadata::NativeBoundaryExecutionProfile,
    term::{NativeBoundaryReplyTerm, NativeBoundaryTerm},
};
use std::{
    collections::BTreeMap,
    task::Waker,
    time::{Duration, Instant},
};

/// VM-owned execution state retained while a database worker runs.
pub(super) struct Continuation {
    pub(super) suspension: PureNativeSuspension,
    pub(super) wait: PureNativeCapabilityWait,
}
pub(super) struct Pending<T = Continuation> {
    pub(super) owner: VmProcessId,
    pub(super) payload: T,
    pub(super) projection: postgres::Projection,
    pub(super) context: VmCapabilityRequestContext,
}
pub(super) struct Completion<T> {
    pub(super) pending: Pending<T>,
    pub(super) reply: NativeBoundaryReplyTerm,
}
struct Worker<T> {
    pump: VmCapabilityWorkerEventPump<Pending<T>>,
    deadline: Option<Instant>,
}
impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        let _ = self.pump.shutdown();
    }
}
/// A worker never changes owners or restarts with live source handles.
pub(super) struct Workers<T = Continuation> {
    owners: BTreeMap<u64, Option<Worker<T>>>,
    executable: Option<std::path::PathBuf>,
}
impl<T> Default for Workers<T> {
    fn default() -> Self {
        Self {
            owners: BTreeMap::new(),
            executable: None,
        }
    }
}
impl<T> Workers<T> {
    pub(super) fn select_worker(&mut self, executable: std::path::PathBuf) {
        self.executable.get_or_insert(executable);
    }

    pub(super) fn submit(
        &mut self,
        operation: &str,
        arguments: Vec<NativeBoundaryTerm>,
        pending: Pending<T>,
    ) -> VmRuntimeResult<()> {
        let owner = pending.owner.as_u64();
        if !self.owners.contains_key(&owner) {
            if self.owners.len() >= 16 {
                return Err("error[postgres.resource_limit]: at most 16 database worker owners are admitted".into());
            }
            let path = match &self.executable {
                Some(path) => path.clone(),
                None => {
                    let executable = std::env::current_exe()
                        .map_err(|_| "error[postgres.worker]: cannot locate installed worker")?;
                    super::storage_transport::installed_worker_path(&executable)?
                }
            };
            let policy =
                VmCapabilityWorkerPolicy::new(path, NativeBoundaryExecutionProfile::CrashIsolated)?
                    .allow("postgres")
                    .admit_worker_class("blocking")
                    .with_credit_limit(1)?;
            let identity = VmCapabilityWorkerIdentity::new(
                VmCapabilityWorkerId::new(format!("postgres-{owner}"))?,
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
        let worker = self.owners.get_mut(&owner).and_then(Option::as_mut).ok_or(
            "error[postgres.indeterminate]: database worker was lost; actor resources are revoked",
        )?;
        if worker.deadline.is_some() {
            return Err(
                "error[postgres.worker]: owner already has a pending database request".into(),
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
    pub(super) fn poll(&mut self) -> VmRuntimeResult<Option<Completion<T>>> {
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
                        reply: lost("database deadline expired"),
                    }));
                }
                continue;
            }
            while let Some(event) = worker.pump.poll()? {
                match event {
                    VmCapabilityWorkerEventPumpEvent::Completed {
                        context,
                        reply,
                        payload,
                        ..
                    } => {
                        worker.deadline = None;
                        let mismatch = context != payload.context;
                        if mismatch {
                            *slot = None;
                        }
                        return Ok(Some(Completion {
                            reply: if mismatch {
                                lost("database completion authority mismatch")
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
                                reply: lost("database worker lost"),
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
}
fn lost(reason: &str) -> NativeBoundaryReplyTerm {
    NativeBoundaryReplyTerm::Error {
        code: "postgres.indeterminate".into(),
        message: format!(
            "{reason}; a database mutation may have occurred; do not retry automatically"
        ),
        offset: 0,
    }
}

#[cfg(test)]
#[path = "postgres_transport_test.rs"]
mod tests;
