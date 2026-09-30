//! Scheduler-local capability worker lifecycle for generated handlers.

use crate::runtime::vm::package_native_helper::{VmOwnedNativeDispatcher, VmPackageNativeHelpers};
use crate::runtime::vm::pure_native::repl_value_to_boundary_term;
use std::collections::{BTreeMap, VecDeque};
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
    helpers: VmPackageNativeHelpers,
    native: VmOwnedNativeDispatcher,
    native_pending: BTreeMap<std::num::NonZeroU64, PendingGeneratedCapability>,
    resource_owners: BTreeMap<std::num::NonZeroU64, VmProcessId>,
    ready: VecDeque<(PendingGeneratedCapability, NativeBoundaryReplyTerm)>,
}

impl GeneratedCapabilityDispatcher {
    /// Installs dispatcher state without starting an external process eagerly.
    pub(super) fn new(scheduler: VmSchedulerId) -> Self {
        Self {
            scheduler,
            enabled: !cfg!(test) || std::env::var_os("TERLAN_TEST_AOT_CAPABILITY_PUMP").is_some(),
            pump: None,
            assignments: BTreeMap::new(),
            helpers: VmPackageNativeHelpers::default(),
            native: VmOwnedNativeDispatcher::default(),
            native_pending: BTreeMap::new(),
            resource_owners: BTreeMap::new(),
            ready: VecDeque::new(),
        }
    }

    /// Returns whether this test or production scheduler owns automatic dispatch.
    pub(super) const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns whether polling is required to settle retained actors.
    pub(super) fn has_pending(&self) -> bool {
        !self.assignments.is_empty() || !self.native_pending.is_empty() || !self.ready.is_empty()
    }

    /// Submits one generated capability wait and retains its complete caller envelope.
    pub(super) fn submit(
        &mut self,
        pending: PendingGeneratedCapability,
    ) -> Result<(), GeneratedCapabilityFailure> {
        let trusted = super::super::protocol_capability::trusted_host_capability(&pending.wait);
        let database = pending
            .wait
            .request()
            .operation
            .starts_with("std.db.postgres.");
        let resource =
            crate::std_native_packages::resource_operation(&pending.wait.request().operation)
                .is_some();
        if database || (resource && !trusted) {
            if database && !trusted {
                return Err(("error[serve.aot.capability_denied]: database access requires trusted host capabilities".into(), Box::new(pending)));
            }
            let worker = match capability_worker_path() {
                Ok(worker) => worker,
                Err(error) => return Err((error.to_string(), Box::new(pending))),
            };
            match self
                .native
                .submit(&mut self.helpers, pending.owner, &pending.wait, worker)
            {
                Ok(true) => {
                    self.resource_owners
                        .insert(pending.route.actor_id(), pending.owner);
                    self.native_pending
                        .insert(pending.route.actor_id(), pending);
                    return Ok(());
                }
                Ok(false) => unreachable!("owned operation was checked"),
                Err(error) => return Err((error.into(), Box::new(pending))),
            }
        }
        if trusted
            && matches!(
                pending.wait.request().capability.as_str(),
                "package-native" | "system.environment"
            )
        {
            let request = pending.wait.request();
            let outcome = if request.capability == "package-native" {
                self.helpers
                    .call(
                        pending.owner.as_u64(),
                        request,
                        pending.wait.admitted_atoms(),
                    )
                    .and_then(repl_value_to_boundary_term)
                    .map(NativeBoundaryReplyTerm::Ok)
            } else {
                crate::runtime::vm::package_native_helper::dispatch_vm_capability_with_program_arguments(request, &[])
            };
            match outcome {
                Ok(outcome) => {
                    self.resource_owners
                        .insert(pending.route.actor_id(), pending.owner);
                    self.ready.push_back((pending, outcome));
                    return Ok(());
                }
                Err(error) => return Err((error.into(), Box::new(pending))),
            }
        }
        let context = match pending.wait.worker_context() {
            Ok(context) => context,
            Err(error) => return Err((error, Box::new(pending))),
        };
        let request = pending.wait.request();
        let operation = request.operation.to_string();
        let arguments = match request.boundary_arguments() {
            Ok(arguments) => arguments.into_owned(),
            Err(error) => return Err((error.to_string(), Box::new(pending))),
        };
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
        if let Some((owner, outcome)) = self.native.poll(&mut self.helpers).map_err(String::from)? {
            if let Some(actor) = self
                .native_pending
                .iter()
                .find_map(|(actor, pending)| (pending.owner == owner).then_some(*actor))
            {
                let pending = self
                    .native_pending
                    .remove(&actor)
                    .expect("located native resource owner");
                self.ready.push_back((pending, outcome));
            }
        }
        if let Some((pending, outcome)) = self.ready.pop_front() {
            return self.publish_completion(
                &mut ShardOwnerState {
                    shard,
                    routes,
                    runnable,
                    timers,
                    control,
                    telemetry,
                    scheduler: self.scheduler,
                },
                pending,
                outcome,
            );
        }
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
        self.close_route(route);
        if let Some(pending) = self.native_pending.remove(&route.actor_id()) {
            return Ok(Some(pending));
        }
        if let Some(index) = self
            .ready
            .iter()
            .position(|(pending, _)| pending.route == route)
        {
            return Ok(self.ready.remove(index).map(|(pending, _)| pending));
        }
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
        for (_, owner) in std::mem::take(&mut self.resource_owners) {
            self.native.close_owner(&mut self.helpers, owner);
        }
        let mut pending = std::mem::take(&mut self.native_pending)
            .into_values()
            .collect::<Vec<_>>();
        pending.extend(self.ready.drain(..).map(|(pending, _)| pending));
        let Some(pump) = self.pump.as_mut() else {
            return (pending, Vec::new());
        };
        let (external, errors) = pump.shutdown();
        pending.extend(external.into_iter().map(|(_, pending)| pending));
        (pending, errors)
    }

    /// Retires the actor route together with its capability resources.
    pub(super) fn finish_route(
        &mut self,
        route: VmFixedActorRoute,
        routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    ) {
        self.close_route(route);
        routes.remove(&route.actor_id());
    }

    /// Releases database and package resources at the terminal actor boundary.
    pub(super) fn close_route(&mut self, route: VmFixedActorRoute) {
        if let Some(owner) = self.resource_owners.remove(&route.actor_id()) {
            self.native.close_owner(&mut self.helpers, owner);
        }
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
            let mut policy = capability_worker_policy()?;
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

/// Shares bounded value-binding authority across HTTP scheduler implementations.
pub(in crate::commands::serve::handler_cache) fn capability_worker_policy(
) -> Result<VmCapabilityWorkerPolicy, String> {
    VmCapabilityWorkerPolicy::new(
        capability_worker_path().map_err(|error| error.to_string())?,
        NativeBoundaryExecutionProfile::CrashIsolated,
    )?
    .allow("filesystem")
    .allow("clock")
    .allow("stdio")
    .allow("package-native")
    .admit_worker_class("fast")
    .with_credit_limit(GENERATED_CAPABILITY_CREDITS)
}

/// Resolves the native worker packaged next to the current Terlan executable.
pub(in crate::commands::serve::handler_cache) fn capability_worker_path() -> std::io::Result<PathBuf>
{
    if let Some(path) = std::env::var_os("TERLAN_NATIVE_WORKER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path.canonicalize();
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "TERLAN_NATIVE_WORKER points to missing runtime `{}`",
                path.display()
            ),
        ));
    }
    find_worker(&std::env::current_exe()?)
}

fn find_worker(current: &std::path::Path) -> std::io::Result<PathBuf> {
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
            return candidate.canonicalize();
        }
        directory = parent.parent();
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!(
        "error[serve.aot.capability_worker_missing]: `{name}` is not packaged with the current Terlan executable"
    )))
}

#[cfg(test)]
#[path = "capability_worker_path_test.rs"]
mod worker_path_tests;
