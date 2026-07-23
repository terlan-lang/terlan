//! Native-only dynamic HTTP handler image cache.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

use crate::commands::artifacts::fingerprint;
use crate::compiler::router::AotRouterPlan;
use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::http_router::{VmHttpCompiledCallableRef, VmHttpRouter};
use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::protocol_task_executor::{
    retire_protocol_resource, with_current_protocol_resource,
};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::scheduler_topology::{
    VmFixedActorRoute, VmSchedulerId, VmSchedulerTopology,
};
use crate::runtime::vm::work_stealing::{
    VmWorkDirective, VmWorkStealingConfig, VmWorkStealingPolicy,
};
use crate::runtime::vm::ReplValue;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

use super::handler::WebPackageHandler;
use super::source_path_from_manifest;

mod http_response;
mod immediate;
pub(super) mod invocation;
mod replay_evidence;
mod request_projection;
mod router_materialization;
mod shard_owner;
mod source_generation;

use immediate::{finish_immediate_step, LocalImmediateShard};
use router_materialization::materialize_router;

const CACHE_ERROR: &str = "error[serve.aot.cache]: native handler cache lock poisoned";

static HANDLER_CACHE: OnceLock<RwLock<HashMap<PathBuf, HandlerCacheEntry>>> = OnceLock::new();
static HANDLER_CACHE_EPOCH: AtomicU64 = AtomicU64::new(1);
static NEXT_HANDLER_GENERATION: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static LOCAL_HANDLER_CACHE: RefCell<LocalHandlerCache> =
        RefCell::new(LocalHandlerCache::default());
}

#[derive(Default)]
struct LocalHandlerCache {
    epoch: u64,
    last: Option<LocalHandlerRuntime>,
    runtimes: HashMap<PathBuf, Weak<AotHandlerRuntime>>,
}

struct LocalHandlerRuntime {
    web_root: PathBuf,
    source: String,
    runtime: Arc<AotHandlerRuntime>,
}

#[cfg(test)]
#[path = "handler_cache_generation_test.rs"]
mod handler_cache_generation_test;
#[path = "handler_cache_test_support.rs"]
pub(super) mod handler_cache_test_support;
#[cfg(test)]
#[path = "handler_cache/multicore_performance_test.rs"]
mod multicore_performance_test;

/// One admitted native handler image. It never owns or retains compiler IR.
#[derive(Debug)]
pub(super) struct AotHandlerRuntime {
    module: String,
    generation: Arc<AotHandlerGeneration>,
    router: Option<VmHttpRouter>,
    primary_request_projection: Option<PrimaryRequestProjection>,
    request_projections:
        HashMap<String, HashMap<usize, crate::runtime::native::http::RequestFieldProjection>>,
}

#[derive(Debug)]
struct PrimaryRequestProjection {
    function: String,
    arity: usize,
    fields: crate::runtime::native::http::RequestFieldProjection,
}

/// One admitted handler generation and its long-lived actor execution shards.
///
/// The immutable image factory and mutable shard share this lifetime so parked
/// continuations keep their exact native generation mapped across hot reloads.
pub(super) struct AotHandlerGeneration {
    identity: u64,
    image: PureNativeExecutionImage,
    shards: Vec<shard_owner::AotHandlerShardOwner>,
    scheduler_control: Arc<VmFixedSchedulerControl<shard_owner::AotSchedulerPublication>>,
    active_actors: Vec<AtomicUsize>,
    next_actor_route: AtomicU64,
    work_policy: Mutex<VmWorkStealingPolicy>,
    work_metrics: AotGeneratedWorkMetrics,
}

/// Shard-wide generated queue coordination counters.
#[derive(Debug, Default)]
struct AotGeneratedWorkMetrics {
    steal_attempts: AtomicU64,
    transferred: AtomicU64,
    failed_steals: AtomicU64,
    backoff_directives: AtomicU64,
    priority_transferred: AtomicU64,
    normal_transferred: AtomicU64,
    background_transferred: AtomicU64,
}

/// Immutable generated queue coordination evidence.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AotGeneratedWorkMetricsSnapshot {
    steal_attempts: u64,
    transferred: u64,
    failed_steals: u64,
    backoff_directives: u64,
    priority_transferred: u64,
    normal_transferred: u64,
    background_transferred: u64,
}

impl AotHandlerGeneration {
    fn load(image: &Path, sessions: VmHttpSessionService) -> Result<Self, String> {
        let scheduler_count = VmSchedulerTopology::from_environment()?.width();
        Self::load_with_shard_count(image, sessions, scheduler_count)
    }

    fn load_with_shard_count(
        image: &Path,
        sessions: VmHttpSessionService,
        shard_count: usize,
    ) -> Result<Self, String> {
        Self::load_with_shard_count_and_failure(image, sessions, shard_count, None)
    }

    /// Injects a deterministic partial-startup failure for lifecycle tests.
    #[cfg(test)]
    fn load_with_start_failure(
        image: &Path,
        sessions: VmHttpSessionService,
        shard_count: usize,
        fail_at: usize,
    ) -> Result<Self, String> {
        Self::load_with_shard_count_and_failure(image, sessions, shard_count, Some(fail_at))
    }

    /// Starts one complete fixed-scheduler generation or tears it all down.
    fn load_with_shard_count_and_failure(
        image: &Path,
        sessions: VmHttpSessionService,
        shard_count: usize,
        #[cfg_attr(not(test), allow(unused_variables))] fail_at: Option<usize>,
    ) -> Result<Self, String> {
        if shard_count == 0 {
            return Err(
                "error[serve.aot.shard_count]: at least one execution shard is required"
                    .to_string(),
            );
        }
        let topology = VmSchedulerTopology::new(shard_count)?;
        let identity = NEXT_HANDLER_GENERATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |identity| {
                identity.checked_add(1)
            })
            .map_err(|_| "error[serve.aot.generation]: identity exhausted".to_string())?;
        let image = PureNativeExecutionImage::load_with_http_sessions(image, sessions)?;
        let scheduler_control = Arc::new(VmFixedSchedulerControl::default());
        let scheduler_failure = Arc::new(Mutex::new(None));
        let mut shards = Vec::with_capacity(shard_count);
        for scheduler in topology.schedulers() {
            #[cfg(test)]
            if fail_at == Some(scheduler.index()) {
                let error = format!(
                    "error[serve.aot.shard_start_injected]: scheduler {} startup failed",
                    scheduler.index()
                );
                abort_partial_startup(
                    &shards,
                    scheduler_control.as_ref(),
                    scheduler_failure.as_ref(),
                    &error,
                );
                return Err(error);
            }
            let shard = image.spawn_shard_on_scheduler(scheduler)?;
            match shard_owner::AotHandlerShardOwner::spawn(
                scheduler,
                shard,
                Arc::clone(&scheduler_control),
                Arc::clone(&scheduler_failure),
            ) {
                Ok(owner) => shards.push(owner),
                Err(error) => {
                    abort_partial_startup(
                        &shards,
                        scheduler_control.as_ref(),
                        scheduler_failure.as_ref(),
                        &error,
                    );
                    return Err(error);
                }
            }
        }
        Ok(Self {
            identity,
            image,
            shards,
            scheduler_control,
            active_actors: (0..shard_count).map(|_| AtomicUsize::new(0)).collect(),
            next_actor_route: AtomicU64::new(1),
            work_policy: Mutex::new(VmWorkStealingPolicy::new(
                shard_count,
                VmWorkStealingConfig::default(),
            )?),
            work_metrics: AotGeneratedWorkMetrics::default(),
        })
    }

    /// Reserves the deterministic home scheduler for a new actor route.
    ///
    /// Stable actors never call this again. Monotonic shard-global identities
    /// distribute evenly while every continuation remains pinned to its home.
    fn route_new_actor(&self) -> Result<VmFixedActorRoute, String> {
        let actor = self
            .next_actor_route
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| "error[vm.actor_route]: actor route identity exhausted".to_string())?;
        let actor = NonZeroU64::new(actor)
            .ok_or_else(|| "error[vm.actor_route]: actor route identity is zero".to_string())?;
        let topology = VmSchedulerTopology::new(self.shards.len())?;
        let route = topology.route(actor);
        let index = route.scheduler().index();
        self.active_actors[index].fetch_add(1, Ordering::Relaxed);
        Ok(route)
    }

    /// Reserves an identity whose immutable home is the calling VM owner.
    fn route_new_actor_on(&self, scheduler: VmSchedulerId) -> Result<VmFixedActorRoute, String> {
        let width = u64::try_from(self.shards.len())
            .map_err(|_| "error[vm.actor_route]: shard width overflow".to_string())?;
        if scheduler.index() >= self.shards.len() {
            return Err(format!(
                "error[vm.actor_route]: scheduler {} is not admitted by width {}",
                scheduler.index(),
                self.shards.len()
            ));
        }
        let base = self
            .next_actor_route
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(width)
            })
            .map_err(|_| "error[vm.actor_route]: actor route identity exhausted".to_string())?;
        let current_home = (base - 1) % width;
        let offset = (scheduler.index() as u64 + width - current_home) % width;
        let actor = base
            .checked_add(offset)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| "error[vm.actor_route]: actor route identity overflow".to_string())?;
        let topology = VmSchedulerTopology::new(self.shards.len())?;
        let route = topology.route(actor);
        debug_assert_eq!(route.scheduler(), scheduler);
        self.active_actors[scheduler.index()].fetch_add(1, Ordering::Relaxed);
        Ok(route)
    }

    fn release_actor_route(&self, index: usize) {
        let prior = self.active_actors[index].fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prior > 0, "actor route reservation underflow");
    }

    fn shard(&self, index: usize) -> Result<&shard_owner::AotHandlerShardOwner, String> {
        self.shards.get(index).ok_or_else(|| {
            format!("error[serve.aot.shard_route]: execution shard {index} is not admitted")
        })
    }

    /// Moves one parked actor through the generation-qualified owner protocol.
    fn migrate_actor(
        &self,
        source: VmFixedActorRoute,
        owner: crate::runtime::vm::process::VmProcessId,
        destination_index: usize,
    ) -> Result<VmFixedActorRoute, String> {
        let source_index = source.scheduler().index();
        if source_index == destination_index {
            return Ok(source);
        }
        let source_owner = self.shard(source_index)?;
        let destination_owner = self.shard(destination_index)?;
        let ticket = self
            .scheduler_control
            .begin_migration(source, destination_owner.scheduler())?;
        let destination = ticket.destination();
        let transfer = match source_owner.detach_migration(source, owner) {
            Ok(transfer) => transfer,
            Err(reason) => {
                let abort = self.scheduler_control.abort_migration(ticket);
                return Err(combine_migration_error(reason, abort.err()));
            }
        };
        match destination_owner.import_migration(destination, transfer) {
            Ok(()) => match self.scheduler_control.complete_migration(ticket) {
                Ok(route) => {
                    let prior = self.active_actors[source_index].fetch_sub(1, Ordering::Relaxed);
                    debug_assert!(prior > 0, "migration source reservation underflow");
                    self.active_actors[destination_index].fetch_add(1, Ordering::Relaxed);
                    Ok(route)
                }
                Err(reason) => {
                    let rollback = destination_owner
                        .detach_migration(destination, owner)
                        .and_then(|transfer| {
                            source_owner
                                .import_migration(source, transfer)
                                .map_err(|failure| failure.reason().to_string())
                        });
                    Err(combine_migration_error(reason, rollback.err()))
                }
            },
            Err(failure) => {
                let reason = failure.reason().to_string();
                let rollback = match failure.into_transfer() {
                    Some(transfer) => source_owner
                        .import_migration(source, transfer)
                        .map_err(|failure| failure.reason().to_string()),
                    None => Err(
                        "error[serve.aot.migration_lost]: destination owner lost actor transfer"
                            .to_string(),
                    ),
                };
                let abort = self.scheduler_control.abort_migration(ticket);
                Err(combine_migration_error(
                    combine_migration_error(reason, rollback.err()),
                    abort.err(),
                ))
            }
        }
    }

    /// Moves at most one queued generated continuation between owner threads.
    fn steal_one_runnable_in_class(
        &self,
        source_index: usize,
        destination_index: usize,
        class: VmSchedulerClass,
    ) -> Result<Option<VmFixedActorRoute>, String> {
        if source_index == destination_index {
            return Err(
                "error[vm.work_stealing.destination]: source and destination match".to_string(),
            );
        }
        let source_owner = self.shard(source_index)?;
        let destination_owner = self.shard(destination_index)?;
        let Some(transfer) =
            source_owner.detach_runnable_to(destination_owner.scheduler(), class)?
        else {
            return Ok(None);
        };
        let destination = transfer.destination();
        self.move_actor_reservation(source_index, destination_index);
        match destination_owner.import_runnable(destination, transfer) {
            Ok(()) => Ok(Some(destination)),
            Err(failure) => {
                self.move_actor_reservation(destination_index, source_index);
                let reason = failure.reason().to_string();
                let Some(transfer) = failure.into_transfer() else {
                    return Err(format!(
                        "{reason}; error[vm.work_stealing.transfer_lost]: destination owner consumed actor envelope"
                    ));
                };
                let route_rollback = self
                    .scheduler_control
                    .begin_migration(destination, source_owner.scheduler())
                    .and_then(|ticket| self.scheduler_control.complete_migration(ticket));
                let rollback = match route_rollback {
                    Ok(route) => source_owner
                        .import_runnable(route, transfer)
                        .map_err(|failure| failure.reason().to_string()),
                    Err(error) => Err(error),
                };
                Err(combine_migration_error(reason, rollback.err()))
            }
        }
    }

    /// Applies bounded policy decisions to live generated owner snapshots.
    fn rebalance_generated_queues(&self) {
        let snapshots = match self
            .shards
            .iter()
            .map(shard_owner::AotHandlerShardOwner::runnable_snapshot)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(snapshots) => snapshots,
            Err(_) => return,
        };
        let Ok(mut policy) = self.work_policy.lock() else {
            return;
        };
        for thief in self.shards.iter().map(|owner| owner.scheduler()) {
            let Ok(directive) = policy.decide(thief, &snapshots) else {
                return;
            };
            match directive {
                VmWorkDirective::Steal(plan) => {
                    self.work_metrics
                        .steal_attempts
                        .fetch_add(1, Ordering::Relaxed);
                    let mut transferred = 0;
                    for _ in 0..plan.maximum_actors() {
                        match self.steal_one_runnable_in_class(
                            plan.victim().index(),
                            plan.thief().index(),
                            plan.class(),
                        ) {
                            Ok(Some(_)) => transferred += 1,
                            Ok(None) => break,
                            Err(_) => {
                                self.work_metrics
                                    .failed_steals
                                    .fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                    self.work_metrics
                        .transferred
                        .fetch_add(transferred as u64, Ordering::Relaxed);
                    let class_counter = match plan.class() {
                        VmSchedulerClass::Priority => &self.work_metrics.priority_transferred,
                        VmSchedulerClass::Normal => &self.work_metrics.normal_transferred,
                        VmSchedulerClass::Background => &self.work_metrics.background_transferred,
                    };
                    class_counter.fetch_add(transferred as u64, Ordering::Relaxed);
                    let _ = policy.record_steal_result(thief, transferred);
                }
                VmWorkDirective::Backoff(_) => {
                    self.work_metrics
                        .backoff_directives
                        .fetch_add(1, Ordering::Relaxed);
                }
                VmWorkDirective::ServeLocal(_) | VmWorkDirective::Sleep => {}
            }
        }
    }

    /// Returns generated queue coordination evidence for focused tests.
    #[cfg(test)]
    fn work_metrics(&self) -> AotGeneratedWorkMetricsSnapshot {
        AotGeneratedWorkMetricsSnapshot {
            steal_attempts: self.work_metrics.steal_attempts.load(Ordering::Relaxed),
            transferred: self.work_metrics.transferred.load(Ordering::Relaxed),
            failed_steals: self.work_metrics.failed_steals.load(Ordering::Relaxed),
            backoff_directives: self.work_metrics.backoff_directives.load(Ordering::Relaxed),
            priority_transferred: self
                .work_metrics
                .priority_transferred
                .load(Ordering::Relaxed),
            normal_transferred: self.work_metrics.normal_transferred.load(Ordering::Relaxed),
            background_transferred: self
                .work_metrics
                .background_transferred
                .load(Ordering::Relaxed),
        }
    }

    /// Transfers one live actor reservation before destination service starts.
    fn move_actor_reservation(&self, source: usize, destination: usize) {
        let prior = self.active_actors[source].fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prior > 0, "actor reservation transfer underflow");
        self.active_actors[destination].fetch_add(1, Ordering::Relaxed);
    }

    fn has_export(&self, function: &str, arity: usize) -> bool {
        self.image.has_export(function, arity)
    }

    #[cfg(test)]
    fn completed_call_count(&self) -> Result<u64, String> {
        self.shards.iter().try_fold(0_u64, |total, shard| {
            shard.completed_call_count().and_then(|count| {
                total.checked_add(count).ok_or_else(|| {
                    "error[serve.aot.completed_count]: call count overflow".to_string()
                })
            })
        })
    }
}

/// Appends rollback context without hiding the primary migration failure.
fn combine_migration_error(primary: String, rollback: Option<String>) -> String {
    match rollback {
        Some(rollback) => format!("{primary}; error[serve.aot.migration_rollback]: {rollback}"),
        None => primary,
    }
}

impl Drop for AotHandlerGeneration {
    fn drop(&mut self) {
        let _ = retire_protocol_resource(self.identity);
        for shard in &self.shards {
            let _ = shard.shutdown();
        }
        let _ = self.scheduler_control.shutdown();
    }
}

/// Closes admission and joins every owner started before a startup failure.
fn abort_partial_startup(
    shards: &[shard_owner::AotHandlerShardOwner],
    control: &VmFixedSchedulerControl<shard_owner::AotSchedulerPublication>,
    failure: &Mutex<Option<String>>,
    error: &str,
) {
    if let Ok(mut current) = failure.lock() {
        *current = Some(format!("partial scheduler startup: {error}"));
    }
    for owner in shards {
        let _ = owner.shutdown();
    }
    let _ = control.shutdown();
}

impl AotHandlerRuntime {
    #[allow(dead_code)] // Retained for direct runtime admission outside the cache.
    pub(in crate::commands::serve) fn load(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
    ) -> Result<Self, String> {
        let sessions = VmHttpSessionService::new(VmHttpSessionRuntime::new("terlc-serve", 86_400)?);
        Ok(Self {
            module,
            generation: Arc::new(AotHandlerGeneration::load(image, sessions)?),
            router: router.map(materialize_router).transpose()?,
            primary_request_projection: None,
            request_projections: HashMap::new(),
        })
    }

    #[cfg(test)]
    fn load_with_shard_count(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
        shard_count: usize,
    ) -> Result<Self, String> {
        let sessions = VmHttpSessionService::new(VmHttpSessionRuntime::new("terlc-serve", 86_400)?);
        Ok(Self {
            module,
            generation: Arc::new(AotHandlerGeneration::load_with_shard_count(
                image,
                sessions,
                shard_count,
            )?),
            router: router.map(materialize_router).transpose()?,
            primary_request_projection: None,
            request_projections: HashMap::new(),
        })
    }

    pub(super) fn has_function(&self, module: &str, function: &str, arity: usize) -> bool {
        if module != self.module {
            return false;
        }
        // Router execution is descriptor-driven; a raw generated `router/0`
        // export without its admitted static plan is not executable through
        // `execute_http_router`. Answer from that plan directly and avoid a
        // formatted export lookup on every ordinary handler request.
        if function == "router" && arity == 0 {
            return self.router.is_some();
        }
        self.generation
            .has_export(&format!("{module}.{function}"), arity)
    }

    /// Executes one native callback that is required to complete immediately.
    ///
    /// A suspended callback is cancelled under its request-owned shard and
    /// rejected. External wake values may enter only through the typed
    /// invocation APIs used by request and channel event pumps.
    pub(super) fn execute_immediate_native(
        &self,
        module: &str,
        function: &str,
        args: Vec<ReplValue>,
        _output: &mut dyn FnMut(&str),
    ) -> Result<ReplValue, String> {
        if module != self.module {
            return Err(format!(
                "error[serve.aot.module_missing]: native handler image `{}` does not own module `{module}`",
                self.module
            ));
        }
        if let Some(value) = with_current_protocol_resource(
            self.generation.identity,
            |scheduler| {
                LocalImmediateShard::new(
                    self.generation.image.spawn_shard_on_scheduler(scheduler)?,
                    module,
                    function,
                    args.len(),
                )
            },
            |local: &mut LocalImmediateShard| local.call(module, function, &args),
        )? {
            return Ok(value);
        }
        finish_immediate_step(self.begin_request_invocation(module, function, args)?)
    }

    /// Executes the common one-argument handler shape through owner-local
    /// argument and export storage.
    #[allow(dead_code)] // Retained for single-argument generated call sites.
    pub(super) fn execute_immediate_native_one(
        &self,
        module: &str,
        function: &str,
        argument: ReplValue,
        _output: &mut dyn FnMut(&str),
    ) -> Result<ReplValue, String> {
        if module != self.module {
            return Err(format!(
                "error[serve.aot.module_missing]: native handler image `{}` does not own module `{module}`",
                self.module
            ));
        }
        let mut argument = Some(argument);
        if let Some(value) = with_current_protocol_resource(
            self.generation.identity,
            |scheduler| {
                LocalImmediateShard::new(
                    self.generation.image.spawn_shard_on_scheduler(scheduler)?,
                    module,
                    function,
                    1,
                )
            },
            |local: &mut LocalImmediateShard| {
                local.call_one(
                    module,
                    function,
                    argument
                        .take()
                        .expect("one immediate argument is consumed exactly once"),
                )
            },
        )? {
            return Ok(value);
        }
        finish_immediate_step(self.begin_request_invocation(
            module,
            function,
            vec![argument.expect("ambient fallback retains its immediate argument")],
        )?)
    }

    pub(super) fn execute_callable(
        &self,
        module: &str,
        callable: &ReplValue,
        args: Vec<ReplValue>,
        output: &mut dyn FnMut(&str),
    ) -> Result<ReplValue, String> {
        let callable = VmHttpCompiledCallableRef::from_value(callable).ok_or_else(|| {
            "error[serve.aot.callable]: router value is not a static native callback".to_string()
        })?;
        if callable.module != module || args.len() != callable.arity {
            return Err(format!(
                "error[serve.aot.callable]: callback `{}.{}/{}` cannot be invoked as `{module}` with {} arguments",
                callable.module,
                callable.function,
                callable.arity,
                args.len()
            ));
        }
        self.execute_immediate_native(module, &callable.function, args, output)
            .map_err(|error| {
                format!(
                    "error[serve.aot.callable]: callback `{}.{}/{}` failed: {error}",
                    callable.module, callable.function, callable.arity
                )
            })
    }

    pub(super) fn callable_arity(&self, callable: &ReplValue) -> Option<usize> {
        VmHttpCompiledCallableRef::from_value(callable).map(|callable| callable.arity)
    }

    pub(super) fn execute_http_router(
        &self,
        module: &str,
        function: &str,
        _output: &mut dyn FnMut(&str),
    ) -> Result<VmHttpRouter, String> {
        if module != self.module || function != "router" {
            return Err(format!(
                "error[serve.aot.router]: native router `{module}.{function}/0` is not loaded"
            ));
        }
        self.router.clone().ok_or_else(|| {
            format!("error[serve.aot.router]: module `{module}` has no static router plan")
        })
    }

    #[cfg(test)]
    fn completed_call_count(&self) -> Result<u64, String> {
        self.generation.completed_call_count()
    }
}

#[derive(Clone)]
struct HandlerCacheEntry {
    checksum: String,
    runtime: Arc<AotHandlerRuntime>,
}

/// Request lease over one immutable native handler generation.
pub(super) struct VmHandlerRuntimeLease(Arc<AotHandlerRuntime>);

impl VmHandlerRuntimeLease {
    pub(super) fn vm(&self) -> &AotHandlerRuntime {
        &self.0
    }
}

pub(super) fn cached_vm_handler_for_manifest(
    web_root: &Path,
    project_root: &Path,
    handler: &WebPackageHandler,
) -> Result<Arc<AotHandlerRuntime>, String> {
    cached_runtime_for_manifest(web_root, project_root, handler)
}

pub(super) fn cached_vm_handler_runtime_for_manifest(
    web_root: &Path,
    project_root: &Path,
    handler: &WebPackageHandler,
) -> Result<VmHandlerRuntimeLease, String> {
    Ok(VmHandlerRuntimeLease(cached_runtime_for_manifest(
        web_root,
        project_root,
        handler,
    )?))
}

/// Resolves the common request handler without repeating project/path lookup.
pub(super) fn cached_vm_handler_runtime_for_request(
    web_root: &Path,
    handler: &WebPackageHandler,
) -> Result<VmHandlerRuntimeLease, String> {
    let source = handler.source.as_ref().ok_or_else(|| {
        format!(
            "error[serve.aot.source_missing]: dynamic handler `{}.{}/{}` has no source metadata",
            handler.module, handler.function, handler.arity
        )
    })?;
    if let Some(runtime) = local_cached_runtime_for_handler(web_root, &source.path) {
        return Ok(VmHandlerRuntimeLease(runtime));
    }
    let project_root = super::manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: dynamic handlers require an adjacent project root".to_string()
    })?;
    Ok(VmHandlerRuntimeLease(cached_runtime_for_manifest(
        web_root,
        &project_root,
        handler,
    )?))
}

/// Executes through the active owner-local generation without incrementing
/// its shared reference count on every request.
pub(super) fn with_cached_vm_handler_runtime_for_request<R>(
    web_root: &Path,
    handler: &WebPackageHandler,
    operation: impl FnOnce(&AotHandlerRuntime) -> R,
) -> Result<R, String> {
    let source = handler.source.as_ref().ok_or_else(|| {
        format!(
            "error[serve.aot.source_missing]: dynamic handler `{}.{}/{}` has no source metadata",
            handler.module, handler.function, handler.arity
        )
    })?;
    let epoch = HANDLER_CACHE_EPOCH.load(Ordering::Acquire);
    let mut operation = Some(operation);
    if let Some(result) = LOCAL_HANDLER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.epoch != epoch {
            cache.last = None;
            cache.runtimes.clear();
            cache.epoch = epoch;
        }
        let last = cache.last.as_ref()?;
        (last.web_root == web_root && last.source == source.path).then(|| {
            operation
                .take()
                .expect("owner-local handler operation runs exactly once")(
                last.runtime.as_ref()
            )
        })
    }) {
        return Ok(result);
    }
    let runtime = cached_vm_handler_runtime_for_request(web_root, handler)?;
    Ok(operation
        .take()
        .expect("cache miss retains the handler operation")(
        runtime.vm(),
    ))
}

fn cached_runtime_for_manifest(
    web_root: &Path,
    project_root: &Path,
    handler: &WebPackageHandler,
) -> Result<Arc<AotHandlerRuntime>, String> {
    let source = handler.source.as_ref().ok_or_else(|| {
        format!(
            "error[serve.aot.source_missing]: dynamic handler `{}.{}/{}` has no source metadata",
            handler.module, handler.function, handler.arity
        )
    })?;
    if let Some(runtime) = local_cached_runtime_for_handler(web_root, &source.path) {
        return Ok(runtime);
    }
    let source_path = source_path_from_manifest(project_root, &source.path).ok_or_else(|| {
        format!(
            "error[serve.aot.source_path]: dynamic handler source path `{}` is unsafe",
            source.path
        )
    })?;
    if let Some(runtime) = local_cached_runtime(&source_path) {
        remember_local_handler_runtime(web_root, &source.path, &runtime);
        return Ok(runtime);
    }
    let entry = cached_source_entry(web_root, &source_path, &handler.module)?;
    remember_local_runtime(source_path, &entry.runtime);
    remember_local_handler_runtime(web_root, &source.path, &entry.runtime);
    Ok(entry.runtime)
}

/// Hits the most recently used handler by borrowed request metadata only.
fn local_cached_runtime_for_handler(
    web_root: &Path,
    source: &str,
) -> Option<Arc<AotHandlerRuntime>> {
    let epoch = HANDLER_CACHE_EPOCH.load(Ordering::Acquire);
    LOCAL_HANDLER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.epoch != epoch {
            cache.last = None;
            cache.runtimes.clear();
            cache.epoch = epoch;
        }
        let last = cache.last.as_ref()?;
        (last.web_root == web_root && last.source == source).then(|| Arc::clone(&last.runtime))
    })
}

fn remember_local_handler_runtime(web_root: &Path, source: &str, runtime: &Arc<AotHandlerRuntime>) {
    let epoch = HANDLER_CACHE_EPOCH.load(Ordering::Acquire);
    LOCAL_HANDLER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.epoch != epoch {
            cache.last = None;
            cache.runtimes.clear();
            cache.epoch = epoch;
        }
        cache.last = Some(LocalHandlerRuntime {
            web_root: web_root.to_path_buf(),
            source: source.to_string(),
            runtime: Arc::clone(runtime),
        });
    });
}

/// Resolves an admitted immutable generation without a process-wide lock.
fn local_cached_runtime(source_path: &Path) -> Option<Arc<AotHandlerRuntime>> {
    let epoch = HANDLER_CACHE_EPOCH.load(Ordering::Acquire);
    LOCAL_HANDLER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.epoch != epoch {
            cache.last = None;
            cache.runtimes.clear();
            cache.epoch = epoch;
        }
        cache.runtimes.get(source_path).and_then(Weak::upgrade)
    })
}

/// Remembers only a weak generation reference on one fixed protocol owner.
fn remember_local_runtime(source_path: PathBuf, runtime: &Arc<AotHandlerRuntime>) {
    let epoch = HANDLER_CACHE_EPOCH.load(Ordering::Acquire);
    LOCAL_HANDLER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.epoch != epoch {
            cache.last = None;
            cache.runtimes.clear();
            cache.epoch = epoch;
        }
        cache.runtimes.insert(source_path, Arc::downgrade(runtime));
    });
}

fn cached_source_entry(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
) -> Result<HandlerCacheEntry, String> {
    source_generation::cached_source_entry(web_root, source_path, expected_module)
}

/// Invalidates admitted source generations after the watcher observes change.
pub(super) fn invalidate_vm_handler_cache() {
    if let Some(cache) = HANDLER_CACHE.get() {
        if let Ok(mut cache) = cache.write() {
            cache.clear();
        }
    }
    HANDLER_CACHE_EPOCH.fetch_add(1, Ordering::AcqRel);
}

fn cache() -> Result<&'static RwLock<HashMap<PathBuf, HandlerCacheEntry>>, String> {
    Ok(HANDLER_CACHE.get_or_init(|| RwLock::new(HashMap::new())))
}
