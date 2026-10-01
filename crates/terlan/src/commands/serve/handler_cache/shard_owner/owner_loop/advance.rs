//! One fixed-scheduler execution slice, including granted native contexts.

use super::*;

/// Advances generated code until it completes, parks, or cooperatively yields.
pub(super) fn advance_slice(
    shard: &mut PureNativeExecutionShard,
    owner: VmProcessId,
    mut execution: PureNativeExecution,
    observed_tick: u64,
) -> Result<ScheduledInvocationStep, String> {
    loop {
        match execution {
            PureNativeExecution::Complete(value) => {
                shard.finish_completed_call(owner)?;
                return Ok(ScheduledInvocationStep::Complete(value));
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Receive =>
            {
                let wait = shard.io_wait(owner, &suspension)?;
                return Ok(ScheduledInvocationStep::Waiting {
                    owner,
                    suspension: *suspension,
                    wait,
                });
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Timer =>
            {
                let wait = shard.begin_timer_call(owner, &suspension, observed_tick)?;
                return Ok(ScheduledInvocationStep::TimerWaiting {
                    owner,
                    suspension: *suspension,
                    wait,
                });
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Yield =>
            {
                let class = shard.scheduler_class(owner)?;
                return Ok(ScheduledInvocationStep::Runnable {
                    owner,
                    class,
                    suspension: *suspension,
                });
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Capability =>
            {
                let wait = shard.begin_capability_call(owner, &suspension)?;
                if shard.has_native_service(&wait) {
                    execution = shard.resume_native_service_call(owner, *suspension, wait)?;
                    continue;
                }
                return Ok(ScheduledInvocationStep::CapabilityWaiting {
                    owner,
                    suspension: *suspension,
                    wait,
                });
            }
            PureNativeExecution::Suspended(suspension) => {
                execution = shard.resume_call(owner, *suspension)?;
            }
        }
    }
}
