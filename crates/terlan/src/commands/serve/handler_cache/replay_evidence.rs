//! Bounded replay diagnostics for one live HTTP handler generation.

use crate::runtime::vm::fixed_scheduler_telemetry::VM_FIXED_SCHEDULER_TRACE_CAPACITY;
use crate::runtime::vm::multicore_replay::VmMulticoreReplayEvidence;

use super::{shard_owner, AotHandlerGeneration};

impl AotHandlerGeneration {
    /// Aggregates bounded scheduler evidence for this exact handler generation.
    pub(super) fn multicore_replay_evidence(&self) -> Result<VmMulticoreReplayEvidence, String> {
        let maximum_events = self
            .shards
            .len()
            .checked_mul(VM_FIXED_SCHEDULER_TRACE_CAPACITY)
            .ok_or_else(|| {
                "error[serve.aot.replay_evidence]: aggregate capacity exhausted".to_string()
            })?;
        let captures = self
            .shards
            .iter()
            .map(|shard| shard.multicore_replay_capture())
            .collect::<Result<Vec<_>, _>>()?;
        VmMulticoreReplayEvidence::new(self.identity, self.shards.len(), maximum_events, captures)
            .map_err(|error| format!("error[serve.aot.replay_evidence]: {error}"))
    }

    /// Returns bounded artifacts retained by scheduler panic containment.
    pub(super) fn scheduler_panic_evidence(
        &self,
    ) -> Result<Vec<shard_owner::AotSchedulerPanicEvidence>, String> {
        self.shards
            .iter()
            .map(|shard| shard.panic_evidence())
            .filter_map(|evidence| match evidence {
                Ok(Some(evidence)) => Some(Ok(evidence)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }
}

impl std::fmt::Debug for AotHandlerGeneration {
    /// Renders aggregate replay health without dumping retained event payloads.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let replay = self.multicore_replay_evidence().ok();
        let scheduler_panics = self
            .scheduler_panic_evidence()
            .map(|evidence| evidence.len())
            .ok();
        formatter
            .debug_struct("AotHandlerGeneration")
            .field("shards", &self.shards.len())
            .field(
                "replay_retained_events",
                &replay.as_ref().map(|evidence| evidence.retained_events),
            )
            .field(
                "replay_dropped_events",
                &replay.as_ref().map(|evidence| evidence.dropped_events),
            )
            .field(
                "replayable",
                &replay.as_ref().map(|evidence| evidence.replayable),
            )
            .field("scheduler_panics", &scheduler_panics)
            .finish_non_exhaustive()
    }
}
