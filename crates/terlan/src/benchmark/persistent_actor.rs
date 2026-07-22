use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use serde::Serialize;

use crate::memory::{
    logical_value_bytes, VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome,
};
use crate::process::{VmExitReason, VmProcessSource, VmProcessTable};
use crate::scheduler::{VmScheduler, VmSchedulerDecision, VmSchedulerOutcome};
pub(crate) use crate::ReplValue;

#[path = "../runtime/vm/checksum.rs"]
mod checksum;
#[path = "../runtime/vm/distributed_state.rs"]
pub(crate) mod distributed_state;
#[path = "../runtime/vm/distributed_storage.rs"]
pub(crate) mod distributed_storage;
#[path = "../runtime/vm/model_sync.rs"]
mod model_sync;
#[path = "../runtime/vm/persistent_actor_adapter.rs"]
mod persistent_actor_adapter;
#[path = "../runtime/vm/persistent_actor_compaction.rs"]
mod persistent_actor_compaction;
#[path = "../runtime/vm/persistent_actor_restore.rs"]
mod persistent_actor_restore;
#[path = "../runtime/vm/persistent_actor_schema.rs"]
mod persistent_actor_schema;
#[path = "../runtime/vm/persistent_actor_store.rs"]
mod persistent_actor_store;

use super::write_report;
use persistent_actor_adapter::execute_persistent_actor_adapter_cross_adapter_restore;
use persistent_actor_compaction::{
    plan_persistent_actor_compaction, VmPersistentActorCompactionCandidate,
    VmPersistentActorReplayEquivalence, VmPersistentActorRetentionPolicy,
};
use persistent_actor_schema::{
    VmPersistentActorField, VmPersistentActorMigrationEdge, VmPersistentActorMigrationGraph,
    VmPersistentActorSchemaDescriptor, VmPersistentActorSchemaKey,
};
use persistent_actor_store::{
    VmFileBackedPersistentActorStore, VmInMemoryPersistentActorStore, VmPersistentActorEvent,
    VmPersistentActorId, VmPersistentActorSchema, VmPersistentActorSnapshot,
    VmPersistentActorStoreAdapter, VmPersistentActorStoreOutcome,
};

pub(crate) const COMMAND: &str = "vm-persistent-actor-runtime-baseline";
const DEFAULT_OUTPUT: &str = "target/quality/vm-persistent-actor-benchmark.json";
const DEFAULT_RUNS: usize = 3;
const DEFAULT_SAMPLES: usize = 100;
const DEFAULT_EVENTS: usize = 64;
const WARMUP_SAMPLES: usize = 10;
const FILE_SAMPLES: usize = 20;
const FILE_EVENTS: usize = 16;
const FILE_WARMUP_SAMPLES: usize = 2;
const COMPACTION_EVENTS: usize = 1_000;
const COMPACTED_THROUGH: usize = 800;
const MIGRATION_SCHEMA_VERSIONS: usize = 64;
const MEMORY_SOFT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MEMORY_HARD_LIMIT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct PersistentActorBenchmarkReport {
    schema: &'static str,
    benchmark: &'static str,
    adapter: &'static str,
    run_count: usize,
    samples_per_run: usize,
    events_per_sample: usize,
    correctness_verified: bool,
    runs: Vec<PersistentActorRun>,
    aggregate: PersistentActorRun,
    file_backed: FileBackedBenchmark,
    compaction: CompactionBenchmark,
    schema_migration: SchemaMigrationBenchmark,
    cross_adapter_restore: CrossAdapterRestoreBenchmark,
    scheduler_attribution: SchedulerAttributionBenchmark,
    memory_high_water: MemoryHighWaterBenchmark,
}

#[derive(Clone, Debug, Serialize)]
struct PersistentActorRun {
    sample_count: usize,
    operation_count: usize,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    throughput_events_per_second: u128,
}

#[derive(Clone, Debug, Serialize)]
struct FileBackedBenchmark {
    run_count: usize,
    samples_per_run: usize,
    events_per_sample: usize,
    correctness_verified: bool,
    runs: Vec<FileBackedRun>,
    aggregate: FileBackedRun,
}

#[derive(Clone, Debug, Serialize)]
struct FileBackedRun {
    sample_count: usize,
    snapshot_commit: PhaseLatency,
    append_events: PhaseLatency,
    reopen_load: PhaseLatency,
    vm_replay: PhaseLatency,
    reopen_replay: PhaseLatency,
    disk_bytes_p50: u64,
    disk_bytes_p99: u64,
}

#[derive(Clone, Debug, Serialize)]
struct PhaseLatency {
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
}

#[derive(Clone, Debug, Serialize)]
struct CompactionBenchmark {
    run_count: usize,
    samples_per_run: usize,
    events_before: usize,
    events_retained: usize,
    correctness_verified: bool,
    runs: Vec<PhaseLatency>,
    aggregate: PhaseLatency,
}

#[derive(Clone, Debug, Serialize)]
struct SchemaMigrationBenchmark {
    run_count: usize,
    samples_per_run: usize,
    schema_versions: usize,
    planned_edges: usize,
    correctness_verified: bool,
    runs: Vec<PhaseLatency>,
    aggregate: PhaseLatency,
}

#[derive(Clone, Debug, Serialize)]
struct CrossAdapterRestoreBenchmark {
    run_count: usize,
    samples_per_run: usize,
    source_adapter: &'static str,
    destination_adapter: &'static str,
    events_per_restore: usize,
    correctness_verified: bool,
    runs: Vec<PhaseLatency>,
    aggregate: PhaseLatency,
}

#[derive(Clone, Debug, Serialize)]
struct SchedulerAttributionBenchmark {
    run_count: usize,
    samples_per_run: usize,
    events_per_sample: usize,
    reductions_per_sample: u64,
    correctness_verified: bool,
    runs: Vec<SchedulerAttributionRun>,
    aggregate: SchedulerAttributionRun,
}

#[derive(Clone, Debug, Serialize)]
struct SchedulerAttributionRun {
    sample_count: usize,
    scheduler_ticks: u64,
    reductions_charged: u64,
    scheduler_overhead: PhaseLatency,
}

#[derive(Clone, Debug, Serialize)]
struct MemoryHighWaterBenchmark {
    run_count: usize,
    samples_per_run: usize,
    events_per_sample: usize,
    soft_limit_bytes: usize,
    hard_limit_bytes: usize,
    correctness_verified: bool,
    budget_pass: bool,
    runs: Vec<SizeDistribution>,
    aggregate: SizeDistribution,
}

#[derive(Clone, Debug, Serialize)]
struct SizeDistribution {
    sample_count: usize,
    p50_bytes: u64,
    p95_bytes: u64,
    p99_bytes: u64,
}

struct FileBackedSample {
    snapshot_ns: u128,
    append_ns: u128,
    reopen_ns: u128,
    replay_ns: u128,
    reopen_replay_ns: u128,
    disk_bytes: u64,
}

pub(crate) fn run_cli() -> ExitCode {
    match run_from_env() {
        Ok(path) => {
            println!(
                "[vm-persistent-actor-runtime-baseline] completed; wrote {}",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[vm-persistent-actor-runtime-baseline] failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_from_env() -> Result<PathBuf, String> {
    let output = env::var_os("TERLAN_BENCH_PERSISTENT_ACTOR_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT));
    let runs = env_usize("TERLAN_BENCH_PERSISTENT_ACTOR_RUNS", DEFAULT_RUNS)?;
    let samples = env_usize("TERLAN_BENCH_PERSISTENT_ACTOR_SAMPLES", DEFAULT_SAMPLES)?;
    let events = env_usize("TERLAN_BENCH_PERSISTENT_ACTOR_EVENTS", DEFAULT_EVENTS)?;
    for warmup in 0..WARMUP_SAMPLES {
        execute_sample(warmup, events)?;
    }
    let mut measured_runs = Vec::with_capacity(runs);
    let mut aggregate_durations = Vec::with_capacity(runs * samples);
    for run_index in 0..runs {
        let mut durations = Vec::with_capacity(samples);
        for sample_index in 0..samples {
            let started = Instant::now();
            execute_sample(run_index * samples + sample_index, events)?;
            durations.push(started.elapsed().as_nanos().max(1));
        }
        aggregate_durations.extend_from_slice(&durations);
        measured_runs.push(summarize(&durations, events));
    }
    let file_backed = measure_file_backed(runs)?;
    let compaction = measure_compaction(runs, samples)?;
    let schema_migration = measure_schema_migration(runs, samples)?;
    let cross_adapter_restore = measure_cross_adapter_restore(runs, samples)?;
    let scheduler_attribution = measure_scheduler_attribution(runs, samples, events)?;
    let memory_high_water = measure_memory_high_water(runs, samples, events)?;
    let report = PersistentActorBenchmarkReport {
        schema: "terlan.vm-persistent-actor-benchmark.v1",
        benchmark: "snapshot-append-replay",
        adapter: "vm-in-memory",
        run_count: runs,
        samples_per_run: samples,
        events_per_sample: events,
        correctness_verified: true,
        runs: measured_runs,
        aggregate: summarize(&aggregate_durations, events),
        file_backed,
        compaction,
        schema_migration,
        cross_adapter_restore,
        scheduler_attribution,
        memory_high_water,
    };
    write_report(Path::new(&output), &report)?;
    Ok(output)
}

fn measure_memory_high_water(
    run_count: usize,
    samples_per_run: usize,
    events_per_sample: usize,
) -> Result<MemoryHighWaterBenchmark, String> {
    let mut runs = Vec::with_capacity(run_count);
    let mut aggregate_values = Vec::with_capacity(run_count * samples_per_run);
    for run_index in 0..run_count {
        let mut values = Vec::with_capacity(samples_per_run);
        for sample_index in 0..samples_per_run {
            values.push(execute_memory_high_water_sample(
                run_index * samples_per_run + sample_index,
                events_per_sample,
            )?);
        }
        aggregate_values.extend_from_slice(&values);
        runs.push(summarize_size(values));
    }
    let aggregate = summarize_size(aggregate_values);
    Ok(MemoryHighWaterBenchmark {
        run_count,
        samples_per_run,
        events_per_sample,
        soft_limit_bytes: MEMORY_SOFT_LIMIT_BYTES,
        hard_limit_bytes: MEMORY_HARD_LIMIT_BYTES,
        correctness_verified: true,
        budget_pass: aggregate.p99_bytes <= MEMORY_HARD_LIMIT_BYTES as u64,
        runs,
        aggregate,
    })
}

fn execute_memory_high_water_sample(
    sample: usize,
    events_per_sample: usize,
) -> Result<u64, String> {
    execute_sample(sample, events_per_sample)?;
    let replay_value = ReplValue::Record {
        name: "PersistentActorReplay".to_string(),
        fields: vec![
            (
                "actor_id".to_string(),
                ReplValue::String("memory-benchmark".to_string()),
            ),
            ("snapshot".to_string(), ReplValue::Int(0)),
            (
                "events".to_string(),
                ReplValue::List(
                    (1..=events_per_sample)
                        .map(|sequence| ReplValue::Int(sequence as i64))
                        .collect(),
                ),
            ),
        ],
    };
    let logical_bytes = logical_value_bytes(&replay_value)
        .map_err(|error| format!("persistent actor memory sizing failed: {error}"))?;
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(VmProcessSource::new("bench.PersistentActor", "memory", 0));
    let limits = VmMemoryLimits::new(MEMORY_SOFT_LIMIT_BYTES, MEMORY_HARD_LIMIT_BYTES)?;
    let mut memory = VmMemoryAccountant::new(limits);
    let decision = memory.account_heap(&mut processes, pid, logical_bytes)?;
    if decision.outcome != VmMemoryPressureOutcome::Accounted {
        return Err("persistent actor memory benchmark exceeded its soft limit".to_string());
    }
    let high_water = memory
        .process_metrics(pid)
        .ok_or_else(|| "persistent actor memory metrics are missing".to_string())?
        .high_water_bytes;
    let released = memory.release_heap(&mut processes, pid, logical_bytes)?;
    let metrics = memory
        .process_metrics(pid)
        .ok_or_else(|| "persistent actor memory metrics disappeared after release".to_string())?;
    if released != logical_bytes
        || metrics.current_bytes != 0
        || metrics.high_water_bytes != logical_bytes
        || metrics.collection_events != 1
        || metrics.released_bytes != logical_bytes
    {
        return Err("persistent actor memory high-water accounting is inconsistent".to_string());
    }
    u64::try_from(high_water)
        .map_err(|_| "persistent actor memory high-water exceeds report range".to_string())
}

fn summarize_size(mut values: Vec<u64>) -> SizeDistribution {
    values.sort_unstable();
    SizeDistribution {
        sample_count: values.len(),
        p50_bytes: percentile_u64(&values, 50),
        p95_bytes: percentile_u64(&values, 95),
        p99_bytes: percentile_u64(&values, 99),
    }
}

fn measure_scheduler_attribution(
    run_count: usize,
    samples_per_run: usize,
    events_per_sample: usize,
) -> Result<SchedulerAttributionBenchmark, String> {
    let reductions_per_sample = u64::try_from(events_per_sample)
        .map_err(|_| "scheduler benchmark event count exceeds u64".to_string())?
        .checked_add(2)
        .ok_or_else(|| "scheduler benchmark reduction count overflow".to_string())?;
    let mut runs = Vec::with_capacity(run_count);
    let mut aggregate_overheads = Vec::with_capacity(run_count * samples_per_run);
    let mut aggregate_ticks = 0_u64;
    let mut aggregate_reductions = 0_u64;
    for run_index in 0..run_count {
        let run = execute_scheduler_attribution_run(
            run_index,
            samples_per_run,
            events_per_sample,
            reductions_per_sample,
        )?;
        aggregate_ticks = aggregate_ticks.saturating_add(run.scheduler_ticks);
        aggregate_reductions = aggregate_reductions.saturating_add(run.reductions_charged);
        aggregate_overheads.extend(run.scheduler_overhead_samples.iter().copied());
        runs.push(SchedulerAttributionRun {
            sample_count: samples_per_run,
            scheduler_ticks: run.scheduler_ticks,
            reductions_charged: run.reductions_charged,
            scheduler_overhead: summarize_phase(run.scheduler_overhead_samples),
        });
    }
    Ok(SchedulerAttributionBenchmark {
        run_count,
        samples_per_run,
        events_per_sample,
        reductions_per_sample,
        correctness_verified: true,
        runs,
        aggregate: SchedulerAttributionRun {
            sample_count: run_count * samples_per_run,
            scheduler_ticks: aggregate_ticks,
            reductions_charged: aggregate_reductions,
            scheduler_overhead: summarize_phase(aggregate_overheads),
        },
    })
}

struct SchedulerAttributionSamples {
    scheduler_ticks: u64,
    reductions_charged: u64,
    scheduler_overhead_samples: Vec<u128>,
}

fn execute_scheduler_attribution_run(
    run_index: usize,
    samples_per_run: usize,
    events_per_sample: usize,
    reductions_per_sample: u64,
) -> Result<SchedulerAttributionSamples, String> {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::default();
    let mut overheads = Vec::with_capacity(samples_per_run);
    let mut final_tick = 0_u64;
    for sample_index in 0..samples_per_run {
        let pid = processes.spawn_root(VmProcessSource::new("bench.PersistentActor", "replay", 0));
        scheduler.enqueue_runnable(&processes, pid)?;
        let mut workload_ns = 0_u128;
        let mut workload_error = None;
        let started = Instant::now();
        let run = scheduler.run_next(&mut processes, |_process, _slice| {
            let workload_started = Instant::now();
            if let Err(error) = execute_sample(
                run_index * samples_per_run + sample_index,
                events_per_sample,
            ) {
                workload_error = Some(error);
            }
            workload_ns = workload_started.elapsed().as_nanos();
            VmSchedulerDecision::Exit {
                reductions: reductions_per_sample,
                reason: VmExitReason::Normal,
            }
        })?;
        if let Some(error) = workload_error {
            return Err(error);
        }
        if run.pid != Some(pid)
            || run.reductions_charged != reductions_per_sample
            || !matches!(run.outcome, VmSchedulerOutcome::Exited(_))
        {
            return Err(format!(
                "persistent actor scheduler benchmark returned invalid run: {run:?}"
            ));
        }
        final_tick = run.tick;
        overheads.push(
            started
                .elapsed()
                .as_nanos()
                .saturating_sub(workload_ns)
                .max(1),
        );
    }
    let metrics = scheduler.metrics();
    let expected_reductions = reductions_per_sample
        .checked_mul(samples_per_run as u64)
        .ok_or_else(|| "scheduler benchmark aggregate reductions overflow".to_string())?;
    if final_tick != samples_per_run as u64
        || metrics.total_slices != samples_per_run as u64
        || metrics.total_reductions != expected_reductions
    {
        return Err("persistent actor scheduler accounting did not match executed samples".into());
    }
    Ok(SchedulerAttributionSamples {
        scheduler_ticks: final_tick,
        reductions_charged: metrics.total_reductions,
        scheduler_overhead_samples: overheads,
    })
}

fn measure_cross_adapter_restore(
    run_count: usize,
    samples_per_run: usize,
) -> Result<CrossAdapterRestoreBenchmark, String> {
    let (runs, aggregate) = measure_phase_runs(run_count, samples_per_run, |_| {
        execute_cross_adapter_restore_sample()
    })?;
    Ok(CrossAdapterRestoreBenchmark {
        run_count,
        samples_per_run,
        source_adapter: "embedded-key-value",
        destination_adapter: "database-backed",
        events_per_restore: 2,
        correctness_verified: true,
        runs,
        aggregate,
    })
}

fn execute_cross_adapter_restore_sample() -> Result<u128, String> {
    let started = Instant::now();
    let execution = execute_persistent_actor_adapter_cross_adapter_restore()
        .map_err(|error| format!("cross-adapter restore benchmark failed: {error:?}"))?;
    let duration = started.elapsed().as_nanos().max(1);
    if execution.source_adapter_kind != "embedded-key-value"
        || execution.destination_adapter_kind != "database-backed"
        || execution.snapshot_generation != 1
        || execution.restored_event_count != 2
        || execution.replayed_event_count != 2
    {
        return Err("cross-adapter restore benchmark produced invalid replay evidence".to_string());
    }
    Ok(duration)
}

fn measure_schema_migration(
    run_count: usize,
    samples_per_run: usize,
) -> Result<SchemaMigrationBenchmark, String> {
    let (runs, aggregate) =
        measure_phase_runs(run_count, samples_per_run, execute_schema_migration_sample)?;
    Ok(SchemaMigrationBenchmark {
        run_count,
        samples_per_run,
        schema_versions: MIGRATION_SCHEMA_VERSIONS,
        planned_edges: MIGRATION_SCHEMA_VERSIONS - 1,
        correctness_verified: true,
        runs,
        aggregate,
    })
}

fn execute_schema_migration_sample(sample: usize) -> Result<u128, String> {
    let schema_id = format!("migration-benchmark-{sample}");
    let schemas = (1..=MIGRATION_SCHEMA_VERSIONS)
        .map(|version| VmPersistentActorSchemaKey::new(&schema_id, version as u64))
        .collect::<Result<Vec<_>, _>>()?;
    let mut graph = VmPersistentActorMigrationGraph::new();
    for (index, schema) in schemas.iter().enumerate() {
        let state = VmPersistentActorField::required("state", "Int")?;
        let descriptor = VmPersistentActorSchemaDescriptor::new(schema.clone(), index as u64 + 1)
            .map_err(|error| format!("migration benchmark descriptor failed: {error:?}"))?
            .with_field(state)
            .map_err(|error| format!("migration benchmark field failed: {error:?}"))?
            .with_event_variant("Changed")?;
        graph
            .register_schema(descriptor)
            .map_err(|error| format!("migration benchmark registration failed: {error:?}"))?;
    }
    for pair in schemas.windows(2) {
        graph
            .add_edge(VmPersistentActorMigrationEdge::new(
                pair[0].clone(),
                pair[1].clone(),
            ))
            .map_err(|error| format!("migration benchmark edge failed: {error:?}"))?;
    }

    let started = Instant::now();
    let plan = graph
        .plan(&schemas[0], &schemas[MIGRATION_SCHEMA_VERSIONS - 1])
        .map_err(|error| format!("migration benchmark plan failed: {error:?}"))?;
    let duration = started.elapsed().as_nanos().max(1);
    let ordered = plan
        .iter()
        .enumerate()
        .all(|(index, edge)| edge.from == schemas[index] && edge.to == schemas[index + 1]);
    if plan.len() != MIGRATION_SCHEMA_VERSIONS - 1 || !ordered {
        return Err("migration benchmark produced an invalid schema chain".to_string());
    }
    Ok(duration)
}

fn measure_compaction(
    run_count: usize,
    samples_per_run: usize,
) -> Result<CompactionBenchmark, String> {
    let (runs, aggregate) =
        measure_phase_runs(run_count, samples_per_run, execute_compaction_sample)?;
    Ok(CompactionBenchmark {
        run_count,
        samples_per_run,
        events_before: COMPACTION_EVENTS,
        events_retained: COMPACTION_EVENTS - COMPACTED_THROUGH,
        correctness_verified: true,
        runs,
        aggregate,
    })
}

fn measure_phase_runs(
    run_count: usize,
    samples_per_run: usize,
    mut execute: impl FnMut(usize) -> Result<u128, String>,
) -> Result<(Vec<PhaseLatency>, PhaseLatency), String> {
    let mut runs = Vec::with_capacity(run_count);
    let mut aggregate = Vec::with_capacity(run_count * samples_per_run);
    for run in 0..run_count {
        let mut durations = Vec::with_capacity(samples_per_run);
        for sample in 0..samples_per_run {
            durations.push(execute(run * samples_per_run + sample)?);
        }
        aggregate.extend_from_slice(&durations);
        runs.push(summarize_phase(durations));
    }
    Ok((runs, summarize_phase(aggregate)))
}

fn execute_compaction_sample(sample: usize) -> Result<u128, String> {
    let actor_id = VmPersistentActorId::new(format!("compaction-benchmark-{sample}"))?;
    let schema = VmPersistentActorSchema::new("benchmark-state", 1)?;
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::Int(0),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
    )?;
    let events = (1..=COMPACTION_EVENTS)
        .map(|sequence| {
            VmPersistentActorEvent::new(
                actor_id.clone(),
                schema.clone(),
                sequence as u64,
                ReplValue::Int(sequence as i64),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let before = persistent_actor_store::VmPersistentActorReplay {
        snapshot,
        events: events.clone(),
    };
    let compacted_snapshot = VmPersistentActorSnapshot::new(
        actor_id,
        schema,
        2,
        ReplValue::Int(COMPACTED_THROUGH as i64),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        COMPACTED_THROUGH as u64,
    )?;
    let equivalence = VmPersistentActorReplayEquivalence::from_snapshot(&compacted_snapshot);
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted_snapshot,
        retained_events: events[COMPACTED_THROUGH..].to_vec(),
    };
    let policy = VmPersistentActorRetentionPolicy::new((COMPACTED_THROUGH + 1) as u64);
    let started = Instant::now();
    let plan = plan_persistent_actor_compaction(&before, &equivalence, &candidate, &policy)
        .map_err(|error| format!("compaction benchmark plan failed: {error:?}"))?;
    let duration = started.elapsed().as_nanos().max(1);
    if plan.compacted_snapshot_generation != 2
        || plan.retained_event_sequences.len() != COMPACTION_EVENTS - COMPACTED_THROUGH
    {
        return Err("compaction benchmark produced an invalid retained suffix".to_string());
    }
    Ok(duration)
}

fn measure_file_backed(run_count: usize) -> Result<FileBackedBenchmark, String> {
    for sample in 0..FILE_WARMUP_SAMPLES {
        execute_file_backed_sample(sample, FILE_EVENTS)?;
    }
    let mut runs = Vec::with_capacity(run_count);
    let mut aggregate = Vec::with_capacity(run_count * FILE_SAMPLES);
    for run in 0..run_count {
        let mut samples = Vec::with_capacity(FILE_SAMPLES);
        for sample in 0..FILE_SAMPLES {
            samples.push(execute_file_backed_sample(
                FILE_WARMUP_SAMPLES + run * FILE_SAMPLES + sample,
                FILE_EVENTS,
            )?);
        }
        runs.push(summarize_file_backed(&samples));
        aggregate.extend(samples);
    }
    Ok(FileBackedBenchmark {
        run_count,
        samples_per_run: FILE_SAMPLES,
        events_per_sample: FILE_EVENTS,
        correctness_verified: true,
        runs,
        aggregate: summarize_file_backed(&aggregate),
    })
}

fn execute_file_backed_sample(
    sample: usize,
    event_count: usize,
) -> Result<FileBackedSample, String> {
    let path = env::temp_dir().join(format!(
        "terlan-persistent-actor-benchmark-{}-{sample}.log",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let actor_id = VmPersistentActorId::new(format!("file-benchmark-{sample}"))?;
    let schema = VmPersistentActorSchema::new("benchmark-state", 1)?;
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::Int(0),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
    )?;
    let mut store = VmFileBackedPersistentActorStore::open_file_backed(&path)?;
    let snapshot_started = Instant::now();
    if !matches!(
        store.store_snapshot(snapshot),
        VmPersistentActorStoreOutcome::SnapshotStored(_)
    ) {
        return Err("file-backed adapter rejected benchmark snapshot".to_string());
    }
    let snapshot_ns = snapshot_started.elapsed().as_nanos().max(1);
    let append_started = Instant::now();
    for sequence in 1..=event_count {
        let event = VmPersistentActorEvent::new(
            actor_id.clone(),
            schema.clone(),
            sequence as u64,
            ReplValue::Int(sequence as i64),
        )?;
        if !matches!(
            store.append_event(event),
            VmPersistentActorStoreOutcome::EventAppended(_)
        ) {
            return Err(format!(
                "file-backed adapter rejected benchmark event {sequence}"
            ));
        }
    }
    let append_ns = append_started.elapsed().as_nanos().max(1);
    let disk_bytes = fs::metadata(&path)
        .map_err(|error| format!("failed to stat file-backed benchmark log: {error}"))?
        .len();
    drop(store);
    let reopen_started = Instant::now();
    let reopened = VmFileBackedPersistentActorStore::open_file_backed(&path)?;
    let reopen_ns = reopen_started.elapsed().as_nanos().max(1);
    let replay_started = Instant::now();
    let replay = reopened
        .replay(&actor_id, &schema)
        .map_err(|outcome| format!("file-backed adapter rejected benchmark replay: {outcome:?}"))?;
    let replay_ns = replay_started.elapsed().as_nanos().max(1);
    let reopen_replay_ns = reopen_started.elapsed().as_nanos().max(1);
    fs::remove_file(&path)
        .map_err(|error| format!("failed to remove file-backed benchmark log: {error}"))?;
    if replay.events.len() != event_count {
        return Err(format!(
            "file-backed replay returned {} events, expected {event_count}",
            replay.events.len()
        ));
    }
    Ok(FileBackedSample {
        snapshot_ns,
        append_ns,
        reopen_ns,
        replay_ns,
        reopen_replay_ns,
        disk_bytes,
    })
}

fn summarize_file_backed(samples: &[FileBackedSample]) -> FileBackedRun {
    let snapshot = samples.iter().map(|sample| sample.snapshot_ns).collect();
    let append = samples.iter().map(|sample| sample.append_ns).collect();
    let reopen = samples.iter().map(|sample| sample.reopen_ns).collect();
    let replay = samples.iter().map(|sample| sample.replay_ns).collect();
    let reopen_replay = samples
        .iter()
        .map(|sample| sample.reopen_replay_ns)
        .collect();
    let mut disk = samples
        .iter()
        .map(|sample| sample.disk_bytes)
        .collect::<Vec<_>>();
    disk.sort_unstable();
    FileBackedRun {
        sample_count: samples.len(),
        snapshot_commit: summarize_phase(snapshot),
        append_events: summarize_phase(append),
        reopen_load: summarize_phase(reopen),
        vm_replay: summarize_phase(replay),
        reopen_replay: summarize_phase(reopen_replay),
        disk_bytes_p50: percentile_u64(&disk, 50),
        disk_bytes_p99: percentile_u64(&disk, 99),
    }
}

fn summarize_phase(mut values: Vec<u128>) -> PhaseLatency {
    values.sort_unstable();
    PhaseLatency {
        p50_ns: percentile(&values, 50),
        p95_ns: percentile(&values, 95),
        p99_ns: percentile(&values, 99),
    }
}

fn percentile_u64(sorted: &[u64], percentile: usize) -> u64 {
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

fn execute_sample(sample: usize, event_count: usize) -> Result<(), String> {
    let actor_id = VmPersistentActorId::new(format!("benchmark-{sample}"))?;
    let schema = VmPersistentActorSchema::new("benchmark-state", 1)?;
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::Int(0),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
    )?;
    let mut store = VmInMemoryPersistentActorStore::new();
    if !matches!(
        store.store_snapshot(snapshot),
        VmPersistentActorStoreOutcome::SnapshotStored(_)
    ) {
        return Err("in-memory adapter rejected benchmark snapshot".to_string());
    }
    for sequence in 1..=event_count {
        let event = VmPersistentActorEvent::new(
            actor_id.clone(),
            schema.clone(),
            sequence as u64,
            ReplValue::Int(sequence as i64),
        )?;
        if !matches!(
            store.append_event(event),
            VmPersistentActorStoreOutcome::EventAppended(_)
        ) {
            return Err(format!(
                "in-memory adapter rejected benchmark event {sequence}"
            ));
        }
    }
    let replay = store
        .replay(&actor_id, &schema)
        .map_err(|outcome| format!("in-memory adapter rejected benchmark replay: {outcome:?}"))?;
    if replay.events.len() != event_count {
        return Err(format!(
            "benchmark replay returned {} events, expected {event_count}",
            replay.events.len()
        ));
    }
    Ok(())
}

fn summarize(durations: &[u128], events_per_sample: usize) -> PersistentActorRun {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    let total_ns = sorted.iter().sum::<u128>().max(1);
    let operation_count = sorted.len() * events_per_sample;
    PersistentActorRun {
        sample_count: sorted.len(),
        operation_count,
        p50_ns: percentile(&sorted, 50),
        p95_ns: percentile(&sorted, 95),
        p99_ns: percentile(&sorted, 99),
        throughput_events_per_second: (operation_count as u128 * 1_000_000_000) / total_ns,
    }
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

fn env_usize(name: &str, default: usize) -> Result<usize, String> {
    let Some(value) = env::var_os(name) else {
        return Ok(default);
    };
    let value = value
        .to_str()
        .ok_or_else(|| format!("{name} must be valid UTF-8"))?
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if value == 0 {
        return Err(format!("{name} must be a positive integer"));
    }
    Ok(value)
}
