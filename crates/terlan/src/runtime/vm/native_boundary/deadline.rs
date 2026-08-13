use std::collections::BTreeMap;

use crate::{
    runtime::vm::{
        process::{VmProcessId, VmProcessState, VmProcessTable},
        scheduler::VmScheduler,
        timer::{VmTimerEvent, VmTimerId, VmTimerKind, VmTimerTable},
    },
    terlan_native_boundary::{
        request::RequestId, term::NativeBoundaryReplyTerm, worker::NativeBoundaryWorker,
    },
};

const NATIVE_BOUNDARY_PARK_REDUCTIONS: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VmPendingNativeBoundaryRequest {
    owner: VmProcessId,
    request_id: RequestId,
}

/// A parked NativeBoundary request protected by one VM-owned deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmScheduledNativeBoundaryRequest {
    pub(crate) timer_id: VmTimerId,
    pub(crate) owner: VmProcessId,
    pub(crate) request_id: RequestId,
    pub(crate) deadline_tick: u64,
}

/// Actor request and monotonic timeout admitted to the deadline queue.
#[derive(Clone, Copy)]
pub(crate) struct VmNativeBoundaryDeadlineStart {
    pub(crate) owner: VmProcessId,
    pub(crate) request_id: RequestId,
    pub(crate) now_tick: u64,
    pub(crate) timeout_ticks: u64,
}

#[cfg(test)]
impl VmNativeBoundaryDeadlineStart {
    pub(crate) fn new(
        owner: VmProcessId,
        request_id: RequestId,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Self {
        Self {
            owner,
            request_id,
            now_tick,
            timeout_ticks,
        }
    }
}

/// Terminal lifecycle result for a parked NativeBoundary request.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmNativeBoundaryDeadlineCompletion {
    Completed {
        timer_id: VmTimerId,
        request_id: RequestId,
    },
    TimedOut {
        timer_id: VmTimerId,
        request_id: RequestId,
        reply: NativeBoundaryReplyTerm,
    },
    Cancelled {
        timer_id: VmTimerId,
        request_id: RequestId,
        reply: NativeBoundaryReplyTerm,
    },
    OwnerExited {
        timer_id: VmTimerId,
        request_id: RequestId,
        reply: NativeBoundaryReplyTerm,
    },
}

/// VM-owned deadline and parking state around one NativeBoundary worker.
#[derive(Debug)]
pub(crate) struct VmNativeBoundaryDeadlineQueue {
    worker: NativeBoundaryWorker,
    pending: BTreeMap<VmTimerId, VmPendingNativeBoundaryRequest>,
    pending_by_owner: BTreeMap<VmProcessId, VmTimerId>,
    pending_by_request: BTreeMap<u64, VmTimerId>,
}

impl VmNativeBoundaryDeadlineQueue {
    /// Creates an empty deadline queue with bounded request credits.
    pub(crate) fn new(credit_limit: u64) -> Self {
        Self {
            worker: NativeBoundaryWorker::new(credit_limit),
            pending: BTreeMap::new(),
            pending_by_owner: BTreeMap::new(),
            pending_by_request: BTreeMap::new(),
        }
    }

    /// Begins worker accounting, parks the actor, and installs its deadline.
    pub(crate) fn start(
        &mut self,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        request: VmNativeBoundaryDeadlineStart,
    ) -> Result<VmScheduledNativeBoundaryRequest, String> {
        let VmNativeBoundaryDeadlineStart {
            owner,
            request_id,
            now_tick,
            timeout_ticks,
        } = request;
        if timeout_ticks == 0 {
            return Err("NativeBoundary timeout must be positive".to_string());
        }
        let deadline_tick = now_tick
            .checked_add(timeout_ticks)
            .ok_or_else(|| "NativeBoundary deadline overflow".to_string())?;
        self.require_startable(processes, owner, request_id)?;

        let timer_id = timers.start_one_shot(processes, owner, deadline_tick)?;
        if let Err(reply) = self.worker.begin_request(request_id) {
            timers
                .cancel(timer_id)
                .expect("new NativeBoundary deadline must remain cancellable");
            return Err(render_worker_rejection(&reply));
        }
        if let Err(error) =
            scheduler.charge_runtime_reductions(processes, owner, NATIVE_BOUNDARY_PARK_REDUCTIONS)
        {
            let _ = self.worker.cancel_request(request_id);
            timers
                .cancel(timer_id)
                .expect("new NativeBoundary deadline must remain cancellable");
            return Err(error);
        }
        processes
            .with_process_control_mutator(owner, |process| process.block())
            .expect("timer start proved NativeBoundary owner exists");
        let pending = VmPendingNativeBoundaryRequest { owner, request_id };
        self.pending.insert(timer_id, pending);
        self.pending_by_owner.insert(owner, timer_id);
        self.pending_by_request.insert(request_id.value, timer_id);
        Ok(VmScheduledNativeBoundaryRequest {
            timer_id,
            owner,
            request_id,
            deadline_tick,
        })
    }

    /// Completes a request only when cancellation proves its deadline is active.
    pub(crate) fn complete(
        &mut self,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        timer_id: VmTimerId,
    ) -> Result<VmNativeBoundaryDeadlineCompletion, String> {
        let pending = self.pending(timer_id)?;
        require_parked_owner(processes, pending.owner)?;
        timers.cancel(timer_id).map_err(|error| {
            format!(
                "NativeBoundary timer {} no longer owns completion: {error}",
                timer_id.as_u64()
            )
        })?;
        self.worker
            .finish_request(pending.request_id)
            .map_err(|reply| render_worker_rejection(&reply))?;
        self.remove_pending(timer_id, pending);
        scheduler.wake_process(processes, pending.owner)?;
        Ok(VmNativeBoundaryDeadlineCompletion::Completed {
            timer_id,
            request_id: pending.request_id,
        })
    }

    /// Proves that a driver completion still owns an active VM deadline.
    pub(crate) fn require_completable(
        &self,
        timers: &VmTimerTable,
        processes: &VmProcessTable,
        timer_id: VmTimerId,
    ) -> Result<(), String> {
        let pending = self.pending(timer_id)?;
        require_parked_owner(processes, pending.owner)?;
        timers
            .cancellation_token(timer_id)
            .map(|_| ())
            .map_err(|error| {
                format!(
                    "NativeBoundary timer {} no longer owns completion: {error}",
                    timer_id.as_u64()
                )
            })
    }

    /// Cancels a parked request and wakes its actor with a typed worker reply.
    pub(crate) fn cancel(
        &mut self,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        timer_id: VmTimerId,
    ) -> Result<VmNativeBoundaryDeadlineCompletion, String> {
        let pending = self.pending(timer_id)?;
        require_parked_owner(processes, pending.owner)?;
        let event = timers.cancel(timer_id)?;
        self.handle_timer_event(processes, scheduler, &event)?
            .ok_or_else(|| {
                format!(
                    "missing pending NativeBoundary request for timer {}",
                    timer_id.as_u64()
                )
            })
    }

    /// Applies one terminal VM timer event to a parked worker request.
    pub(crate) fn handle_timer_event(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        event: &VmTimerEvent,
    ) -> Result<Option<VmNativeBoundaryDeadlineCompletion>, String> {
        let timer_id = event.timer_id();
        let Some(pending) = self.pending.get(&timer_id).copied() else {
            return Ok(None);
        };
        validate_event(event, pending)?;
        if !matches!(event, VmTimerEvent::OwnerExited { .. }) {
            require_parked_owner(processes, pending.owner)?;
        }
        let completion = match event {
            VmTimerEvent::Fired { .. } | VmTimerEvent::DeadlineMissed { .. } => {
                let reply = self.worker.timeout_request(pending.request_id);
                wake_parked_owner(processes, scheduler, pending.owner)?;
                VmNativeBoundaryDeadlineCompletion::TimedOut {
                    timer_id,
                    request_id: pending.request_id,
                    reply,
                }
            }
            VmTimerEvent::Cancelled { .. } => {
                let reply = self.worker.cancel_request(pending.request_id);
                wake_parked_owner(processes, scheduler, pending.owner)?;
                VmNativeBoundaryDeadlineCompletion::Cancelled {
                    timer_id,
                    request_id: pending.request_id,
                    reply,
                }
            }
            VmTimerEvent::OwnerExited { .. } => {
                let reply = self.worker.cancel_request(pending.request_id);
                VmNativeBoundaryDeadlineCompletion::OwnerExited {
                    timer_id,
                    request_id: pending.request_id,
                    reply,
                }
            }
            VmTimerEvent::Coalesced { .. } | VmTimerEvent::Overflow { .. } => {
                unreachable!("interval-only events are rejected before worker mutation")
            }
        };
        self.remove_pending(timer_id, pending);
        Ok(Some(completion))
    }

    vm_capability_component! {
        /// Returns the number of actors currently parked on worker requests.
        pub(crate) fn pending_len(&self) -> usize {
            self.pending.len()
        }
    }

    /// Returns credits reserved by currently parked worker requests.
    #[cfg(test)]
    pub(crate) fn reserved_credits(&self) -> u64 {
        self.worker.reserved_credits()
    }

    vm_capability_component! {
        /// Finds the active VM deadline associated with a worker request.
        #[cfg(test)]
        pub(crate) fn timer_for_request(&self, request_id: RequestId) -> Option<VmTimerId> {
            self.pending_by_request.get(&request_id.value).copied()
        }

        /// Returns the owner and request identity parked behind one deadline.
        #[cfg(test)]
        pub(crate) fn request_for_timer(
            &self,
            timer_id: VmTimerId,
        ) -> Option<(VmProcessId, RequestId)> {
            self.pending
                .get(&timer_id)
                .map(|pending| (pending.owner, pending.request_id))
        }

        /// Returns active deadline identities in deterministic timer order.
        #[cfg(test)]
        pub(crate) fn pending_timer_ids(&self) -> Vec<VmTimerId> {
            self.pending.keys().copied().collect()
        }
    }

    fn require_startable(
        &self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        request_id: RequestId,
    ) -> Result<(), String> {
        let process = processes
            .get(owner)
            .ok_or_else(|| format!("cannot park missing process {}", owner.as_u64()))?;
        if process.state != VmProcessState::Runnable {
            return Err(format!(
                "cannot park non-runnable process {}",
                owner.as_u64()
            ));
        }
        if let Some(timer_id) = self.pending_by_owner.get(&owner) {
            return Err(format!(
                "process {} already has NativeBoundary request on timer {}",
                owner.as_u64(),
                timer_id.as_u64()
            ));
        }
        if let Some(timer_id) = self.pending_by_request.get(&request_id.value) {
            return Err(format!(
                "NativeBoundary request {} is already pending on timer {}",
                request_id.value,
                timer_id.as_u64()
            ));
        }
        Ok(())
    }

    fn pending(&self, timer_id: VmTimerId) -> Result<VmPendingNativeBoundaryRequest, String> {
        self.pending.get(&timer_id).copied().ok_or_else(|| {
            format!(
                "missing pending NativeBoundary request for timer {}",
                timer_id.as_u64()
            )
        })
    }

    fn remove_pending(&mut self, timer_id: VmTimerId, pending: VmPendingNativeBoundaryRequest) {
        self.pending.remove(&timer_id);
        self.pending_by_owner.remove(&pending.owner);
        self.pending_by_request.remove(&pending.request_id.value);
    }
}

fn validate_event(
    event: &VmTimerEvent,
    pending: VmPendingNativeBoundaryRequest,
) -> Result<(), String> {
    let timer_id = event.timer_id();
    let observed_owner = timer_event_owner(event);
    if observed_owner != pending.owner {
        return Err(format!(
            "NativeBoundary timer {} owner mismatch: expected {}, observed {}",
            timer_id.as_u64(),
            pending.owner.as_u64(),
            observed_owner.as_u64()
        ));
    }
    if timer_event_kind(event) != VmTimerKind::OneShot
        || matches!(
            event,
            VmTimerEvent::Coalesced { .. } | VmTimerEvent::Overflow { .. }
        )
    {
        return Err(format!(
            "NativeBoundary timer {} emitted invalid deadline outcome",
            timer_id.as_u64()
        ));
    }
    Ok(())
}

fn wake_parked_owner(
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
    owner: VmProcessId,
) -> Result<(), String> {
    scheduler.wake_process(processes, owner)
}

fn require_parked_owner(processes: &VmProcessTable, owner: VmProcessId) -> Result<(), String> {
    let process = processes
        .get(owner)
        .ok_or_else(|| format!("missing parked NativeBoundary process {}", owner.as_u64()))?;
    if process.state != VmProcessState::Blocked {
        return Err(format!(
            "NativeBoundary process {} is no longer parked",
            owner.as_u64()
        ));
    }
    Ok(())
}

fn timer_event_owner(event: &VmTimerEvent) -> VmProcessId {
    event.owner()
}

fn timer_event_kind(event: &VmTimerEvent) -> VmTimerKind {
    event.kind()
}

fn render_worker_rejection(reply: &NativeBoundaryReplyTerm) -> String {
    match reply {
        NativeBoundaryReplyTerm::Error { code, message, .. } => {
            format!("NativeBoundary worker rejected request: {code}: {message}")
        }
        NativeBoundaryReplyTerm::Ok(_) => {
            "NativeBoundary worker rejected request without an error reply".to_string()
        }
    }
}

#[cfg(test)]
#[path = "deadline_test.rs"]
#[cfg(test)]
mod deadline_test;
