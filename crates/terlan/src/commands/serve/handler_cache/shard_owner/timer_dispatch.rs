//! Fixed-owner publication and cancellation of elapsed generated timers.

use std::time::Instant;

use crate::runtime::vm::actor_directory::VmMailboxWake;

use super::capability_dispatch::GeneratedCapabilityDispatcher;
use super::owner_loop::{drain_route, ShardOwnerState};
use super::timer_queue::PendingTimerInvocation;
use super::AotSchedulerPublication;

/// Publishes every elapsed timer through the fixed actor directory before resume.
pub(super) fn publish_due_timers(
    state: &mut ShardOwnerState<'_>,
    capabilities: &mut GeneratedCapabilityDispatcher,
) -> Result<(), String> {
    for PendingTimerInvocation {
        route,
        owner,
        suspension,
        wait,
        reply,
        ..
    } in state.timers.take_due(Instant::now())
    {
        let publication = AotSchedulerPublication::Timer {
            owner,
            suspension,
            wait,
            reply,
        };
        let kind = publication.published_kind();
        let (identity, wake) = state.control.publish_identified(route, publication)?;
        state.telemetry.record_publication(kind, route, identity)?;
        if wake != VmMailboxWake::Enqueue {
            return Err(format!(
                "error[vm.timer_wake]: actor {} timer observed non-parked lifecycle",
                route.actor_id()
            ));
        }
        drain_route(state, capabilities, route)?;
    }
    Ok(())
}

/// Cancels timer-parked actors and settles their retained invocation replies.
pub(super) fn cancel_timers(
    state: &mut ShardOwnerState<'_>,
    capabilities: &mut GeneratedCapabilityDispatcher,
    detail: &str,
) -> Result<(), String> {
    let routes_to_cancel = state.timers.routes();
    for route in routes_to_cancel {
        let reason = format!("error[vm.scheduler_shutdown]: {detail}");
        let owner = *state.routes.get(&route.actor_id()).ok_or_else(|| {
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
        let (identity, wake) = state.control.publish_identified(route, publication)?;
        state.telemetry.record_publication(kind, route, identity)?;
        if wake != VmMailboxWake::Enqueue {
            return Err(format!(
                "error[vm.timer_shutdown]: actor {} was not parked",
                route.actor_id()
            ));
        }
        drain_route(state, capabilities, route)?;
    }
    Ok(())
}
