//! Cooperative execution loop for one generated AOT scheduler owner.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(test)]
use std::sync::atomic::Ordering;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::actor_directory::VmActorLifecycle;
use crate::runtime::vm::debugger_control::{
    VmDebuggerControlCommand, VmDebuggerScheduleControl, VmDebuggerSlicePermit,
};
use crate::runtime::vm::fixed_scheduler_control::{VmFixedActorLease, VmFixedSchedulerControl};
use crate::runtime::vm::fixed_scheduler_telemetry::{
    VmFixedSchedulerEventKind, VmFixedSchedulerTelemetry,
};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{
    PureNativeActorImportFailure, PureNativeCapabilityWait, PureNativeExecution,
    PureNativeExecutionShard, PureNativeIoWait, PureNativeSuspension, PureNativeTimerWait,
};
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};
use crate::runtime::vm::ReplValue;

use super::capability_dispatch::{GeneratedCapabilityDispatcher, PendingGeneratedCapability};
use super::migration::{detach_runnable, import_runnable};
use super::replay_events::{execute_interval, settle_terminal as settle_terminal_owned};
use super::runnable_queue::{GeneratedRunnableQueues, PendingRunnableInvocation};
use super::timer_dispatch::{cancel_timers, publish_due_timers};
use super::timer_queue::GeneratedTimerQueue;
use super::{
    AotSchedulerPublication, OwnedInvocationStep, OwnedRunnableImportFailure, ShardCommand,
    SHARD_INBOX_CAPACITY,
};

/// The runnable queue is bounded independently from command ingress.
pub(super) const RUNNABLE_QUEUE_CAPACITY: usize = SHARD_INBOX_CAPACITY;

/// Internal execution boundary hidden from HTTP invocation callers.
enum ScheduledInvocationStep {
    Complete(ReplValue),
    Waiting {
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeIoWait,
    },
    TimerWaiting {
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeTimerWait,
    },
    CapabilityWaiting {
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
    },
    Runnable {
        owner: VmProcessId,
        class: crate::runtime::vm::scheduler::VmSchedulerClass,
        suspension: PureNativeSuspension,
    },
}

/// Runs command ingress and one real runnable actor slice in alternation.
pub(super) fn owner_loop(
    shard: &mut PureNativeExecutionShard,
    commands: Receiver<ShardCommand>,
    control: Arc<VmFixedSchedulerControl<AotSchedulerPublication>>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
) {
    let mut routes = BTreeMap::new();
    let mut runnable = GeneratedRunnableQueues::new();
    let mut timers = GeneratedTimerQueue::new();
    let mut capabilities = GeneratedCapabilityDispatcher::new(scheduler);
    let mut debugger = VmDebuggerScheduleControl::running();
    let mut reject_runnable_imports = false;
    loop {
        publish_due_timers(
            shard,
            &mut routes,
            &mut runnable,
            &mut timers,
            &mut capabilities,
            &control,
            telemetry,
            scheduler,
        )
        .unwrap_or_else(|error| panic!("fixed scheduler timer corruption: {error}"));
        capabilities
            .dispatch_next(
                shard,
                &mut routes,
                &mut runnable,
                &mut timers,
                &control,
                telemetry,
            )
            .unwrap_or_else(|error| panic!("fixed scheduler capability corruption: {error}"));
        let command = if runnable.is_empty() || !debugger.can_service_runnable() {
            let timeout = scheduler_wait_timeout(&timers, &capabilities);
            match timeout {
                Some(timeout) => match commands.recv_timeout(timeout) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => break,
                },
            }
        } else {
            match commands.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        };

        if let Some(command) = command {
            if handle_command(
                shard,
                &mut routes,
                &mut runnable,
                &mut timers,
                &mut capabilities,
                &control,
                telemetry,
                scheduler,
                &mut debugger,
                &mut reject_runnable_imports,
                command,
            ) {
                return;
            }
        }
        if !runnable.is_empty() {
            let Some(permit) = debugger.claim_runnable_slice() else {
                continue;
            };
            let Some(pending) = runnable.pop_weighted() else {
                continue;
            };
            service_runnable(
                shard,
                &mut routes,
                &mut runnable,
                &mut timers,
                &mut capabilities,
                &control,
                telemetry,
                scheduler,
                pending,
                permit,
            )
            .unwrap_or_else(|error| panic!("fixed scheduler control corruption: {error}"));
        }
    }
    cancel_runnable(
        shard,
        &mut routes,
        &mut runnable,
        &control,
        telemetry,
        scheduler,
        "owner command channel closed",
    );
    cancel_timers(
        shard,
        &mut routes,
        &mut runnable,
        &mut timers,
        &mut capabilities,
        &control,
        telemetry,
        scheduler,
        "owner command channel closed",
    )
    .unwrap_or_else(|error| panic!("fixed scheduler timer cancellation failed: {error}"));
    capabilities
        .cancel_all(
            shard,
            &mut routes,
            &control,
            telemetry,
            "owner command channel closed",
        )
        .unwrap_or_else(|error| panic!("fixed scheduler capability cancellation failed: {error}"));
    let _ = shard.shutdown();
}

/// Bounds command sleep while external worker completions remain pending.
fn scheduler_wait_timeout(
    timers: &GeneratedTimerQueue,
    capabilities: &GeneratedCapabilityDispatcher,
) -> Option<Duration> {
    let timer = timers.next_timeout(Instant::now());
    if capabilities.has_pending() {
        Some(
            timer
                .unwrap_or(Duration::from_millis(1))
                .min(Duration::from_millis(1)),
        )
    } else {
        timer
    }
}

/// Applies one command and reports whether orderly shutdown completed.
fn handle_command(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    timers: &mut GeneratedTimerQueue,
    capabilities: &mut GeneratedCapabilityDispatcher,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
    debugger: &mut VmDebuggerScheduleControl,
    _reject_runnable_imports: &mut bool,
    command: ShardCommand,
) -> bool {
    match command {
        ShardCommand::Begin {
            route,
            export,
            args,
            reply,
        } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
            let lease = reject_duplicate_route(routes, route)
                .and_then(|()| validate_scheduler_route(route, &thread::current()))
                .and_then(|()| control.acquire(route, scheduler));
            match lease {
                Ok(lease) => {
                    telemetry
                        .record_owned(VmFixedSchedulerEventKind::Entry, &lease)
                        .unwrap_or_else(|error| {
                            panic!("fixed scheduler telemetry corruption: {error}")
                        });
                    let result = execute_interval(telemetry, &lease, || {
                        shard
                            .begin_call(&export, &args)
                            .and_then(|(owner, execution)| {
                                advance_slice(shard, owner, execution, timers.observed_tick())
                            })
                            .and_then(|step| register_route(routes, route, step))
                    });
                    finish_execution(
                        shard,
                        routes,
                        runnable,
                        timers,
                        capabilities,
                        control,
                        telemetry,
                        route,
                        lease,
                        result,
                        reply,
                    )
                    .unwrap_or_else(|error| panic!("fixed scheduler control corruption: {error}"));
                }
                Err(error) => {
                    record_or_panic(telemetry, VmFixedSchedulerEventKind::Failed, Some(route));
                    let _ = reply.send(Err(error));
                }
            }
        }
        ShardCommand::Drain { route } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
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
            )
            .unwrap_or_else(|error| panic!("fixed scheduler control corruption: {error}"));
        }
        ShardCommand::DetachMigration {
            route,
            owner,
            reply,
        } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
            let result = validate_scheduler_route(route, &thread::current())
                .and_then(|()| validate_live_route(routes, route, owner))
                .and_then(|()| shard.detach_actor_state(owner));
            if result.is_ok() {
                routes.remove(&route.actor_id());
            }
            let _ = reply.send(result);
        }
        ShardCommand::ImportMigration {
            route,
            transfer,
            reply,
        } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
            let owner = transfer.owner();
            let validation = validate_scheduler_route(route, &thread::current())
                .and_then(|()| reject_duplicate_route(routes, route));
            let result = match validation {
                Ok(()) => shard.import_actor_state(transfer),
                Err(reason) => Err(PureNativeActorImportFailure::rejected(reason, transfer)),
            };
            if result.is_ok() {
                routes.insert(route.actor_id(), owner);
            }
            let _ = reply.send(result);
        }
        ShardCommand::DetachRunnable {
            destination,
            class,
            reply,
        } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, None);
            let result = detach_runnable(
                shard,
                routes,
                runnable,
                control,
                telemetry,
                destination,
                class,
            );
            let _ = reply.send(result);
        }
        ShardCommand::ImportRunnable {
            route,
            transfer,
            reply,
        } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
            let result = if *_reject_runnable_imports {
                Err(OwnedRunnableImportFailure {
                    reason:
                        "error[vm.work_stealing.import_injected]: destination rejected runnable"
                            .to_string(),
                    transfer: Some(transfer),
                })
            } else {
                import_runnable(
                    shard, routes, runnable, telemetry, scheduler, route, transfer,
                )
            };
            let _ = reply.send(result);
        }
        ShardCommand::RunnableSnapshot { reply } => {
            let _ = reply.send(runnable.snapshot(scheduler));
        }
        ShardCommand::DebuggerControl { command, reply } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, None);
            let result = debugger.apply(command);
            if result.is_ok() {
                let kind = match command {
                    VmDebuggerControlCommand::Pause => {
                        Some(VmFixedSchedulerEventKind::DebuggerPaused)
                    }
                    VmDebuggerControlCommand::Continue => {
                        Some(VmFixedSchedulerEventKind::DebuggerContinued)
                    }
                    VmDebuggerControlCommand::Step { .. } => None,
                };
                if let Some(kind) = kind {
                    record_or_panic(telemetry, kind, None);
                }
            }
            let _ = reply.send(result);
        }
        #[cfg(test)]
        ShardCommand::CompletedCount { reply } => {
            let _ = reply.send(shard.completed_call_count());
        }
        #[cfg(test)]
        ShardCommand::PanicWhileOwning { route } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
            let lease = control.acquire(route, scheduler).unwrap_or_else(|error| {
                panic!("failed to acquire injected panic actor lease: {error}")
            });
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Entry, Some(route));
            telemetry
                .begin_execution(&lease)
                .unwrap_or_else(|error| panic!("failed to begin injected panic interval: {error}"));
            panic!("injected scheduler panic while owning actor");
        }
        #[cfg(test)]
        ShardCommand::RejectRunnableImports { reject, reply } => {
            *_reject_runnable_imports = reject;
            let _ = reply.send(());
        }
        #[cfg(test)]
        ShardCommand::ProbeExecution {
            route,
            export,
            args,
            barrier,
            active,
            maximum,
            reply,
        } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, Some(route));
            let result = control.acquire(route, scheduler).and_then(|lease| {
                telemetry.record_owned(VmFixedSchedulerEventKind::Entry, &lease)?;
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                barrier.wait();
                let result = execute_interval(telemetry, &lease, || shard.call(&export, &args));
                active.fetch_sub(1, Ordering::SeqCst);
                settle_terminal_owned(control, telemetry, lease, result).map(|value| {
                    let owner_thread = thread::current().name().unwrap_or("unnamed").to_string();
                    (value, owner_thread)
                })
            });
            let _ = reply.send(result);
        }
        ShardCommand::Shutdown { reply } => {
            record_or_panic(telemetry, VmFixedSchedulerEventKind::Command, None);
            cancel_runnable(
                shard,
                routes,
                runnable,
                control,
                telemetry,
                scheduler,
                "scheduler shutdown",
            );
            cancel_timers(
                shard,
                routes,
                runnable,
                timers,
                capabilities,
                control,
                telemetry,
                scheduler,
                "scheduler shutdown",
            )
            .unwrap_or_else(|error| panic!("fixed scheduler timer cancellation failed: {error}"));
            let result = capabilities
                .cancel_all(shard, routes, control, telemetry, "scheduler shutdown")
                .and_then(|()| shard.shutdown());
            let kind = if result.is_ok() {
                VmFixedSchedulerEventKind::Shutdown
            } else {
                VmFixedSchedulerEventKind::Failed
            };
            record_or_panic(telemetry, kind, None);
            let _ = reply.send(result);
            return true;
        }
    }
    false
}

/// Executes one queued continuation under a newly acquired owner lease.
fn service_runnable(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    timers: &mut GeneratedTimerQueue,
    capabilities: &mut GeneratedCapabilityDispatcher,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
    pending: PendingRunnableInvocation,
    permit: VmDebuggerSlicePermit,
) -> Result<(), String> {
    let PendingRunnableInvocation {
        route,
        owner,
        class: _,
        suspension,
        enqueued_at: _,
        reply,
    } = pending;
    let lease = control.acquire(route, scheduler)?;
    if permit == VmDebuggerSlicePermit::Step {
        telemetry.record_owned(VmFixedSchedulerEventKind::DebuggerStepped, &lease)?;
    }
    telemetry.record_owned(VmFixedSchedulerEventKind::Resumed, &lease)?;
    let result = execute_interval(telemetry, &lease, || {
        validate_live_route(routes, route, owner).and_then(|()| {
            shard.resume_call(owner, suspension).and_then(|execution| {
                advance_slice(shard, owner, execution, timers.observed_tick())
            })
        })
    });
    finish_execution(
        shard,
        routes,
        runnable,
        timers,
        capabilities,
        control,
        telemetry,
        route,
        lease,
        result,
        reply,
    )
}

/// Releases a slice and either replies externally or retains runnable work.
#[allow(clippy::too_many_arguments)]
fn finish_execution(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    timers: &mut GeneratedTimerQueue,
    capabilities: &mut GeneratedCapabilityDispatcher,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    route: VmFixedActorRoute,
    lease: VmFixedActorLease,
    result: Result<ScheduledInvocationStep, String>,
    reply: SyncSender<Result<OwnedInvocationStep, String>>,
) -> Result<(), String> {
    match result {
        Ok(ScheduledInvocationStep::Runnable {
            owner,
            class,
            suspension,
        }) => {
            if runnable.len() == RUNNABLE_QUEUE_CAPACITY {
                let reason = format!(
                    "error[vm.scheduler_queue_full]: scheduler {} runnable capacity {} exhausted",
                    route.scheduler().index(),
                    RUNNABLE_QUEUE_CAPACITY
                );
                let result = shard.cancel_call(owner, reason.clone()).and(Err(reason));
                routes.remove(&route.actor_id());
                let result = settle_terminal_owned(control, telemetry, lease, result);
                let _ = reply.send(result);
                return Ok(());
            }
            let context = telemetry.context_for_lease(&lease)?;
            control.release(lease, VmActorLifecycle::Yielding)?;
            control.requeue_yielded(route)?;
            telemetry.record_with_context(VmFixedSchedulerEventKind::Yielded, context)?;
            runnable.push(PendingRunnableInvocation {
                route,
                owner,
                class,
                suspension,
                enqueued_at: Instant::now(),
                reply,
            });
        }
        Ok(ScheduledInvocationStep::Waiting {
            owner,
            suspension,
            wait,
        }) => {
            let context = telemetry.context_for_lease(&lease)?;
            control.release(lease, VmActorLifecycle::Parked)?;
            telemetry.record_with_context(VmFixedSchedulerEventKind::Parked, context)?;
            let _ = reply.send(Ok(OwnedInvocationStep::Waiting {
                route,
                owner,
                suspension,
                wait,
            }));
        }
        Ok(ScheduledInvocationStep::TimerWaiting {
            owner,
            suspension,
            wait,
        }) => {
            if let Err(rejection) = timers.push(route, owner, suspension, wait, reply) {
                let result: Result<OwnedInvocationStep, String> = shard
                    .cancel_call(owner, rejection.reason.clone())
                    .and(Err(rejection.reason));
                routes.remove(&route.actor_id());
                let result = settle_terminal_owned(control, telemetry, lease, result);
                let _ = rejection.reply.send(result);
                return Ok(());
            }
            let context = telemetry.context_for_lease(&lease)?;
            control.release(lease, VmActorLifecycle::Parked)?;
            telemetry.record_with_context(VmFixedSchedulerEventKind::Parked, context)?;
        }
        Ok(ScheduledInvocationStep::CapabilityWaiting {
            owner,
            suspension,
            wait,
        }) => {
            if capabilities.is_enabled() {
                let pending = PendingGeneratedCapability {
                    route,
                    owner,
                    suspension,
                    wait,
                    reply,
                };
                match capabilities.submit(pending) {
                    Ok(()) => {
                        let context = telemetry.context_for_lease(&lease)?;
                        control.release(lease, VmActorLifecycle::Parked)?;
                        telemetry
                            .record_with_context(VmFixedSchedulerEventKind::Parked, context)?;
                    }
                    Err((reason, pending)) => {
                        let result: Result<OwnedInvocationStep, String> =
                            shard.cancel_call(owner, reason.clone()).and(Err(reason));
                        routes.remove(&route.actor_id());
                        let result = settle_terminal_owned(control, telemetry, lease, result);
                        let _ = pending.reply.send(result);
                    }
                }
                return Ok(());
            }
            let context = telemetry.context_for_lease(&lease)?;
            control.release(lease, VmActorLifecycle::Parked)?;
            telemetry.record_with_context(VmFixedSchedulerEventKind::Parked, context)?;
            let _ = reply.send(Ok(OwnedInvocationStep::CapabilityWaiting {
                route,
                owner,
                suspension,
                wait,
            }));
        }
        Ok(ScheduledInvocationStep::Complete(value)) => {
            routes.remove(&route.actor_id());
            let result = settle_terminal_owned(
                control,
                telemetry,
                lease,
                Ok(OwnedInvocationStep::Complete { route, value }),
            );
            let _ = reply.send(result);
        }
        Err(error) => {
            routes.remove(&route.actor_id());
            let result = settle_terminal_owned(control, telemetry, lease, Err(error));
            let _ = reply.send(result);
        }
    }
    Ok(())
}

/// Drains one published event after acquiring its fixed scheduler lease.
pub(super) fn drain_route(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    timers: &mut GeneratedTimerQueue,
    capabilities: &mut GeneratedCapabilityDispatcher,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
    route: VmFixedActorRoute,
) -> Result<(), String> {
    let lease = control.acquire(route, scheduler)?;
    let mut publications = control.drain_identified(&lease)?;
    if publications.len() != 1 {
        let count = publications.len();
        routes.remove(&route.actor_id());
        control.release(lease, VmActorLifecycle::Exiting)?;
        control.reclaim(route)?;
        return Err(format!(
            "error[vm.fixed_scheduler.publication_count]: actor {} received {count} events",
            route.actor_id()
        ));
    }
    let (identity, publication) = publications.pop().expect("exactly one publication");
    telemetry.record_dispatch(publication.dispatched_kind(), &lease, identity)?;
    match publication {
        AotSchedulerPublication::IoCompletion {
            owner,
            suspension,
            wake,
            reply,
        } => {
            let result = execute_interval(telemetry, &lease, || {
                validate_live_route(routes, route, owner).and_then(|()| {
                    shard
                        .resume_io_call(owner, suspension, wake)
                        .and_then(|execution| {
                            advance_slice(shard, owner, execution, timers.observed_tick())
                        })
                })
            });
            finish_execution(
                shard,
                routes,
                runnable,
                timers,
                capabilities,
                control,
                telemetry,
                route,
                lease,
                result,
                reply,
            )?;
        }
        AotSchedulerPublication::Timer {
            owner,
            suspension,
            wait,
            reply,
        } => {
            let result = execute_interval(telemetry, &lease, || {
                validate_live_route(routes, route, owner).and_then(|()| {
                    shard
                        .resume_timer_call(owner, suspension, wait)
                        .and_then(|execution| {
                            advance_slice(shard, owner, execution, timers.observed_tick())
                        })
                })
            });
            finish_execution(
                shard,
                routes,
                runnable,
                timers,
                capabilities,
                control,
                telemetry,
                route,
                lease,
                result,
                reply,
            )?;
        }
        AotSchedulerPublication::CapabilityCompletion {
            owner,
            suspension,
            wait,
            outcome,
            reply,
        } => {
            let result = execute_interval(telemetry, &lease, || {
                validate_live_route(routes, route, owner).and_then(|()| {
                    shard
                        .resume_capability_call(owner, suspension, wait, outcome)
                        .and_then(|execution| {
                            advance_slice(shard, owner, execution, timers.observed_tick())
                        })
                })
            });
            finish_execution(
                shard,
                routes,
                runnable,
                timers,
                capabilities,
                control,
                telemetry,
                route,
                lease,
                result,
                reply,
            )?;
        }
        AotSchedulerPublication::CancellationSignal {
            owner,
            reason,
            reply,
        } => {
            let retained = timers.remove_route(route);
            let (retained_capability, capability_error) = match capabilities.cancel_route(route) {
                Ok(pending) => (pending, None),
                Err((error, pending)) => (Some(pending), Some(error)),
            };
            let mut result = execute_interval(telemetry, &lease, || {
                validate_live_route(routes, route, owner)
                    .and_then(|()| shard.cancel_call(owner, reason.clone()))
            });
            if result.is_ok() {
                if let Some(error) = capability_error {
                    result = Err(error);
                }
            }
            routes.remove(&route.actor_id());
            let result = settle_terminal_owned(control, telemetry, lease, result);
            for pending in retained {
                let _ = pending.reply.send(Err(reason.clone()));
            }
            if let Some(pending) = retained_capability {
                let _ = pending.reply.send(Err(reason.clone()));
            }
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
    }
    Ok(())
}

/// Cancels queued actors before their owner thread exits.
fn cancel_runnable(
    shard: &mut PureNativeExecutionShard,
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    runnable: &mut GeneratedRunnableQueues,
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    scheduler: VmSchedulerId,
    detail: &str,
) {
    while let Some(pending) = runnable.pop_any() {
        let reason = format!("error[vm.scheduler_shutdown]: {detail}");
        let result = control.acquire(pending.route, scheduler).and_then(|lease| {
            let cancelled = shard
                .cancel_call(pending.owner, reason.clone())
                .and(Err(reason.clone()));
            settle_terminal_owned(control, telemetry, lease, cancelled)
        });
        routes.remove(&pending.route.actor_id());
        let _ = pending.reply.send(result);
    }
}

/// Advances generated code until it completes, parks, or cooperatively yields.
fn advance_slice(
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
            PureNativeExecution::HttpResponse(_) => {
                shard.finish_completed_call(owner)?;
                return Err("error[serve.aot.result_projection]: typed HTTP response entered the asynchronous invocation path".to_string());
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Receive =>
            {
                let wait = shard.io_wait(owner, &suspension)?;
                return Ok(ScheduledInvocationStep::Waiting {
                    owner,
                    suspension,
                    wait,
                });
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Timer =>
            {
                let wait = shard.begin_timer_call(owner, &suspension, observed_tick)?;
                return Ok(ScheduledInvocationStep::TimerWaiting {
                    owner,
                    suspension,
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
                    suspension,
                });
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Capability =>
            {
                let wait = shard.begin_capability_call(owner, &suspension)?;
                return Ok(ScheduledInvocationStep::CapabilityWaiting {
                    owner,
                    suspension,
                    wait,
                });
            }
            PureNativeExecution::Suspended(suspension) => {
                execution = shard.resume_call(owner, suspension)?;
            }
        }
    }
}

/// Registers a route only while generated state survives the current slice.
fn register_route(
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    route: VmFixedActorRoute,
    step: ScheduledInvocationStep,
) -> Result<ScheduledInvocationStep, String> {
    let owner = match &step {
        ScheduledInvocationStep::Waiting { owner, .. }
        | ScheduledInvocationStep::TimerWaiting { owner, .. }
        | ScheduledInvocationStep::CapabilityWaiting { owner, .. }
        | ScheduledInvocationStep::Runnable { owner, .. } => Some(*owner),
        ScheduledInvocationStep::Complete(_) => None,
    };
    if let Some(owner) = owner {
        if routes.insert(route.actor_id(), owner).is_some() {
            return Err("error[vm.actor_route]: duplicate live actor route".to_string());
        }
    }
    Ok(step)
}

/// Converts telemetry corruption into the scheduler's fail-stop panic path.
pub(super) fn record_or_panic(
    telemetry: &VmFixedSchedulerTelemetry,
    kind: VmFixedSchedulerEventKind,
    route: Option<VmFixedActorRoute>,
) {
    if let Err(error) = telemetry.record(kind, route) {
        panic!("fixed scheduler telemetry corruption: {error}");
    }
}

/// Rejects duplicate shard-global identities before actor state allocation.
pub(super) fn reject_duplicate_route(
    routes: &BTreeMap<std::num::NonZeroU64, VmProcessId>,
    route: VmFixedActorRoute,
) -> Result<(), String> {
    if routes.contains_key(&route.actor_id()) {
        Err(format!(
            "error[vm.actor_route]: route {} is already live",
            route.actor_id()
        ))
    } else {
        Ok(())
    }
}

/// Validates the shard-global route before touching mutable actor state.
pub(super) fn validate_live_route(
    routes: &BTreeMap<std::num::NonZeroU64, VmProcessId>,
    route: VmFixedActorRoute,
    owner: VmProcessId,
) -> Result<(), String> {
    match routes.get(&route.actor_id()) {
        Some(expected) if *expected == owner => Ok(()),
        Some(expected) => Err(format!(
            "error[vm.actor_route]: route {} owns process {}, not {}",
            route.actor_id(),
            expected.as_u64(),
            owner.as_u64()
        )),
        None => Err(format!(
            "error[vm.actor_route]: route {} is not live",
            route.actor_id()
        )),
    }
}

/// Rejects a command delivered to a scheduler other than its fixed home.
pub(super) fn validate_scheduler_route(
    route: VmFixedActorRoute,
    current: &thread::Thread,
) -> Result<(), String> {
    let expected = format!("terlan-aot-scheduler-{}", route.scheduler().index());
    if current.name() == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "error[vm.actor_route]: route {} reached the wrong scheduler",
            route.actor_id()
        ))
    }
}
