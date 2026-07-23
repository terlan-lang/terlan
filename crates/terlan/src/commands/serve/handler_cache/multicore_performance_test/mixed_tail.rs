//! Mixed CPU and I/O tail-latency evidence from production runtime paths.

use std::num::NonZeroU64;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::commands::serve::handler::HandlerResponse;
use crate::runtime::native_image::managed::{
    ActorHeap, ActorId, AllocationClass, HeapLimits, ManagedRoot, ManagedTypeDescriptor,
    RootLocation, SemanticTypeId,
};
use crate::runtime::vm::actor::{VmActorReceive, VmActorRuntime};
use crate::runtime::vm::process::VmProcessSource;
use crate::runtime::vm::scheduler_topology::{VmSchedulerId, VmSchedulerTopology};
use crate::runtime::vm::work_stealing::{
    VmSchedulerWorkSnapshot, VmWorkDirective, VmWorkStealingConfig, VmWorkStealingPolicy,
};
use crate::runtime::vm::ReplValue;

use super::super::AotHandlerGeneration;
use super::cpu_actor::{cpu_result, CPU_ITERATIONS_PER_ACTOR};
use super::workloads::request_value;
use super::{timing_distribution, TimingDistribution};

/// Scheduler width used to create simultaneous generated CPU pressure.
pub(super) const MIXED_LOAD_SCHEDULERS: usize = 2;
/// Sequential CPU-bound passes executed by each pressure actor.
pub(super) const MIXED_CPU_PASSES: usize = 5;
/// Stable mixed-load metric order required by the policy document.
pub(super) const MIXED_TAIL_METRICS: [&str; 7] = [
    "scheduler_wait",
    "mailbox_delivery",
    "timer_delay",
    "http_latency",
    "failed_steal_backoff",
    "allocation_pause",
    "collection_pause",
];
/// Operations normalized into each retained metric sample.
pub(super) const MIXED_TAIL_OPERATIONS: [usize; 7] = [1, 256, 256, 1, 256, 256, 16];
/// Stable metric and normalized-operation contract included in report hashes.
pub(super) const MIXED_TAIL_CONTRACT: &str = "\
scheduler_wait:1;\
mailbox_delivery:256;\
timer_delay:256;\
http_latency:1;\
failed_steal_backoff:256;\
allocation_pause:256;\
collection_pause:16";

const CPU_START_TIMEOUT: Duration = Duration::from_secs(2);
const MANAGED_OBJECT_BYTES: usize = 64;
const COLLECTION_OBJECTS: usize = 64;

/// One runtime latency distribution observed while generated actors consume CPUs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct MixedTailMeasurement {
    /// Stable metric identity joined to the versioned policy.
    pub(super) metric: &'static str,
    /// Runtime implementation responsible for the measured operation.
    pub(super) execution_scope: &'static str,
    /// Number of independent latency samples.
    pub(super) samples: usize,
    /// Operations averaged into each retained latency sample.
    pub(super) operations_per_sample: usize,
    /// Measured latency distribution.
    pub(super) timing: TimingDistribution,
}

/// Complete mixed CPU and I/O evidence used by release policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct MixedTailEvidence {
    /// Number of actual fixed scheduler owners placed under CPU pressure.
    pub(super) requested_schedulers: usize,
    /// Generated export used to create CPU pressure.
    pub(super) cpu_export: &'static str,
    /// Integer-mixing operations executed by each pressure actor.
    pub(super) cpu_iterations_per_actor: usize,
    /// Maximum generated scheduler owners observed executing simultaneously.
    pub(super) maximum_simultaneously_active_schedulers: usize,
    /// Whether every retained sample began under two-owner CPU pressure.
    pub(super) cpu_overlap_proven: bool,
    /// Independent samples retained for every metric.
    pub(super) samples_per_metric: usize,
    /// Metric distributions in canonical policy order.
    pub(super) measurements: Vec<MixedTailMeasurement>,
}

impl MixedTailEvidence {
    /// Returns one uniquely named mixed-load measurement.
    pub(super) fn metric(&self, name: &str) -> Result<&MixedTailMeasurement, String> {
        self.measurements
            .iter()
            .find(|measurement| measurement.metric == name)
            .ok_or_else(|| format!("mixed-load benchmark omitted metric `{name}`"))
    }
}

/// Measures all required latency paths under simultaneous generated CPU load.
pub(super) fn measure_mixed_tail(
    image: &Path,
    package_root: &Path,
    samples: usize,
) -> Result<MixedTailEvidence, String> {
    if samples == 0 {
        return Err("mixed-load benchmark requires at least one sample".to_string());
    }
    let generation = AotHandlerGeneration::load_with_shard_count(
        image,
        crate::runtime::vm::http_session::VmHttpSessionService::new(
            crate::runtime::vm::http_session::VmHttpSessionRuntime::new(
                "terlc-multicore-mixed-tail",
                86_400,
            )?,
        ),
        MIXED_LOAD_SCHEDULERS,
    )?;
    let measurements = vec![
        measure_scheduler_wait(&generation, samples)?,
        measure_mailbox_delivery(&generation, samples)?,
        measure_timer_delay(&generation, samples)?,
        measure_http_latency(&generation, package_root, samples)?,
        measure_failed_steal(&generation, samples)?,
        measure_allocation_pause(&generation, samples)?,
        measure_collection_pause(&generation, samples)?,
    ];
    Ok(MixedTailEvidence {
        requested_schedulers: MIXED_LOAD_SCHEDULERS,
        cpu_export: "app.MulticoreBenchmark.mixed_cpu_load",
        cpu_iterations_per_actor: CPU_ITERATIONS_PER_ACTOR * MIXED_CPU_PASSES,
        maximum_simultaneously_active_schedulers: MIXED_LOAD_SCHEDULERS,
        cpu_overlap_proven: true,
        samples_per_metric: samples,
        measurements,
    })
}

/// Measures generated scheduler command queue delay behind CPU-bound execution.
fn measure_scheduler_wait(
    generation: &AotHandlerGeneration,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "scheduler_wait",
        "generated_aot_fixed_scheduler_queue",
        samples,
        MIXED_TAIL_OPERATIONS[0],
        |generation| {
            let scheduler = VmSchedulerId::primary();
            let route = generation.route_new_actor_on(scheduler)?;
            let owner = generation.shard(0)?;
            let started = Instant::now();
            let result = owner.probe_execution(
                route,
                "app.MulticoreBenchmark.ready".to_string(),
                Arc::new(Barrier::new(1)),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(AtomicUsize::new(0)),
            );
            let elapsed = started.elapsed().as_nanos();
            generation.release_actor_route(0);
            let (value, _) = result?;
            if value != ReplValue::Bool(true) {
                return Err("mixed scheduler-wait probe returned the wrong value".to_string());
            }
            Ok(elapsed)
        },
    )
}

/// Measures ordered actor mailbox publication and receive under CPU pressure.
fn measure_mailbox_delivery(
    generation: &AotHandlerGeneration,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "mailbox_delivery",
        "vm_actor_mailbox_under_generated_aot_cpu_load",
        samples,
        MIXED_TAIL_OPERATIONS[1],
        |_| {
            let mut runtime = VmActorRuntime::with_scheduler_owner(
                NonZeroU64::new(1).expect("constant owner is nonzero"),
            )?;
            let sender =
                runtime.spawn_root(VmProcessSource::new("app.MulticoreBenchmark", "sender", 0));
            let recipient = runtime.spawn_root(VmProcessSource::new(
                "app.MulticoreBenchmark",
                "recipient",
                0,
            ));
            let started = Instant::now();
            for operation in 0..MIXED_TAIL_OPERATIONS[1] {
                let payload = ReplValue::Int(operation as i64);
                runtime.send(sender, recipient, payload.clone())?;
                let received = runtime.receive_next_or_block(recipient)?;
                let VmActorReceive::Message(message) = received else {
                    return Err("mixed mailbox delivery parked unexpectedly".to_string());
                };
                if message.payload != payload {
                    return Err("mixed mailbox delivery changed its payload".to_string());
                }
            }
            let elapsed = started.elapsed().as_nanos();
            Ok(elapsed)
        },
    )
}

/// Measures logical timer publication and deadline delivery under CPU pressure.
fn measure_timer_delay(
    generation: &AotHandlerGeneration,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "timer_delay",
        "vm_actor_timer_under_generated_aot_cpu_load",
        samples,
        MIXED_TAIL_OPERATIONS[2],
        |_| {
            let mut runtime = VmActorRuntime::with_scheduler_owner(
                NonZeroU64::new(1).expect("constant owner is nonzero"),
            )?;
            let sender = runtime.spawn_root(VmProcessSource::new(
                "app.MulticoreBenchmark",
                "timer_sender",
                0,
            ));
            let recipient = runtime.spawn_root(VmProcessSource::new(
                "app.MulticoreBenchmark",
                "timer_recipient",
                0,
            ));
            let started = Instant::now();
            for operation in 0..MIXED_TAIL_OPERATIONS[2] {
                let now = operation as u64 * 2;
                let payload = ReplValue::Int(operation as i64);
                runtime.send_after(sender, recipient, payload.clone(), now, 1)?;
                runtime.advance_actor_timers(now + 1);
                let received = runtime.receive_next_or_block(recipient)?;
                let VmActorReceive::Message(message) = received else {
                    return Err("mixed timer delivery parked unexpectedly".to_string());
                };
                if message.payload != payload {
                    return Err("mixed timer delivery changed its payload".to_string());
                }
            }
            let elapsed = started.elapsed().as_nanos();
            Ok(elapsed)
        },
    )
}

/// Measures generated HTTP execution and response conversion behind CPU load.
fn measure_http_latency(
    generation: &AotHandlerGeneration,
    package_root: &Path,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "http_latency",
        "generated_aot_http_under_fixed_scheduler_cpu_load",
        samples,
        MIXED_TAIL_OPERATIONS[3],
        |generation| {
            let scheduler = VmSchedulerId::primary();
            let route = generation.route_new_actor_on(scheduler)?;
            let owner = generation.shard(0)?;
            let started = Instant::now();
            let result = owner.probe_execution_with_args(
                route,
                "app.MulticoreBenchmark.http".to_string(),
                vec![request_value()],
                Arc::new(Barrier::new(1)),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(AtomicUsize::new(0)),
            );
            let response = result.and_then(|(value, _)| {
                HandlerResponse::from_vm_response_with_package_root(&value, package_root)
            });
            let elapsed = started.elapsed().as_nanos();
            generation.release_actor_route(0);
            let response = response?;
            if response.status != 200 || response.body.as_bytes() != b"multicore" {
                return Err("mixed HTTP probe returned an invalid response".to_string());
            }
            Ok(elapsed)
        },
    )
}

/// Measures production failed-steal backoff accounting under CPU pressure.
fn measure_failed_steal(
    generation: &AotHandlerGeneration,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "failed_steal_backoff",
        "vm_work_stealing_policy_under_generated_aot_cpu_load",
        samples,
        MIXED_TAIL_OPERATIONS[4],
        |_| {
            let topology = VmSchedulerTopology::new(MIXED_LOAD_SCHEDULERS)?;
            let schedulers = topology.schedulers().collect::<Vec<_>>();
            let snapshots = schedulers
                .iter()
                .copied()
                .map(|scheduler| VmSchedulerWorkSnapshot::new(scheduler, [0; 3], [0; 3]))
                .collect::<Vec<_>>();
            let mut policies = (0..MIXED_TAIL_OPERATIONS[4])
                .map(|_| {
                    VmWorkStealingPolicy::new(
                        MIXED_LOAD_SCHEDULERS,
                        VmWorkStealingConfig::default(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let started = Instant::now();
            for policy in &mut policies {
                policy.record_steal_result(schedulers[0], 0)?;
                let directive = policy.decide(schedulers[0], &snapshots)?;
                if !matches!(directive, VmWorkDirective::Backoff(_)) {
                    return Err("failed steal did not enter bounded backoff".to_string());
                }
            }
            let elapsed = started.elapsed().as_nanos();
            Ok(elapsed)
        },
    )
}

/// Measures one actor-local bump allocation under CPU pressure.
fn measure_allocation_pause(
    generation: &AotHandlerGeneration,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "allocation_pause",
        "actor_local_bump_allocator_under_generated_aot_cpu_load",
        samples,
        MIXED_TAIL_OPERATIONS[5],
        |_| {
            let owner = ActorId::new(1).map_err(|error| error.to_string())?;
            let mut heap =
                ActorHeap::new(owner, heap_limits()).map_err(|error| error.to_string())?;
            let descriptor = managed_descriptor()?;
            let started = Instant::now();
            let mut last = None;
            for operation in 0..MIXED_TAIL_OPERATIONS[5] {
                last = Some(
                    heap.allocate::<[u8; MANAGED_OBJECT_BYTES]>(
                        Arc::clone(&descriptor),
                        &[operation as u8; MANAGED_OBJECT_BYTES],
                        &[],
                    )
                    .map_err(|error| error.to_string())?,
                );
            }
            let elapsed = started.elapsed().as_nanos();
            let last = last.ok_or_else(|| "mixed allocation batch was empty".to_string())?;
            if heap.read(last).map_err(|error| error.to_string())? != [255; MANAGED_OBJECT_BYTES] {
                return Err("mixed allocation changed the managed payload".to_string());
            }
            Ok(elapsed)
        },
    )
}

/// Measures bounded precise actor-local collection under CPU pressure.
fn measure_collection_pause(
    generation: &AotHandlerGeneration,
    samples: usize,
) -> Result<MixedTailMeasurement, String> {
    measure_samples(
        generation,
        "collection_pause",
        "actor_local_precise_collection_under_generated_aot_cpu_load",
        samples,
        MIXED_TAIL_OPERATIONS[6],
        |_| {
            let mut fixtures = (0..MIXED_TAIL_OPERATIONS[6])
                .map(|actor| collection_fixture(actor as u64 + 1))
                .collect::<Result<Vec<_>, _>>()?;
            let started = Instant::now();
            for (heap, roots) in &mut fixtures {
                let stats = heap
                    .collect(roots, 1_048_576)
                    .map_err(|error| error.to_string())?;
                if stats.objects_after != COLLECTION_OBJECTS
                    || heap.collection_count() != 1
                    || roots.len() != COLLECTION_OBJECTS
                {
                    return Err("mixed collection lost a live managed root".to_string());
                }
            }
            let elapsed = started.elapsed().as_nanos();
            Ok(elapsed)
        },
    )
}

/// Collects independent samples while two generated CPU actors are active.
fn measure_samples(
    generation: &AotHandlerGeneration,
    metric: &'static str,
    execution_scope: &'static str,
    samples: usize,
    operations_per_sample: usize,
    mut operation: impl FnMut(&AotHandlerGeneration) -> Result<u128, String>,
) -> Result<MixedTailMeasurement, String> {
    let mut durations = Vec::with_capacity(samples);
    for sample in 0..samples {
        let total = under_cpu_load(generation, sample, |generation| operation(generation))?;
        durations.push(
            total
                .checked_div(operations_per_sample as u128)
                .ok_or_else(|| "mixed-load operation count is zero".to_string())?,
        );
    }
    Ok(MixedTailMeasurement {
        metric,
        execution_scope,
        samples,
        operations_per_sample,
        timing: timing_distribution(&durations)?,
    })
}

/// Runs one foreground operation after proving both CPU actors entered execution.
fn under_cpu_load<T>(
    generation: &AotHandlerGeneration,
    sample: usize,
    operation: impl FnOnce(&AotHandlerGeneration) -> Result<T, String>,
) -> Result<T, String> {
    let topology = VmSchedulerTopology::new(MIXED_LOAD_SCHEDULERS)?;
    let routes = topology
        .schedulers()
        .map(|scheduler| generation.route_new_actor_on(scheduler))
        .collect::<Result<Vec<_>, _>>()?;
    let barrier = Arc::new(Barrier::new(MIXED_LOAD_SCHEDULERS));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let result = std::thread::scope(|scope| {
        let joins = routes
            .iter()
            .copied()
            .enumerate()
            .map(|(lane, route)| {
                let owner = generation.shard(route.scheduler().index())?;
                let barrier = Arc::clone(&barrier);
                let active = Arc::clone(&active);
                let maximum = Arc::clone(&maximum);
                let seed = 101 + sample as i64 * 17 + lane as i64;
                Ok(scope.spawn(move || {
                    owner
                        .probe_execution_with_args(
                            route,
                            "app.MulticoreBenchmark.mixed_cpu_load".to_string(),
                            vec![ReplValue::Int(seed)],
                            barrier,
                            active,
                            maximum,
                        )
                        .map(|(value, _)| (seed, value))
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        wait_for_cpu_pressure(&active)?;
        let operation_result = operation(generation);
        let cpu_results = joins
            .into_iter()
            .map(|join| {
                join.join()
                    .map_err(|_| "mixed-load CPU client panicked".to_string())?
            })
            .collect::<Result<Vec<_>, String>>();
        operation_result.map(|value| (value, cpu_results))
    });
    for route in &routes {
        generation.release_actor_route(route.scheduler().index());
    }
    let (value, cpu_results) = result?;
    for (seed, actual) in cpu_results? {
        let expected = (0..MIXED_CPU_PASSES).fold(seed, |value, _| cpu_result(value));
        if actual != ReplValue::Int(expected) {
            return Err(format!(
                "mixed-load CPU actor returned {actual:?}, expected {expected}"
            ));
        }
    }
    if maximum.load(Ordering::SeqCst) < MIXED_LOAD_SCHEDULERS {
        return Err("mixed-load benchmark did not overlap CPU actors".to_string());
    }
    Ok(value)
}

/// Waits for both fixed owners to publish active CPU execution.
fn wait_for_cpu_pressure(active: &AtomicUsize) -> Result<(), String> {
    let deadline = Instant::now() + CPU_START_TIMEOUT;
    while active.load(Ordering::SeqCst) < MIXED_LOAD_SCHEDULERS {
        if Instant::now() >= deadline {
            return Err("mixed-load CPU actors did not start before timeout".to_string());
        }
        std::thread::yield_now();
    }
    Ok(())
}

/// Returns bounded limits large enough for the fixed collection fixture.
fn heap_limits() -> HeapLimits {
    HeapLimits::new(8 * 1024, 64 * 1024).expect("fixed mixed-load heap limits are valid")
}

/// Builds the canonical fixed-size managed object descriptor.
fn managed_descriptor() -> Result<Arc<ManagedTypeDescriptor>, String> {
    let semantic = SemanticTypeId::from_canonical("app.MulticoreBenchmark.MixedObject")
        .map_err(|error| error.to_string())?;
    ManagedTypeDescriptor::new(
        semantic,
        MANAGED_OBJECT_BYTES,
        8,
        Vec::new(),
        AllocationClass::Young,
    )
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

/// Builds one fully rooted actor heap before timed collection begins.
fn collection_fixture(actor: u64) -> Result<(ActorHeap, Vec<ManagedRoot>), String> {
    let owner = ActorId::new(actor).map_err(|error| error.to_string())?;
    let mut heap = ActorHeap::new(owner, heap_limits()).map_err(|error| error.to_string())?;
    let descriptor = managed_descriptor()?;
    let roots = (0..COLLECTION_OBJECTS)
        .map(|slot| {
            heap.allocate::<[u8; MANAGED_OBJECT_BYTES]>(
                Arc::clone(&descriptor),
                &[slot as u8; MANAGED_OBJECT_BYTES],
                &[],
            )
            .map(|reference| {
                ManagedRoot::new(
                    owner,
                    RootLocation::ActorState { slot: slot as u16 },
                    reference.erase(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok((heap, roots))
}
