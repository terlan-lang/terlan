//! Admission and commit of operations against the current shard epoch.

use super::*;

impl PureNativeExecutionShard {
    /// Requires this shard to own one fully acknowledged routable image.
    pub(super) fn require_routable(&self, operation: &'static str) -> Result<VmShardEpoch, String> {
        if !self.supervisor.is_routable() {
            return Err(format!(
                "error[execution_shard.lifecycle]: {operation} requires Ready, found {:?}",
                self.supervisor.phase()
            ));
        }
        self.supervisor.epoch().ok_or_else(|| {
            format!("error[execution_shard.lifecycle]: {operation} has no admitted epoch")
        })
    }

    /// Requires an admitted generation that is ready or draining accepted work.
    pub(super) fn require_active_epoch(
        &self,
        operation: &'static str,
    ) -> Result<VmShardEpoch, String> {
        if !matches!(
            self.supervisor.phase(),
            crate::runtime::vm::execution_shard_supervisor::VmShardPhase::Ready
                | crate::runtime::vm::execution_shard_supervisor::VmShardPhase::Draining
        ) {
            return Err(format!(
                "error[execution_shard.lifecycle]: {operation} requires Ready or Draining, found {:?}",
                self.supervisor.phase()
            ));
        }
        self.generation()
    }

    /// Admits one direct native execution step under the active shard epoch.
    pub(super) fn begin_epoch_operation(
        &mut self,
        label: &'static str,
        kind: VmShardOperationKind,
        replay_policy: VmShardReplayPolicy,
    ) -> Result<VmShardEpochOperation, String> {
        let operation = self.new_epoch_operation(label, kind, replay_policy)?;
        match self
            .supervisor
            .begin_epoch_operation(operation)
            .map_err(|error| lifecycle_error("admit shard operation", error))?
        {
            VmShardOperationAdmission::ExecuteFirst => Ok(operation),
            admission => Err(format!(
                "error[execution_shard.operation_admission]: fresh operation {} received {admission:?}",
                operation.id.as_u64()
            )),
        }
    }

    /// Admits one non-resubmittable owner-local step without a tree allocation.
    pub(super) fn begin_internal_epoch_operation(
        &mut self,
        label: &'static str,
        kind: VmShardOperationKind,
        replay_policy: VmShardReplayPolicy,
    ) -> Result<VmShardEpochOperation, String> {
        let operation = self.new_epoch_operation(label, kind, replay_policy)?;
        match self
            .supervisor
            .begin_internal_operation(operation)
            .map_err(|error| lifecycle_error("admit internal shard operation", error))?
        {
            VmShardOperationAdmission::ExecuteFirst => Ok(operation),
            admission => Err(format!(
                "error[execution_shard.operation_admission]: fresh internal operation {} received {admission:?}",
                operation.id.as_u64()
            )),
        }
    }

    pub(super) fn new_epoch_operation(
        &mut self,
        label: &'static str,
        kind: VmShardOperationKind,
        replay_policy: VmShardReplayPolicy,
    ) -> Result<VmShardEpochOperation, String> {
        let epoch = self.require_active_epoch(label)?;
        let sequence = allocate_sequence(&self.next_operation_sequence, "shard operation")?;
        let operation_id = VmShardOperationId::new(sequence)
            .map_err(|error| lifecycle_error("allocate shard operation identity", error))?;
        Ok(VmShardEpochOperation::new(
            operation_id,
            epoch,
            kind,
            replay_policy,
        ))
    }

    /// Commits one direct native execution step and advances observable progress.
    pub(super) fn commit_epoch_operation(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<(), String> {
        match self
            .supervisor
            .commit_epoch_operation(operation)
            .map_err(|error| lifecycle_error("commit shard operation", error))?
        {
            VmShardOperationCommit::Committed => {
                self.supervisor
                    .signal_progress(operation.epoch, operation.id.as_u64())
                    .map_err(|error| lifecycle_error("publish shard operation progress", error))?;
                if !self.supervisor.retire_internal_operation(operation) {
                    return Err(format!(
                        "error[execution_shard.operation_retirement]: committed internal operation {} was not retained",
                        operation.id.as_u64()
                    ));
                }
                Ok(())
            }
            VmShardOperationCommit::AlreadyCommitted => Err(format!(
                "error[execution_shard.operation_commit]: fresh operation {} was already committed",
                operation.id.as_u64()
            )),
        }
    }

    /// Commits one scalar owner-local step and advances observable progress.
    pub(super) fn commit_internal_epoch_operation(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<(), String> {
        match self
            .supervisor
            .commit_internal_operation(operation)
            .map_err(|error| lifecycle_error("commit internal shard operation", error))?
        {
            VmShardOperationCommit::Committed => self
                .supervisor
                .signal_progress(operation.epoch, operation.id.as_u64())
                .map_err(|error| lifecycle_error("publish shard operation progress", error)),
            VmShardOperationCommit::AlreadyCommitted => Err(format!(
                "error[execution_shard.operation_commit]: fresh internal operation {} was already committed",
                operation.id.as_u64()
            )),
        }
    }
}
