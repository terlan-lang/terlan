//! Actor parking and wakeup bridge for isolated capability workers.

use super::{VmActorRuntime, VmProcessId};
use crate::{
    runtime::vm::{
        capability_worker::{
            VmCapabilityRequestContext, VmCapabilityWorkerClient, VmCapabilityWorkerCompletion,
            VmCapabilityWorkerRuntime, VmCapabilityWorkerTerminal,
        },
        native_boundary::deadline::VmScheduledNativeBoundaryRequest,
        timer::{VmTimerEvent, VmTimerId},
    },
    terlan_native_boundary::{capability_wire::CapabilityHandle, term::NativeBoundaryTerm},
};

impl VmActorRuntime {
    /// Submits one explicitly admitted capability call and parks its actor.
    ///
    /// The caller owns the worker process as a VM service. This bridge lends
    /// only the shard-local timer, process, and scheduler tables needed for
    /// exactly-once parking and wakeup; the execution shard never owns worker
    /// transport or lets a worker load application code.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_capability_call(
        &mut self,
        worker: &mut VmCapabilityWorkerClient,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmScheduledNativeBoundaryRequest, String> {
        let mut runtime = self.capability_worker_runtime();
        worker.start_call(
            &mut runtime,
            owner,
            context,
            operation,
            arguments,
            now_tick,
            timeout_ticks,
        )
    }

    /// Submits disposal of one actor-owned external capability handle.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_capability_dispose(
        &mut self,
        worker: &mut VmCapabilityWorkerClient,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        handle: CapabilityHandle,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmScheduledNativeBoundaryRequest, String> {
        let mut runtime = self.capability_worker_runtime();
        worker.start_dispose(
            &mut runtime,
            owner,
            context,
            handle,
            now_tick,
            timeout_ticks,
        )
    }

    /// Polls one nonblocking worker event and applies it to actor state.
    pub(crate) fn poll_capability_worker(
        &mut self,
        worker: &mut VmCapabilityWorkerClient,
    ) -> Result<Option<VmCapabilityWorkerCompletion>, String> {
        worker.poll(&mut self.capability_worker_runtime())
    }

    /// Cancels one parked capability call by its VM-owned timer identity.
    pub(crate) fn cancel_capability_call(
        &mut self,
        worker: &mut VmCapabilityWorkerClient,
        timer_id: VmTimerId,
    ) -> Result<VmCapabilityWorkerTerminal, String> {
        worker.cancel(&mut self.capability_worker_runtime(), timer_id)
    }

    /// Applies a timer or owner-exit event to one capability-worker request.
    pub(crate) fn handle_capability_timer_event(
        &mut self,
        worker: &mut VmCapabilityWorkerClient,
        event: &VmTimerEvent,
    ) -> Result<Option<VmCapabilityWorkerTerminal>, String> {
        worker.handle_timer_event(&mut self.capability_worker_runtime(), event)
    }

    /// Borrows the exact actor tables that own asynchronous call lifecycle.
    fn capability_worker_runtime(&mut self) -> VmCapabilityWorkerRuntime<'_> {
        VmCapabilityWorkerRuntime {
            timers: &mut self.timers,
            processes: &mut self.processes,
            scheduler: &mut self.scheduler,
        }
    }
}
