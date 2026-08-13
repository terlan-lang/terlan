//! Scheduler-local capability worker lifecycle for generated handlers.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;

use crate::runtime::vm::actor_directory::VmMailboxWake;
use crate::runtime::vm::capability_worker::{
    VmCapabilityWorkerClient, VmCapabilityWorkerEventPump, VmCapabilityWorkerEventPumpEvent,
    VmCapabilityWorkerGeneration, VmCapabilityWorkerId, VmCapabilityWorkerIdentity,
    VmCapabilityWorkerParkedRequest, VmCapabilityWorkerPolicy, VmCapabilityWorkerPool,
    VmCapabilityWorkerPoolSlot,
};
use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::fixed_scheduler_telemetry::{
    VmFixedSchedulerEventKind, VmFixedSchedulerTelemetry,
};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{
    PureNativeCapabilityWait, PureNativeExecutionShard, PureNativeSuspension,
};
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

use super::owner_loop::{drain_route, ShardOwnerState};
use super::replay_events::settle_terminal;
use super::runnable_queue::GeneratedRunnableQueues;
use super::timer_queue::GeneratedTimerQueue;
use super::{AotSchedulerPublication, OwnedInvocationStep};

/// Maximum external calls retained by one generated scheduler owner.
pub(in crate::commands::serve::handler_cache) const GENERATED_CAPABILITY_CREDITS: u64 = 64;

/// Complete generated invocation state retained while a worker executes.
pub(super) struct PendingGeneratedCapability {
    /// Fixed actor route that must receive completion or cancellation.
    pub(super) route: VmFixedActorRoute,
    /// Exact local actor owning the parked continuation.
    pub(super) owner: VmProcessId,
    /// Generated continuation retained outside native stack memory.
    pub(super) suspension: PureNativeSuspension,
    /// Epoch-qualified capability operation and result type.
    pub(super) wait: PureNativeCapabilityWait,
    /// Original caller settled after generated execution becomes terminal or parks again.
    pub(super) reply: SyncSender<Result<OwnedInvocationStep, String>>,
}

/// Event returned to the scheduler owner without granting actor mutation authority.
pub(super) type GeneratedCapabilityEvent =
    VmCapabilityWorkerEventPumpEvent<PendingGeneratedCapability>;

/// Rare worker-dispatch rejection retaining the exact parked actor envelope.
pub(super) type GeneratedCapabilityFailure = (String, Box<PendingGeneratedCapability>);

/// Lazy scheduler-local capability event pump and route assignment index.
pub(super) struct GeneratedCapabilityDispatcher {
    scheduler: VmSchedulerId,
    enabled: bool,
    pump: Option<VmCapabilityWorkerEventPump<PendingGeneratedCapability>>,
    assignments: BTreeMap<std::num::NonZeroU64, VmCapabilityWorkerParkedRequest>,
}

impl GeneratedCapabilityDispatcher {
    /// Installs dispatcher state without starting an external process eagerly.
    pub(super) fn new(scheduler: VmSchedulerId) -> Self {
        Self {
            scheduler,
            enabled: !cfg!(test) || std::env::var_os("TERLAN_TEST_AOT_CAPABILITY_PUMP").is_some(),
            pump: None,
            assignments: BTreeMap::new(),
        }
    }

    /// Returns whether this test or production scheduler owns automatic dispatch.
    pub(super) const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns whether polling is required to settle retained actors.
    pub(super) fn has_pending(&self) -> bool {
        !self.assignments.is_empty()
    }

    /// Submits one generated capability wait and retains its complete caller envelope.
    pub(super) fn submit(
        &mut self,
        pending: PendingGeneratedCapability,
    ) -> Result<(), GeneratedCapabilityFailure> {
        let context = match pending.wait.worker_context() {
            Ok(context) => context,
            Err(error) => return Err((error, Box::new(pending))),
        };
        let request = pending.wait.request();
        let operation = request.operation.to_string();
        let arguments = request.arguments.clone();
        let route = pending.route;
        let owner = pending.owner;
        let pump = match self.ensure_pump() {
            Ok(pump) => pump,
            Err(error) => return Err((error, Box::new(pending))),
        };
        let assignment = pump
            .submit(owner, context, operation, arguments, pending)
            .map_err(|(error, pending)| (error, Box::new(pending)))?;
        if self
            .assignments
            .insert(route.actor_id(), assignment)
            .is_some()
        {
            panic!("generated capability route acquired duplicate worker assignment");
        }
        Ok(())
    }

    /// Polls one worker transport event and removes completed route ownership.
    pub(super) fn poll(&mut self) -> Result<Option<GeneratedCapabilityEvent>, String> {
        let Some(pump) = self.pump.as_mut() else {
            return Ok(None);
        };
        let event = pump.poll()?;
        if let Some(event) = &event {
            match event {
                VmCapabilityWorkerEventPumpEvent::Completed { payload, .. } => {
                    self.assignments.remove(&payload.route.actor_id());
                }
                VmCapabilityWorkerEventPumpEvent::WorkerLost { pending, .. } => {
                    for (_, payload) in pending {
                        self.assignments.remove(&payload.route.actor_id());
                    }
                }
                VmCapabilityWorkerEventPumpEvent::Ignored { .. } => {}
            }
        }
        Ok(event)
    }

    /// Publishes and drains at most one worker event on this scheduler owner.
    pub(super) fn dispatch_next(
        &mut self,
        shard: &mut PureNativeExecutionShard,
        routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
        runnable: &mut GeneratedRunnableQueues,
        timers: &mut GeneratedTimerQueue,
        control: &VmFixedSchedulerControl<AotSchedulerPublication>,
        telemetry: &VmFixedSchedulerTelemetry,
    ) -> Result<(), String> {
        let Some(event) = self.poll()? else {
            return Ok(());
        };
        match event {
            VmCapabilityWorkerEventPumpEvent::Completed {
                assignment,
                context,
                reply,
                payload,
            } => {
                let expected = payload.wait.worker_context()?;
                let outcome = if assignment.owner == payload.owner && context == expected {
                    reply
                } else {
                    NativeBoundaryReplyTerm::Error {
                        code: "capability.worker_correlation".to_string(),
                        message: "worker completion did not match its parked actor context"
                            .to_string(),
                        offset: 0,
                    }
                };
                self.publish_completion(
                    &mut ShardOwnerState {
                        shard,
                        routes,
                        runnable,
                        timers,
                        control,
                        telemetry,
                        scheduler: self.scheduler,
                    },
                    payload,
                    outcome,
                )?;
            }
            VmCapabilityWorkerEventPumpEvent::WorkerLost {
                worker,
                reason,
                pending,
            } => {
                for (_, payload) in pending {
                    let outcome = NativeBoundaryReplyTerm::Error {
                        code: "capability.worker_lost".to_string(),
                        message: format!(
                            "worker `{}` generation {} failed: {reason}",
                            worker.id.as_str(),
                            worker.generation.as_u64()
                        ),
                        offset: 0,
                    };
                    self.publish_completion(
                        &mut ShardOwnerState {
                            shard,
                            routes,
                            runnable,
                            timers,
                            control,
                            telemetry,
                            scheduler: self.scheduler,
                        },
                        payload,
                        outcome,
                    )?;
                }
            }
            VmCapabilityWorkerEventPumpEvent::Ignored { .. } => {}
        }
        Ok(())
    }

    /// Cancels one route assignment before actor state is released.
    pub(super) fn cancel_route(
        &mut self,
        route: VmFixedActorRoute,
    ) -> Result<Option<PendingGeneratedCapability>, GeneratedCapabilityFailure> {
        let Some(assignment) = self.assignments.remove(&route.actor_id()) else {
            return Ok(None);
        };
        let pump = self
            .pump
            .as_mut()
            .expect("an indexed assignment has an initialized event pump");
        match pump.cancel(&assignment) {
            Ok(pending) => Ok(Some(pending)),
            Err((error, pending)) => Err((error, Box::new(pending))),
        }
    }

    /// Cancels all retained assignments and requests orderly worker shutdown.
    pub(super) fn shutdown(&mut self) -> (Vec<PendingGeneratedCapability>, Vec<String>) {
        self.assignments.clear();
        let Some(pump) = self.pump.as_mut() else {
            return (Vec::new(), Vec::new());
        };
        let (pending, errors) = pump.shutdown();
        (
            pending.into_iter().map(|(_, pending)| pending).collect(),
            errors,
        )
    }

    /// Cancels every retained generated actor before this scheduler exits.
    pub(super) fn cancel_all(
        &mut self,
        shard: &mut PureNativeExecutionShard,
        routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
        control: &VmFixedSchedulerControl<AotSchedulerPublication>,
        telemetry: &VmFixedSchedulerTelemetry,
        detail: &str,
    ) -> Result<(), String> {
        let (pending, mut errors) = self.shutdown();
        let reason = format!("error[vm.scheduler_shutdown]: {detail}");
        for pending in pending {
            let actor_result: Result<OwnedInvocationStep, String> = control
                .acquire(pending.route, self.scheduler)
                .and_then(|lease| {
                    let cancelled = shard
                        .cancel_call(pending.owner, reason.clone())
                        .and(Err(reason.clone()));
                    settle_terminal(control, telemetry, lease, cancelled)
                });
            routes.remove(&pending.route.actor_id());
            if let Err(error) = &actor_result {
                errors.push(error.clone());
            }
            let _ = pending.reply.send(actor_result);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            errors.sort();
            errors.dedup();
            Err(errors.join("; "))
        }
    }

    /// Publishes a complete result before reacquiring and resuming its actor.
    fn publish_completion(
        &mut self,
        state: &mut ShardOwnerState<'_>,
        pending: PendingGeneratedCapability,
        outcome: NativeBoundaryReplyTerm,
    ) -> Result<(), String> {
        let route = pending.route;
        let (identity, wake) = state.control.publish_identified(
            route,
            AotSchedulerPublication::CapabilityCompletion {
                owner: pending.owner,
                suspension: pending.suspension,
                wait: pending.wait,
                outcome,
                reply: pending.reply,
            },
        )?;
        state.telemetry.record_publication(
            VmFixedSchedulerEventKind::CapabilityCompletionPublished,
            route,
            identity,
        )?;
        if wake != VmMailboxWake::Enqueue {
            return Err(format!(
                "error[vm.capability_wakeup]: actor {} was not parked",
                route.actor_id()
            ));
        }
        drain_route(state, self, route)
    }

    /// Lazily starts one sandboxed worker process for this scheduler owner.
    fn ensure_pump(
        &mut self,
    ) -> Result<&mut VmCapabilityWorkerEventPump<PendingGeneratedCapability>, String> {
        if self.pump.is_none() {
            let executable = capability_worker_path()?;
            let mut policy = VmCapabilityWorkerPolicy::new(
                executable,
                NativeBoundaryExecutionProfile::CrashIsolated,
            )?
            .allow("filesystem")
            .allow("stdio")
            .with_credit_limit(GENERATED_CAPABILITY_CREDITS)?;
            if cfg!(test) && std::env::var_os("TERLAN_TEST_CAPABILITY_NETWORK_SANDBOX").is_some() {
                // Some test hosts prohibit creating a network namespace. The
                // production binary cannot enter this test-only branch.
                policy = policy.allow("postgres");
            }
            let id =
                VmCapabilityWorkerId::new(format!("aot-scheduler-{}", self.scheduler.index()))?;
            let generation =
                VmCapabilityWorkerGeneration::new(1).map_err(|error| error.to_string())?;
            let client = VmCapabilityWorkerClient::spawn(
                VmCapabilityWorkerIdentity::new(id, generation),
                policy,
            )?;
            let slot = VmCapabilityWorkerPoolSlot::new(client, GENERATED_CAPABILITY_CREDITS)?;
            let pool = VmCapabilityWorkerPool::new(vec![slot])?;
            self.pump = Some(VmCapabilityWorkerEventPump::new(pool));
        }
        Ok(self.pump.as_mut().expect("event pump initialized"))
    }
}

/// Resolves the native worker packaged next to the current Terlan executable.
pub(in crate::commands::serve::handler_cache) fn capability_worker_path() -> Result<PathBuf, String>
{
    if let Some(path) = std::env::var_os("TERLAN_NATIVE_WORKER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path
                .canonicalize()
                .map_err(|error| format!("failed to canonicalize capability worker: {error}"));
        }
        return Err(format!(
            "TERLAN_NATIVE_WORKER points to missing runtime `{}`",
            path.display()
        ));
    }
    let current = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current Terlan executable: {error}"))?;
    let name = if cfg!(windows) {
        "terlan-native-worker.exe"
    } else {
        "terlan-native-worker"
    };
    let mut directory = current.parent();
    for _ in 0..3 {
        let Some(parent) = directory else {
            break;
        };
        let candidate = parent.join(name);
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .map_err(|error| format!("failed to canonicalize capability worker: {error}"));
        }
        directory = parent.parent();
    }
    Err(format!(
        "error[serve.aot.capability_worker_missing]: `{name}` is not packaged with the current Terlan executable"
    ))
}
