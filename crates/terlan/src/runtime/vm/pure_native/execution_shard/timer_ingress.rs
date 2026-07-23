//! Generation-fenced timer clock ingress for one execution shard.

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::actor::{VmActorTimerAdvance, VmNativeTimerWait};
use crate::runtime::vm::execution_shard_epoch::{
    VmShardEpochOperation, VmShardOperationAdmission, VmShardOperationId, VmShardOperationKind,
    VmShardReplayPolicy,
};
use crate::runtime::vm::execution_shard_protocol::{VmExecutionShardId, VmShardEpoch};
use crate::runtime::vm::process::{VmExitReason, VmProcessId};

use crate::runtime::vm::pure_native::{
    PureNativeExecution, PureNativeExecutionContext, PureNativeSuspension,
};

use super::{allocate_sequence, lifecycle_error, PureNativeExecutionShard};

/// One immutable clock observation authorized for an exact shard generation.
///
/// A reactor may retain and publish this value, but only the owning execution
/// shard may consume it and mutate actor timer state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PureNativeTimerTick {
    /// Stable destination shard identity.
    shard: VmExecutionShardId,
    /// Exact generation and duplicate-suppression identity.
    operation: VmShardEpochOperation,
    /// Caller-owned monotonic clock observation.
    observed_tick: u64,
}

impl PureNativeTimerTick {
    /// Returns the destination execution shard.
    pub(crate) fn shard(&self) -> &VmExecutionShardId {
        &self.shard
    }

    /// Returns the generation that issued this clock observation.
    pub(crate) const fn epoch(&self) -> VmShardEpoch {
        self.operation.epoch
    }
}

/// Exact parked generated timer and its generation-fenced clock event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PureNativeTimerWait {
    owner: VmProcessId,
    request_id: u64,
    continuation_id: u64,
    native: VmNativeTimerWait,
    tick: PureNativeTimerTick,
}

impl PureNativeTimerWait {
    /// Returns the absolute logical deadline published by the scheduler.
    pub(crate) const fn deadline_tick(&self) -> u64 {
        self.native.deadline_tick
    }

    fn validate(
        &self,
        shard: &VmExecutionShardId,
        epoch: VmShardEpoch,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<(), String> {
        if self.tick.shard() != shard || self.tick.epoch() != epoch {
            return Err("error[execution_shard.timer_generation]: stale timer wait".to_string());
        }
        if self.owner != owner
            || suspension.owner_id() != owner.as_u64()
            || self.request_id != suspension.request_id()
            || self.continuation_id != suspension.continuation_id()
            || suspension.operation() != TvmTransitionOperation::Timer
        {
            return Err("error[execution_shard.timer_continuation]: timer wait does not match the parked continuation".to_string());
        }
        Ok(())
    }
}

impl PureNativeExecutionShard {
    /// Parks one generated Timer transition against the scheduler's clock.
    pub(crate) fn begin_timer_call(
        &mut self,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
        observed_tick: u64,
    ) -> Result<PureNativeTimerWait, String> {
        let epoch = self.require_active_epoch("begin_timer_call")?;
        if suspension.owner_id() != owner.as_u64()
            || suspension.operation() != TvmTransitionOperation::Timer
        {
            return Err(
                "error[execution_shard.timer_continuation]: invalid timer suspension".into(),
            );
        }
        let [delay] = suspension.arguments() else {
            return Err("error[execution_shard.timer_arguments]: Timer requires one delay".into());
        };
        let delay_ticks = u64::try_from(*delay).map_err(|_| {
            "error[execution_shard.timer_arguments]: Timer delay must be positive".to_string()
        })?;
        if delay_ticks == 0 {
            return Err(
                "error[execution_shard.timer_arguments]: Timer delay must be positive".into(),
            );
        }
        let native = self.actors.begin_native_timer_at(
            owner.as_u64(),
            suspension.request_id(),
            suspension.continuation_id(),
            observed_tick,
            delay_ticks,
        )?;
        let tick = self.issue_timer_tick(native.deadline_tick)?;
        debug_assert_eq!(tick.epoch(), epoch);
        Ok(PureNativeTimerWait {
            owner,
            request_id: suspension.request_id(),
            continuation_id: suspension.continuation_id(),
            native,
            tick,
        })
    }

    /// Applies one timer event and resumes only its exact generated continuation.
    pub(crate) fn resume_timer_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeTimerWait,
    ) -> Result<PureNativeExecution, String> {
        let epoch = self.require_active_epoch("resume_timer_call")?;
        wait.validate(self.supervisor.shard_id(), epoch, owner, &suspension)?;
        let advance = self.apply_timer_tick(wait.tick)?.ok_or_else(|| {
            "error[execution_shard.timer_duplicate]: timer completion already consumed".to_string()
        })?;
        self.actors.complete_delivered_native_timer(
            owner.as_u64(),
            suspension.request_id(),
            suspension.continuation_id(),
            wait.native,
            &advance,
        )?;
        let operation = self.begin_epoch_operation(
            "resume_timer_call",
            VmShardOperationKind::ContinuationResume,
            VmShardReplayPolicy::AtMostOnce,
        )?;
        #[cfg(test)]
        self.trace.push(super::NativeShardDispatchEvent::Resume {
            owner,
            continuation_id: suspension.continuation_id(),
        });
        let execution = {
            let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
            self.boundary
                .resume_timer_for_actor(&mut self.actors, &mut context, suspension)
        };
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                    )),
                };
            }
        };
        self.record_completion(owner, &execution);
        self.commit_epoch_operation(operation)?;
        Ok(execution)
    }

    /// Issues one timer clock observation under the currently active epoch.
    ///
    /// Issuance does not inspect or mutate actor state, so a reactor may wait
    /// after obtaining this value without borrowing the execution shard.
    pub(crate) fn issue_timer_tick(
        &self,
        observed_tick: u64,
    ) -> Result<PureNativeTimerTick, String> {
        let epoch = self.require_active_epoch("issue_timer_tick")?;
        let sequence = allocate_sequence(&self.next_operation_sequence, "timer tick")?;
        let operation_id = VmShardOperationId::new(sequence)
            .map_err(|error| lifecycle_error("allocate timer operation identity", error))?;
        Ok(PureNativeTimerTick {
            shard: self.supervisor.shard_id().clone(),
            operation: VmShardEpochOperation::new(
                operation_id,
                epoch,
                VmShardOperationKind::TimerDelivery,
                VmShardReplayPolicy::AtMostOnce,
            ),
            observed_tick,
        })
    }

    /// Applies one clock observation on the owning shard thread.
    ///
    /// `None` means the exact event was already admitted or committed. Every
    /// identity and epoch check runs before the actor timer table is advanced.
    pub(crate) fn apply_timer_tick(
        &mut self,
        tick: PureNativeTimerTick,
    ) -> Result<Option<VmActorTimerAdvance>, String> {
        if tick.shard != *self.supervisor.shard_id() {
            return Err(format!(
                "error[execution_shard.timer_identity]: timer tick targets shard `{}`, current shard is `{}`",
                tick.shard.as_str(),
                self.supervisor.shard_id().as_str()
            ));
        }
        self.require_active_epoch("apply_timer_tick")?;
        let admission = self
            .supervisor
            .begin_epoch_operation(tick.operation)
            .map_err(|error| lifecycle_error("admit timer tick", error))?;
        if admission == VmShardOperationAdmission::DuplicateSuppressed {
            return Ok(None);
        }
        if admission != VmShardOperationAdmission::ExecuteFirst {
            return Err(format!(
                "error[execution_shard.timer_admission]: timer operation {} received {admission:?}",
                tick.operation.id.as_u64()
            ));
        }

        let advance = self.actors.advance_actor_timers(tick.observed_tick);
        match self
            .supervisor
            .commit_epoch_operation(tick.operation)
            .map_err(|error| lifecycle_error("commit timer tick", error))?
        {
            crate::runtime::vm::execution_shard_epoch::VmShardOperationCommit::Committed => {
                let progress = allocate_sequence(
                    &self.next_operation_sequence,
                    "timer completion progress",
                )?;
                self.supervisor
                    .signal_progress(tick.operation.epoch, progress)
                    .map_err(|error| lifecycle_error("publish timer progress", error))?;
                Ok(Some(advance))
            }
            crate::runtime::vm::execution_shard_epoch::VmShardOperationCommit::AlreadyCommitted => {
                Err(format!(
                    "error[execution_shard.timer_commit]: fresh timer operation {} was already committed",
                    tick.operation.id.as_u64()
                ))
            }
        }
    }
}
