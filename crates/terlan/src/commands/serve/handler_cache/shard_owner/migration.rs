//! Runnable actor transfer between generated scheduler owners.

use std::collections::BTreeMap;
use std::thread;

use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::fixed_scheduler_telemetry::{
    VmFixedSchedulerEventKind, VmFixedSchedulerTelemetry,
};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};

use super::owner_loop::{
    reject_duplicate_route, validate_live_route, validate_scheduler_route, RUNNABLE_QUEUE_CAPACITY,
};
use super::runnable_queue::{GeneratedRunnableQueues, PendingRunnableInvocation};
use super::{AotSchedulerPublication, OwnedRunnableImportFailure, OwnedRunnableTransfer};

/// Removes one queued continuation and publishes its destination route.
pub(super) fn detach_runnable(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    destination: VmSchedulerId,
    class: VmSchedulerClass,
) -> Result<Option<OwnedRunnableTransfer>, String> {
    let Some(pending) = runnable.pop_for_steal(class) else {
        return Ok(None);
    };
    let source = pending.route;
    if source.scheduler() == destination {
        runnable.push(pending);
        return Err(
            "error[vm.work_stealing.destination]: source and destination match".to_string(),
        );
    }
    validate_live_route(routes, source, pending.owner)?;
    let ticket = match control.begin_migration(source, destination) {
        Ok(ticket) => ticket,
        Err(error) => {
            runnable.push(pending);
            return Err(error);
        }
    };
    let replay_context = telemetry.context_for_migration(&ticket)?;
    record_context_or_panic(
        telemetry,
        VmFixedSchedulerEventKind::MigrationStarted,
        replay_context,
    );
    let destination_route = match control.complete_migration(ticket) {
        Ok(route) => {
            record_context_or_panic(
                telemetry,
                VmFixedSchedulerEventKind::MigrationCompleted,
                replay_context,
            );
            route
        }
        Err(error) => {
            record_context_or_panic(
                telemetry,
                VmFixedSchedulerEventKind::MigrationAborted,
                replay_context,
            );
            runnable.push(pending);
            return Err(error);
        }
    };
    let actor = match shard.detach_actor_state(pending.owner) {
        Ok(actor) => actor,
        Err(error) => {
            let rollback = move_control_route(control, destination_route, source.scheduler());
            record_context_or_panic(
                telemetry,
                VmFixedSchedulerEventKind::MigrationAborted,
                replay_context,
            );
            runnable.push(pending);
            return match rollback {
                Ok(_) => Err(error),
                Err(rollback) => Err(format!("{error}; {rollback}")),
            };
        }
    };
    routes.remove(&source.actor_id());
    record_context_or_panic(telemetry, VmFixedSchedulerEventKind::Stolen, replay_context);
    record_context_or_panic(
        telemetry,
        VmFixedSchedulerEventKind::StealOutcome,
        replay_context,
    );
    Ok(Some(OwnedRunnableTransfer {
        source,
        destination: destination_route,
        owner: pending.owner,
        class: pending.class,
        suspension: pending.suspension,
        enqueued_at: pending.enqueued_at,
        reply: pending.reply,
        actor,
        replay_context,
    }))
}

/// Imports one detached runnable envelope without duplicating actor authority.
pub(super) fn import_runnable(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
    route: VmFixedActorRoute,
    transfer: OwnedRunnableTransfer,
) -> Result<(), OwnedRunnableImportFailure> {
    let validation = validate_scheduler_route(route, &thread::current())
        .and_then(|()| reject_duplicate_route(routes, route))
        .and_then(|()| {
            if route.actor_id() != transfer.source.actor_id()
                || route.actor_id() != transfer.destination.actor_id()
            {
                return Err(
                    "error[vm.work_stealing.actor]: transfer route changed actor".to_string(),
                );
            }
            if route.scheduler() != scheduler {
                return Err(
                    "error[vm.work_stealing.destination]: import scheduler mismatch".to_string(),
                );
            }
            if runnable.len() == RUNNABLE_QUEUE_CAPACITY {
                return Err(format!(
                    "error[vm.scheduler_queue_full]: scheduler {} runnable capacity {} exhausted",
                    scheduler.index(),
                    RUNNABLE_QUEUE_CAPACITY
                ));
            }
            Ok(())
        });
    if let Err(reason) = validation {
        return Err(OwnedRunnableImportFailure {
            reason,
            transfer: Some(Box::new(transfer)),
        });
    }
    let OwnedRunnableTransfer {
        source,
        destination,
        owner,
        class,
        suspension,
        enqueued_at,
        reply,
        actor,
        replay_context,
    } = transfer;
    if let Err(failure) = shard.import_actor_state(actor) {
        return Err(OwnedRunnableImportFailure {
            reason: failure.reason().to_string(),
            transfer: Some(Box::new(OwnedRunnableTransfer {
                source,
                destination,
                owner,
                class,
                suspension,
                enqueued_at,
                reply,
                actor: failure.into_transfer(),
                replay_context,
            })),
        });
    }
    routes.insert(route.actor_id(), owner);
    runnable.push(PendingRunnableInvocation {
        route,
        owner,
        class,
        suspension,
        enqueued_at,
        reply,
    });
    let destination_context = replay_context.with_peer_scheduler(source.scheduler());
    record_context_or_panic(
        telemetry,
        VmFixedSchedulerEventKind::Imported,
        destination_context,
    );
    record_context_or_panic(
        telemetry,
        VmFixedSchedulerEventKind::StealOutcome,
        destination_context,
    );
    Ok(())
}

/// Converts canonical replay corruption into the owner thread's fail-stop path.
fn record_context_or_panic(
    telemetry: &VmFixedSchedulerTelemetry,
    kind: VmFixedSchedulerEventKind,
    context: crate::runtime::vm::multicore_replay::VmMulticoreEventContext,
) {
    if let Err(error) = telemetry.record_with_context(kind, context) {
        panic!("fixed scheduler telemetry corruption: {error}");
    }
}

/// Moves one unowned directory route between scheduler identities.
fn move_control_route(
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    source: VmFixedActorRoute,
    destination: VmSchedulerId,
) -> Result<VmFixedActorRoute, String> {
    let ticket = control.begin_migration(source, destination)?;
    control.complete_migration(ticket)
}
