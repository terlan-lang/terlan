//! Reproducible direct-AOT actor runtime workload benchmarks.

use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::managed_heap::{
    ActorHeap, ActorId, AllocationClass, HeapLimits, ManagedRoot, ManagedTypeDescriptor,
    RootLocation, SemanticTypeId,
};
use super::vm_runtime::actor::{VmActorReceive, VmActorRuntime};
use super::vm_runtime::process::{VmExitReason, VmProcessSource};
use super::vm_runtime::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::ReplValue;

use super::{rustc_version, unix_timestamp_seconds, write_report, BenchmarkStatus, Measurement};

/// Stable benchmark command name.
pub(crate) const COMMAND: &str = "vm-aot-runtime-workloads";
const DEFAULT_OUTPUT: &str = "target/quality/vm-aot-runtime-workloads.json";
const REFERENCE_MANIFEST: &str =
    include_str!("../../../../benchmarks/baselines/vm-aot-runtime-workloads.json");
const EXPECTED_WORKLOADS: &[&str] = &[
    "actor_heap_allocation",
    "local_message_round_trip",
    "scheduler_yield_cycle",
    "actor_local_collection_pause",
    "actor_spawn_exit_churn",
    "mixed_actor_runtime_tail",
];

/// Checked reference workload manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RuntimeWorkloadManifest {
    schema: String,
    samples: usize,
    workloads: Vec<RuntimeWorkloadSpec>,
}

/// One fixed workload definition used by every benchmark run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RuntimeWorkloadSpec {
    name: String,
    operations_per_sample: usize,
    scope: String,
}

/// Timing and throughput for one checked workload.
#[derive(Debug, Serialize)]
struct RuntimeWorkloadMeasurement {
    name: &'static str,
    operations_per_sample: usize,
    operations_per_second: u128,
    timing: Measurement,
}

/// Machine-readable direct-AOT runtime benchmark report.
#[derive(Debug, Serialize)]
struct RuntimeWorkloadReport {
    schema: &'static str,
    benchmark: &'static str,
    status: BenchmarkStatus,
    timestamp_unix_seconds: u64,
    terlan_version: &'static str,
    rustc_version: Option<String>,
    reference_schema: String,
    samples: usize,
    workloads: Vec<RuntimeWorkloadSpec>,
    measurements: Vec<RuntimeWorkloadMeasurement>,
    error_reason: Option<String>,
}

impl RuntimeWorkloadReport {
    /// Creates a completed report tied to one validated reference manifest.
    fn completed(
        manifest: RuntimeWorkloadManifest,
        measurements: Vec<RuntimeWorkloadMeasurement>,
    ) -> Self {
        Self {
            schema: "terlan.vm-aot-runtime-benchmark.v1",
            benchmark: COMMAND,
            status: BenchmarkStatus::Completed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            reference_schema: manifest.schema,
            samples: manifest.samples,
            workloads: manifest.workloads,
            measurements,
            error_reason: None,
        }
    }
}

/// Runs the checked direct-AOT runtime benchmark command.
pub(crate) fn run_cli() -> ExitCode {
    let output = env::var_os("TERLAN_BENCH_AOT_RUNTIME_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT));
    let report = match run_benchmark() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("[{COMMAND}] failed: {error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = write_report(&output, &report) {
        eprintln!("[{COMMAND}] failed: {error}");
        return ExitCode::from(1);
    }
    println!("[{COMMAND}] completed; wrote {}", output.display());
    ExitCode::SUCCESS
}

/// Loads, validates, and executes every recorded runtime workload.
fn run_benchmark() -> Result<RuntimeWorkloadReport, String> {
    let manifest = parse_reference_manifest(REFERENCE_MANIFEST)?;
    let mut measurements = Vec::with_capacity(manifest.workloads.len());
    for workload in &manifest.workloads {
        measurements.push(run_workload(workload, manifest.samples)?);
    }
    Ok(RuntimeWorkloadReport::completed(manifest, measurements))
}

/// Parses a reference manifest and enforces the complete workload family.
fn parse_reference_manifest(source: &str) -> Result<RuntimeWorkloadManifest, String> {
    let manifest: RuntimeWorkloadManifest = serde_json::from_str(source)
        .map_err(|error| format!("invalid AOT runtime workload manifest: {error}"))?;
    if manifest.schema != "terlan.vm-aot-runtime-workloads.v1" {
        return Err(format!(
            "unsupported AOT runtime workload schema `{}`",
            manifest.schema
        ));
    }
    if manifest.samples < 32 {
        return Err("AOT runtime workloads require at least 32 tail samples".to_string());
    }
    let names = manifest
        .workloads
        .iter()
        .map(|workload| workload.name.as_str())
        .collect::<Vec<_>>();
    if names != EXPECTED_WORKLOADS {
        return Err(format!(
            "AOT runtime workload order was {names:?}, expected {EXPECTED_WORKLOADS:?}"
        ));
    }
    let unique = names.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != names.len() {
        return Err("AOT runtime workload names must be unique".to_string());
    }
    for workload in &manifest.workloads {
        if workload.operations_per_sample == 0 || workload.scope.trim().is_empty() {
            return Err(format!(
                "AOT runtime workload `{}` requires operations and scope",
                workload.name
            ));
        }
    }
    Ok(manifest)
}

/// Dispatches one validated workload to its canonical runtime implementation.
fn run_workload(
    workload: &RuntimeWorkloadSpec,
    samples: usize,
) -> Result<RuntimeWorkloadMeasurement, String> {
    let (name, timing) = match workload.name.as_str() {
        "actor_heap_allocation" => (
            "actor_heap_allocation",
            measure_samples("actor_heap_allocation", samples, || {
                actor_heap_allocation(workload.operations_per_sample)
            })?,
        ),
        "local_message_round_trip" => (
            "local_message_round_trip",
            measure_samples("local_message_round_trip", samples, || {
                local_message_round_trip(workload.operations_per_sample)
            })?,
        ),
        "scheduler_yield_cycle" => (
            "scheduler_yield_cycle",
            measure_samples("scheduler_yield_cycle", samples, || {
                scheduler_yield_cycle(workload.operations_per_sample)
            })?,
        ),
        "actor_local_collection_pause" => (
            "actor_local_collection_pause",
            measure_collection_pauses(samples, workload.operations_per_sample)?,
        ),
        "actor_spawn_exit_churn" => (
            "actor_spawn_exit_churn",
            measure_samples("actor_spawn_exit_churn", samples, || {
                actor_spawn_exit_churn(workload.operations_per_sample)
            })?,
        ),
        "mixed_actor_runtime_tail" => (
            "mixed_actor_runtime_tail",
            measure_samples("mixed_actor_runtime_tail", samples, || {
                mixed_actor_runtime_tail(workload.operations_per_sample)
            })?,
        ),
        unknown => return Err(format!("unsupported AOT runtime workload `{unknown}`")),
    };
    let operations_per_second = (workload.operations_per_sample as u128)
        .saturating_mul(1_000_000_000)
        .checked_div(timing.mean_ns.max(1))
        .unwrap_or(0);
    Ok(RuntimeWorkloadMeasurement {
        name,
        operations_per_sample: workload.operations_per_sample,
        operations_per_second,
        timing,
    })
}

/// Measures correctness-checked samples with the shared timing summary.
fn measure_samples(
    name: &'static str,
    samples: usize,
    mut sample: impl FnMut() -> Result<(), String>,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        sample()?;
        durations.push(started.elapsed());
    }
    Ok(Measurement::from_durations(name, &durations))
}

/// Allocates one batch of fixed-size objects on a fresh actor-owned heap.
fn actor_heap_allocation(operations: usize) -> Result<(), String> {
    let mut heap = benchmark_heap(1, operations)?;
    let descriptor = benchmark_descriptor()?;
    for index in 0..operations {
        let mut payload = [0_u8; 32];
        payload[..8].copy_from_slice(&(index as u64).to_le_bytes());
        heap.allocate::<()>(Arc::clone(&descriptor), &payload, &[])
            .map_err(|error| error.to_string())?;
    }
    if heap.object_count() != operations {
        return Err(format!(
            "allocation workload retained {} objects, expected {operations}",
            heap.object_count()
        ));
    }
    Ok(())
}

/// Sends and receives one ordered batch through the shard-local actor mailbox.
fn local_message_round_trip(operations: usize) -> Result<(), String> {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(VmProcessSource::new("bench.Message", "sender", 0));
    let receiver = runtime.spawn_root(VmProcessSource::new("bench.Message", "receiver", 0));
    for sequence in 0..operations {
        runtime.send(sender, receiver, ReplValue::Int(sequence as i64))?;
    }
    for expected in 0..operations {
        match runtime.receive_next_or_block(receiver)? {
            VmActorReceive::Message(message)
                if message.sender == sender
                    && message.payload == ReplValue::Int(expected as i64) => {}
            other => return Err(format!("message workload received {other:?}")),
        }
    }
    Ok(())
}

/// Runs one actor through repeated cooperative scheduler yield cycles.
fn scheduler_yield_cycle(operations: usize) -> Result<(), String> {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(VmProcessSource::new("bench.Schedule", "yielding", 0));
    for _ in 0..operations {
        let run =
            runtime.run_next(|_process, _slice| VmSchedulerDecision::Yield { reductions: 1 })?;
        if run.pid != Some(actor) || run.outcome != VmSchedulerOutcome::Ran {
            return Err(format!("scheduler workload returned {run:?}"));
        }
    }
    Ok(())
}

/// Times only moving collection after preparing each actor heap and root set.
fn measure_collection_pauses(samples: usize, objects: usize) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(samples);
    for sample in 0..samples {
        let (mut heap, mut roots) = prepared_collection_heap((sample + 1) as u64, objects)?;
        let started = Instant::now();
        let collected = heap
            .collect(&mut roots, usize::MAX)
            .map_err(|error| error.to_string())?;
        durations.push(started.elapsed());
        if collected.objects_before != objects || collected.objects_after != roots.len() {
            return Err(format!("collection workload returned {collected:?}"));
        }
    }
    Ok(Measurement::from_durations(
        "actor_local_collection_pause",
        &durations,
    ))
}

/// Spawns and exits actors through the unified runtime lifecycle.
fn actor_spawn_exit_churn(operations: usize) -> Result<(), String> {
    let mut runtime = VmActorRuntime::default();
    for _ in 0..operations {
        let actor = runtime.spawn_root(VmProcessSource::new("bench.Churn", "actor", 0));
        runtime.exit_actor(actor, VmExitReason::Normal)?;
    }
    if !runtime.live_process_ids().is_empty() {
        return Err("actor churn left live processes".to_string());
    }
    Ok(())
}

/// Runs allocation, collection, messaging, scheduling, and teardown together.
fn mixed_actor_runtime_tail(operations: usize) -> Result<(), String> {
    let allocation_count = operations.max(8);
    let (mut heap, mut roots) = prepared_collection_heap(1, allocation_count)?;
    heap.collect(&mut roots, usize::MAX)
        .map_err(|error| error.to_string())?;
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(VmProcessSource::new("bench.Mixed", "sender", 0));
    let receiver = runtime.spawn_root(VmProcessSource::new("bench.Mixed", "receiver", 0));
    for sequence in 0..operations {
        runtime.send(sender, receiver, ReplValue::Int(sequence as i64))?;
        match runtime.receive_next_or_block(receiver)? {
            VmActorReceive::Message(message)
                if message.payload == ReplValue::Int(sequence as i64) => {}
            other => return Err(format!("mixed workload received {other:?}")),
        }
        let run =
            runtime.run_next(|_process, _slice| VmSchedulerDecision::Yield { reductions: 1 })?;
        if run.outcome != VmSchedulerOutcome::Ran {
            return Err(format!("mixed workload scheduler returned {run:?}"));
        }
    }
    runtime.exit_actor(sender, VmExitReason::Normal)?;
    runtime.exit_actor(receiver, VmExitReason::Normal)?;
    Ok(())
}

/// Prepares one fixed actor heap and eight precise roots for collection.
fn prepared_collection_heap(
    owner_id: u64,
    objects: usize,
) -> Result<(ActorHeap, Vec<ManagedRoot>), String> {
    let owner = ActorId::new(owner_id).map_err(|error| error.to_string())?;
    let mut heap = benchmark_heap(owner.get(), objects)?;
    let descriptor = benchmark_descriptor()?;
    let root_stride = (objects / 8).max(1);
    let mut roots = Vec::new();
    for index in 0..objects {
        let reference = heap
            .allocate::<()>(Arc::clone(&descriptor), &[0_u8; 32], &[])
            .map_err(|error| error.to_string())?;
        if index % root_stride == 0 && roots.len() < 8 {
            roots.push(ManagedRoot::new(
                owner,
                RootLocation::ActorState {
                    slot: roots.len() as u16,
                },
                reference,
            ));
        }
    }
    Ok((heap, roots))
}

/// Creates one bounded actor heap sized for the requested benchmark batch.
fn benchmark_heap(owner: u64, objects: usize) -> Result<ActorHeap, String> {
    let hard_bytes = objects
        .checked_mul(64)
        .and_then(|bytes| bytes.checked_add(4_096))
        .ok_or_else(|| "benchmark heap size overflow".to_string())?;
    let limits = HeapLimits::new(hard_bytes / 2, hard_bytes).map_err(|error| error.to_string())?;
    ActorHeap::new(
        ActorId::new(owner).map_err(|error| error.to_string())?,
        limits,
    )
    .map_err(|error| error.to_string())
}

/// Builds the fixed no-reference object layout used by allocation workloads.
fn benchmark_descriptor() -> Result<Arc<ManagedTypeDescriptor>, String> {
    let semantic = SemanticTypeId::from_canonical("benchmark.RuntimeObject")
        .map_err(|error| error.to_string())?;
    ManagedTypeDescriptor::new(semantic, 32, 8, Vec::new(), AllocationClass::Young)
        .map(Arc::new)
        .map_err(|error| error.to_string())
}
