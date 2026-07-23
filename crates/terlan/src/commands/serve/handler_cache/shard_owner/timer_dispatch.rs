//! Fixed-owner publication and cancellation of elapsed generated timers.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::runtime::vm::actor_directory::VmMailboxWake;
use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::fixed_scheduler_telemetry::VmFixedSchedulerTelemetry;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

use super::capability_dispatch::GeneratedCapabilityDispatcher;
use super::owner_loop::drain_route;
use super::runnable_queue::GeneratedRunnableQueues;
use super::timer_queue::{GeneratedTimerQueue, PendingTimerInvocation};
use super::AotSchedulerPublication;

/// Publishes every elapsed timer through the fixed actor directory before resume.
#[allow(clippy::too_many_arguments)]
pub(super) fn publish_due_timers(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    timers: &mut GeneratedTimerQueue,
    capabilities: &mut GeneratedCapabilityDispatcher,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
) -> Result<(), String> {
    for PendingTimerInvocation {
        route,
        owner,
        suspension,
        wait,
        reply,
        ..
    } in timers.take_due(Instant::now())
    {
        let publication = AotSchedulerPublication::Timer {
            owner,
            suspension,
            wait,
            reply,
        };
        let kind = publication.published_kind();
        let (identity, wake) = control.publish_identified(route, publication)?;
        telemetry.record_publication(kind, route, identity)?;
        if wake != VmMailboxWake::Enqueue {
            return Err(format!(
                "error[vm.timer_wake]: actor {} timer observed non-parked lifecycle",
                route.actor_id()
            ));
        }
        drain_route(
            shard,
            routes,
            runnable,
            timers,
            capabilities,
            control,
            telemetry,
            scheduler,
            route,
        )?;
    }
    Ok(())
}

/// Cancels timer-parked actors and settles their retained invocation replies.
pub(super) fn cancel_timers(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    timers: &mut GeneratedTimerQueue,
    capabilities: &mut GeneratedCapabilityDispatcher,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
    detail: &str,
) -> Result<(), String> {
    let routes_to_cancel = timers.routes();
    for route in routes_to_cancel {
        let reason = format!("error[vm.scheduler_shutdown]: {detail}");
        let owner = *routes.get(&route.actor_id()).ok_or_else(|| {
            format!(
                "error[vm.timer_shutdown]: actor {} route is missing",
                route.actor_id()
            )
        })?;
        let publication = AotSchedulerPublication::CancellationSignal {
            owner,
            reason,
            reply: None,
        };
        let kind = publication.published_kind();
        let (identity, wake) = control.publish_identified(route, publication)?;
        telemetry.record_publication(kind, route, identity)?;
        if wake != VmMailboxWake::Enqueue {
            return Err(format!(
                "error[vm.timer_shutdown]: actor {} was not parked",
                route.actor_id()
            ));
        }
        drain_route(
            shard,
            routes,
            runnable,
            timers,
            capabilities,
            control,
            telemetry,
            scheduler,
            route,
        )?;
    }
    Ok(())
}
