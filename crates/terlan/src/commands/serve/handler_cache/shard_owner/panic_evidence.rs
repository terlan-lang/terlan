//! Bounded evidence retained after one fixed scheduler terminates.

use crate::runtime::vm::multicore_replay::VmMulticoreReplayCapture;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

/// Immutable scheduler and supervisor evidence captured after panic containment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AotSchedulerPanicEvidence {
    /// Fixed scheduler that terminated.
    pub(crate) scheduler: VmSchedulerId,
    /// Bounded stable failure reason without a host backtrace.
    pub(crate) reason: String,
    /// Bounded scheduler stream ending in the fail-stop panic event.
    pub(crate) scheduler_replay: VmMulticoreReplayCapture,
    /// Shard lifecycle stream proving supervisor-owned crash disposition.
    pub(crate) shard_lifecycle: VmMulticoreReplayCapture,
}

impl AotSchedulerPanicEvidence {
    /// Joins scheduler and shard lifecycle captures for one contained panic.
    pub(super) fn new(
        scheduler: VmSchedulerId,
        reason: String,
        scheduler_replay: VmMulticoreReplayCapture,
        shard_lifecycle: VmMulticoreReplayCapture,
    ) -> Self {
        Self {
            scheduler,
            reason,
            scheduler_replay,
            shard_lifecycle,
        }
    }
}
