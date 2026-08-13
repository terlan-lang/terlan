use super::super::postgres::VmPostgresInspectionSnapshot;
use super::super::process::{VmProcessId, VmProcessSnapshot};
use super::super::process_environment::VmRuntimeEnvironmentSnapshot;
use super::super::scheduler::VmSchedulerMetrics;
use super::super::timer::{VmTimerMetrics, VmTimerSnapshot};

/// Opaque execution identity for one live actor invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorContext {
    pub(super) process_id: VmProcessId,
}

/// Correlated read-only state from one actor-runtime inspection boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmActorObservationSnapshot {
    pub(crate) environment: VmRuntimeEnvironmentSnapshot,
    pub(crate) processes: Vec<VmProcessSnapshot>,
    pub(crate) scheduler: VmSchedulerMetrics,
    pub(crate) timers: Vec<VmTimerSnapshot>,
    pub(crate) timer_metrics: VmTimerMetrics,
    pub(crate) postgres: VmPostgresInspectionSnapshot,
}

impl VmActorContext {
    /// Returns the current actor process identity.
    pub(crate) fn process_id(self) -> VmProcessId {
        self.process_id
    }
}
