use super::super::memory::{VmMemoryPressureDecision, VmMemoryPressureOutcome};
use super::{VmActorRuntime, VmExitReason, VmProcessId, ACTOR_OPERATION_REDUCTIONS};

/// Process policy applied when a requested heap charge exceeds the hard limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmActorHeapLimitPolicy {
    Reject,
    Kill,
}

/// Result of one actor heap reservation and its process-lifecycle effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmActorHeapLimitOutcome {
    pub(crate) pressure: VmMemoryPressureDecision,
    pub(crate) exited: bool,
}

impl VmActorRuntime {
    /// Reserves process heap and optionally turns a hard-limit rejection into
    /// an immediate, untrappable `killed` process exit.
    pub(crate) fn reserve_actor_heap(
        &mut self,
        pid: VmProcessId,
        requested_bytes: usize,
        policy: VmActorHeapLimitPolicy,
    ) -> Result<VmActorHeapLimitOutcome, String> {
        let pressure = self
            .memory
            .account_heap(&mut self.processes, pid, requested_bytes)?;
        self.scheduler
            .charge_memory_reductions(&mut self.processes, pid, requested_bytes)
            .expect("heap-accounted actor remains live while charging memory reductions");
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);

        let exited = pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected
            && policy == VmActorHeapLimitPolicy::Kill;
        if exited {
            self.exit_actor(pid, VmExitReason::Killed)?;
        }
        Ok(VmActorHeapLimitOutcome { pressure, exited })
    }
}
