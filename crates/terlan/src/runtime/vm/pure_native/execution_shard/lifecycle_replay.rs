//! Generation-qualified execution-shard lifecycle evidence.

use crate::runtime::vm::fixed_scheduler_telemetry::VM_FIXED_SCHEDULER_TRACE_CAPACITY;
use crate::runtime::vm::multicore_replay::{
    VmMulticoreEventContext, VmMulticoreEventKind, VmMulticoreReplayCapture,
    VmMulticoreReplayRecorder,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

/// Bounded lifecycle recorder owned by one native execution shard.
#[derive(Debug)]
pub(super) struct PureNativeShardLifecycleReplay {
    recorder: VmMulticoreReplayRecorder,
}

impl PureNativeShardLifecycleReplay {
    /// Creates a recorder and publishes the shard's initially admitted image.
    pub(super) fn new(scheduler: VmSchedulerId, shard_epoch: u64) -> Result<Self, String> {
        let mut replay = Self {
            recorder: VmMulticoreReplayRecorder::recording(
                scheduler,
                VM_FIXED_SCHEDULER_TRACE_CAPACITY,
            )
            .map_err(replay_error)?,
        };
        replay.record(VmMulticoreEventKind::ImageGeneration, shard_epoch, None)?;
        Ok(replay)
    }

    /// Records one generation-qualified lifecycle transition.
    pub(super) fn record(
        &mut self,
        kind: VmMulticoreEventKind,
        shard_epoch: u64,
        operation_sequence: Option<u64>,
    ) -> Result<(), String> {
        let mut context = VmMulticoreEventContext::scheduler()
            .with_shard_epoch(shard_epoch)
            .map_err(replay_error)?;
        if let Some(operation_sequence) = operation_sequence {
            context = context
                .with_operation_sequence(operation_sequence)
                .map_err(replay_error)?;
        }
        self.recorder
            .observe(kind, context)
            .map(|_| ())
            .map_err(replay_error)
    }

    /// Captures the bounded lifecycle stream without stopping the shard.
    pub(super) fn capture(&self) -> Result<VmMulticoreReplayCapture, String> {
        self.recorder.capture().map_err(replay_error)
    }
}

/// Adds the execution-shard replay namespace to one typed recorder failure.
fn replay_error(error: impl std::fmt::Display) -> String {
    format!("error[execution_shard.replay]: {error}")
}
