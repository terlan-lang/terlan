//! Recorded fixed-scheduler width evidence for generated AOT execution.

use std::collections::BTreeSet;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::scheduler_topology::{VmSchedulerHostSnapshot, VmSchedulerTopology};
use crate::runtime::vm::ReplValue;

use super::handler_cache_test_support::compile_native_handler_fixture;
use super::AotHandlerGeneration;

#[path = "multicore_performance_test/cpu_actor.rs"]
mod cpu_actor;
#[path = "multicore_performance_test/mixed_tail.rs"]
mod mixed_tail;
#[path = "multicore_performance_test/policy.rs"]
mod policy;
#[path = "multicore_performance_test/policy_test.rs"]
mod policy_test;
#[path = "multicore_performance_test/workloads.rs"]
mod workloads;

const REPORT_SCHEMA: &str = "terlan.vm-multicore-performance.v1";
const REPORT_OUTPUT_ENV: &str = "TERLAN_VM_MULTICORE_PERFORMANCE_OUTPUT";
const DEFAULT_REPORT_OUTPUT: &str = "target/quality/vm-multicore-performance.json";
const OFFICIAL_REPOSITORY: &str = "terlan-lang/terlan";
const DEDICATED_RUNNER_ENV: &str = "TERLAN_VM_MULTICORE_DEDICATED_RUNNER";
const SAMPLE_COUNT: usize = 9;
const WIDTHS: [usize; 3] = [1, 2, 4];
const SOURCE: &str = r#"module app.MulticoreBenchmark.

import std.http.Response.
import type std.http.{Request, Response}.

pub ready(): Bool ->
    true.

pub cpu_burn(value: Int, remaining: Int): Int ->
    if {
        remaining == 0 -> value;
        true -> cpu_burn((value * 1664525 + 1013904223) rem 2147483647, remaining - 1)
    }.

pub cpu_bound(seed: Int): Int ->
    let phase_1 = cpu_burn(seed, 20000);
    let phase_2 = cpu_burn(phase_1, 20000);
    let phase_3 = cpu_burn(phase_2, 20000);
    let phase_4 = cpu_burn(phase_3, 20000);
    let phase_5 = cpu_burn(phase_4, 20000);
    let phase_6 = cpu_burn(phase_5, 20000);
    let phase_7 = cpu_burn(phase_6, 20000);
    let phase_8 = cpu_burn(phase_7, 20000);
    let phase_9 = cpu_burn(phase_8, 20000);
    cpu_burn(phase_9, 20000).

pub mixed_cpu_load(seed: Int): Int ->
    let pass_1 = cpu_bound(seed);
    let pass_2 = cpu_bound(pass_1);
    let pass_3 = cpu_bound(pass_2);
    let pass_4 = cpu_bound(pass_3);
    cpu_bound(pass_4).

pub http(_request: Request): Response ->
    Response.text("multicore").
"#;

/// One observed Linux load-average snapshot.
#[derive(Debug, Serialize)]
struct BackgroundLoad {
    one_minute: Option<f64>,
    five_minutes: Option<f64>,
    fifteen_minutes: Option<f64>,
    declared_state: String,
}

/// Stable latency distribution for one scheduler width.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct TimingDistribution {
    minimum_ns: u128,
    median_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    maximum_ns: u128,
    median_absolute_deviation_ns: u128,
}

/// Actual scheduler-owner evidence for one requested width.
#[derive(Debug, Serialize)]
struct SchedulerWidthMeasurement {
    requested_schedulers: usize,
    samples: usize,
    actor_executions_per_sample: usize,
    operations_per_second: u128,
    maximum_simultaneously_active_schedulers: usize,
    distinct_scheduler_owner_threads: Vec<String>,
    overlap_proven: bool,
    timing: TimingDistribution,
}

/// Timing for one identical runtime workload at one scheduler width.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct RuntimeWorkloadMeasurement {
    workload: &'static str,
    execution_scope: &'static str,
    requested_schedulers: usize,
    samples: usize,
    operations_per_lane: usize,
    operations_per_sample: usize,
    operations_per_second: u128,
    timing: TimingDistribution,
}

/// Source and execution identity attached to performance evidence.
#[derive(Debug, Serialize)]
struct PerformanceProvenance {
    /// Local or GitHub Actions execution environment.
    execution_environment: &'static str,
    /// Official GitHub repository for hosted evidence.
    repository: Option<String>,
    /// Exact workflow reference that launched hosted evidence.
    workflow_ref: Option<String>,
    /// Numeric GitHub Actions run identity.
    run_id: Option<u64>,
    /// Numeric retry identity within one workflow run.
    run_attempt: Option<u64>,
    /// GitHub commit identity associated with the workflow.
    commit_sha: Option<String>,
    /// Physical or virtual GitHub runner identity.
    runner_name: Option<String>,
    /// GitHub-hosted or self-hosted runner classification.
    runner_environment: Option<String>,
}

/// Machine-readable first-slice MC-9 performance report.
#[derive(Debug, Serialize)]
struct MulticorePerformanceReport {
    schema: &'static str,
    benchmark: &'static str,
    generated_at_unix_seconds: u64,
    terlan_version: &'static str,
    rustc_version: Option<String>,
    source_revision: String,
    provenance: PerformanceProvenance,
    target_os: &'static str,
    target_arch: &'static str,
    optimization_profile: &'static str,
    hardware: VmSchedulerHostSnapshot,
    background_load: BackgroundLoad,
    eligible_for_parallel_assertion: bool,
    sample_count: usize,
    workload_sha256: String,
    native_image_sha256: String,
    runtime_workload_contract_sha256: String,
    mixed_tail_contract_sha256: String,
    performance_policy_sha256: String,
    benchmark_sha256: String,
    measurements: Vec<SchedulerWidthMeasurement>,
    cpu_bound_actor: cpu_actor::CpuBoundActorEvidence,
    mixed_load_tail: mixed_tail::MixedTailEvidence,
    performance_policy: policy::MulticorePerformancePolicyEvidence,
    workload_measurements: Vec<RuntimeWorkloadMeasurement>,
    pending_policy: &'static str,
}

/// Generates the first MC-9 fixed-scheduler performance artifact.
#[test]
#[ignore = "records host-specific release performance evidence"]
fn multicore_runtime_width_matrix_records_workloads_and_owner_overlap() {
    let fixture = compile_native_handler_fixture(
        "multicore_performance",
        "src/app/MulticoreBenchmark.terl",
        "app_MulticoreBenchmark",
        SOURCE,
    );
    let hardware = VmSchedulerHostSnapshot::capture();
    let source_revision = source_revision().expect("resolve benchmark source revision");
    let provenance =
        performance_provenance(&source_revision).expect("validate benchmark provenance");
    let optimization_profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let background_load = background_load();
    let image_bytes = fs::read(&fixture.image).expect("read generated native image");
    let measurements = WIDTHS
        .into_iter()
        .map(|width| measure_scheduler_width(&fixture.image, width, SAMPLE_COUNT))
        .collect::<Result<Vec<_>, _>>()
        .expect("measure generated AOT scheduler widths");
    let cpu_bound_actor =
        cpu_actor::measure_cpu_bound_actor(&fixture.image, cpu_actor::CPU_SAMPLE_COUNT, &WIDTHS)
            .expect("measure CPU-bound generated actors");
    let mixed_load_tail =
        mixed_tail::measure_mixed_tail(&fixture.image, &fixture.root, cpu_actor::CPU_SAMPLE_COUNT)
            .expect("measure mixed CPU and I/O tails");
    let performance_policy = policy::canonical_policy().expect("load multicore policy");
    let runner_context = policy::DedicatedRunnerContext::capture(
        optimization_profile,
        hardware.effective_parallelism(),
        background_load.declared_state.clone(),
    );
    let performance_policy_evidence = policy::evaluate_policy(
        &performance_policy,
        &runner_context,
        &cpu_bound_actor,
        &mixed_load_tail,
    )
    .expect("enforce dedicated multicore runner policy");
    let workload_measurements = workloads::measure_runtime_workloads(
        &fixture.image,
        fixture.router.clone(),
        &fixture.root,
        SAMPLE_COUNT,
        &WIDTHS,
    )
    .expect("measure identical multicore runtime workloads");
    assert_eq!(
        workload_measurements.len(),
        WIDTHS.len() * workloads::WORKLOAD_NAMES.len()
    );
    for width in WIDTHS {
        let names = workload_measurements
            .iter()
            .filter(|measurement| measurement.requested_schedulers == width)
            .map(|measurement| measurement.workload)
            .collect::<Vec<_>>();
        assert_eq!(names, workloads::WORKLOAD_NAMES);
    }
    let eligible_for_parallel_assertion = hardware.effective_parallelism() >= 2;
    let two_scheduler = measurements
        .iter()
        .find(|measurement| measurement.requested_schedulers == 2)
        .expect("two-scheduler measurement");
    if eligible_for_parallel_assertion {
        assert!(
            two_scheduler.maximum_simultaneously_active_schedulers >= 2,
            "eligible host did not overlap two scheduler-owned AOT executions"
        );
    }
    let report = MulticorePerformanceReport {
        schema: REPORT_SCHEMA,
        benchmark: "generated_aot_fixed_scheduler_width_matrix",
        generated_at_unix_seconds: unix_timestamp_seconds(),
        terlan_version: env!("CARGO_PKG_VERSION"),
        rustc_version: rustc_version(),
        source_revision,
        provenance,
        target_os: env::consts::OS,
        target_arch: env::consts::ARCH,
        optimization_profile,
        hardware,
        background_load,
        eligible_for_parallel_assertion,
        sample_count: SAMPLE_COUNT,
        workload_sha256: sha256_hex(SOURCE.as_bytes()),
        native_image_sha256: sha256_hex(&image_bytes),
        runtime_workload_contract_sha256: sha256_hex(workloads::WORKLOAD_CONTRACT.as_bytes()),
        mixed_tail_contract_sha256: sha256_hex(mixed_tail::MIXED_TAIL_CONTRACT.as_bytes()),
        performance_policy_sha256: sha256_hex(policy::canonical_policy_bytes()),
        benchmark_sha256: benchmark_hash(),
        measurements,
        cpu_bound_actor,
        mixed_load_tail,
        performance_policy: performance_policy_evidence,
        workload_measurements,
        pending_policy: "same-revision performance and sanitizer evidence remain MC-9 work",
    };
    write_report(&report).expect("write multicore performance report");
    fs::remove_dir_all(fixture.root).expect("cleanup multicore performance fixture");
}

/// Validates distribution statistics without relying on host timing.
#[test]
fn timing_distribution_is_stable_for_even_and_odd_samples() {
    assert_eq!(
        timing_distribution(&[9, 1, 5, 3, 7]).expect("odd distribution"),
        TimingDistribution {
            minimum_ns: 1,
            median_ns: 5,
            p95_ns: 9,
            p99_ns: 9,
            maximum_ns: 9,
            median_absolute_deviation_ns: 2,
        }
    );
    assert_eq!(
        timing_distribution(&[40, 10, 30, 20]).expect("even distribution"),
        TimingDistribution {
            minimum_ns: 10,
            median_ns: 25,
            p95_ns: 40,
            p99_ns: 40,
            maximum_ns: 40,
            median_absolute_deviation_ns: 10,
        }
    );
    assert!(timing_distribution(&[]).is_err());
}

/// Validates that the workload contract is complete and duplicate-free.
#[test]
fn runtime_workload_contract_is_complete_and_unique() {
    assert_eq!(
        workloads::WORKLOAD_NAMES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        workloads::WORKLOAD_NAMES.len()
    );
    for workload in workloads::WORKLOAD_NAMES {
        assert!(
            workloads::WORKLOAD_CONTRACT.contains(workload),
            "workload contract omitted {workload}"
        );
    }
    for (metric, operations) in mixed_tail::MIXED_TAIL_METRICS
        .iter()
        .zip(mixed_tail::MIXED_TAIL_OPERATIONS)
    {
        assert!(
            mixed_tail::MIXED_TAIL_CONTRACT.contains(&format!("{metric}:{operations}")),
            "mixed-load contract omitted {metric}:{operations}"
        );
    }
}

/// Executes one synchronized generated export per fixed scheduler owner.
fn measure_scheduler_width(
    image: &std::path::Path,
    width: usize,
    samples: usize,
) -> Result<SchedulerWidthMeasurement, String> {
    let sessions = VmHttpSessionService::new(VmHttpSessionRuntime::new(
        "terlc-multicore-benchmark",
        86_400,
    )?);
    let generation = AotHandlerGeneration::load_with_shard_count(image, sessions, width)?;
    let topology = VmSchedulerTopology::new(width)?;
    let mut durations = Vec::with_capacity(samples);
    let mut maximum_active = 0;
    let mut owner_threads = BTreeSet::new();
    for _ in 0..samples {
        let routes = topology
            .schedulers()
            .map(|scheduler| generation.route_new_actor_on(scheduler))
            .collect::<Result<Vec<_>, _>>()?;
        let barrier = Arc::new(Barrier::new(width));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let started = Instant::now();
        let results = std::thread::scope(|scope| {
            routes
                .iter()
                .copied()
                .map(|route| {
                    let owner = generation.shard(route.scheduler().index())?;
                    let barrier = Arc::clone(&barrier);
                    let active = Arc::clone(&active);
                    let maximum = Arc::clone(&maximum);
                    Ok(scope.spawn(move || {
                        owner.probe_execution(
                            route,
                            "app.MulticoreBenchmark.ready".to_string(),
                            barrier,
                            active,
                            maximum,
                        )
                    }))
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|joins| {
                    joins
                        .into_iter()
                        .map(|join| {
                            join.join().map_err(|_| {
                                "multicore benchmark client thread panicked".to_string()
                            })?
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
        });
        durations.push(started.elapsed().as_nanos());
        maximum_active = maximum_active.max(maximum.load(Ordering::SeqCst));
        for route in &routes {
            generation.release_actor_route(route.scheduler().index());
        }
        for (value, owner_thread) in results?? {
            if value != ReplValue::Bool(true) {
                return Err(format!(
                    "generated multicore benchmark returned {value:?}, expected true"
                ));
            }
            owner_threads.insert(owner_thread);
        }
    }
    let timing = timing_distribution(&durations)?;
    let operations_per_second = (width as u128)
        .saturating_mul(1_000_000_000)
        .checked_div(timing.median_ns.max(1))
        .unwrap_or(0);
    Ok(SchedulerWidthMeasurement {
        requested_schedulers: width,
        samples,
        actor_executions_per_sample: width,
        operations_per_second,
        maximum_simultaneously_active_schedulers: maximum_active,
        distinct_scheduler_owner_threads: owner_threads.into_iter().collect(),
        overlap_proven: maximum_active == width,
        timing,
    })
}

/// Computes deterministic nearest-rank tails and median absolute deviation.
fn timing_distribution(samples: &[u128]) -> Result<TimingDistribution, String> {
    if samples.is_empty() {
        return Err("multicore timing distribution requires at least one sample".to_string());
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let median_ns = median(&sorted);
    let mut deviations = sorted
        .iter()
        .map(|sample| sample.abs_diff(median_ns))
        .collect::<Vec<_>>();
    deviations.sort_unstable();
    Ok(TimingDistribution {
        minimum_ns: sorted[0],
        median_ns,
        p95_ns: percentile(&sorted, 95),
        p99_ns: percentile(&sorted, 99),
        maximum_ns: *sorted.last().expect("nonempty timing samples"),
        median_absolute_deviation_ns: median(&deviations),
    })
}

/// Returns the midpoint median of one sorted nonempty sample.
fn median(sorted: &[u128]) -> u128 {
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        sorted[middle - 1]
            .saturating_add(sorted[middle])
            .checked_div(2)
            .unwrap_or(0)
    } else {
        sorted[middle]
    }
}

/// Returns a nearest-rank percentile from one sorted nonempty sample.
fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    let rank = sorted
        .len()
        .saturating_mul(percentile)
        .saturating_add(99)
        .checked_div(100)
        .unwrap_or(1)
        .clamp(1, sorted.len());
    sorted[rank - 1]
}

/// Hashes the stable workload and width configuration.
fn benchmark_hash() -> String {
    let mut input = SOURCE.as_bytes().to_vec();
    input.extend_from_slice(format!("samples={SAMPLE_COUNT};widths={WIDTHS:?}").as_bytes());
    input.extend_from_slice(workloads::WORKLOAD_CONTRACT.as_bytes());
    input.extend_from_slice(mixed_tail::MIXED_TAIL_CONTRACT.as_bytes());
    input.extend_from_slice(policy::canonical_policy_bytes());
    sha256_hex(&input)
}

/// Returns lowercase SHA-256 evidence for one byte sequence.
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a string cannot fail");
            output
        })
}

/// Reads the active Rust compiler version when available.
fn rustc_version() -> Option<String> {
    let output = Command::new("rustc").arg("--version").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Returns the full checked-out Git revision used by the benchmark.
fn source_revision() -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("failed to inspect benchmark source revision: {error}"))?;
    if !output.status.success() {
        return Err("failed to inspect benchmark source revision".to_string());
    }
    let revision = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "benchmark source revision `{revision}` is not one full Git commit"
        ));
    }
    Ok(revision)
}

/// Captures local identity or validates complete official hosted provenance.
fn performance_provenance(source_revision: &str) -> Result<PerformanceProvenance, String> {
    if env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return Ok(PerformanceProvenance {
            execution_environment: "local",
            repository: None,
            workflow_ref: None,
            run_id: None,
            run_attempt: None,
            commit_sha: None,
            runner_name: None,
            runner_environment: None,
        });
    }
    let repository = required_environment("GITHUB_REPOSITORY")?;
    let workflow_ref = required_environment("GITHUB_WORKFLOW_REF")?;
    let commit_sha = required_environment("GITHUB_SHA")?;
    let runner_name = required_environment("RUNNER_NAME")?;
    let runner_environment = required_environment("RUNNER_ENVIRONMENT")?;
    let run_id = required_environment("GITHUB_RUN_ID")?
        .parse::<u64>()
        .map_err(|_| "GITHUB_RUN_ID must be numeric".to_string())?;
    let run_attempt = required_environment("GITHUB_RUN_ATTEMPT")?
        .parse::<u64>()
        .map_err(|_| "GITHUB_RUN_ATTEMPT must be numeric".to_string())?;
    if repository != OFFICIAL_REPOSITORY {
        return Err(format!(
            "performance evidence repository `{repository}` is not `{OFFICIAL_REPOSITORY}`"
        ));
    }
    if commit_sha != source_revision {
        return Err(format!(
            "performance workflow commit `{commit_sha}` does not match `{source_revision}`"
        ));
    }
    if env::var_os(DEDICATED_RUNNER_ENV).is_some() && runner_environment != "self-hosted" {
        return Err(
            "dedicated performance policy requires a self-hosted GitHub runner".to_string(),
        );
    }
    Ok(PerformanceProvenance {
        execution_environment: "github-actions",
        repository: Some(repository),
        workflow_ref: Some(workflow_ref),
        run_id: Some(run_id),
        run_attempt: Some(run_attempt),
        commit_sha: Some(commit_sha),
        runner_name: Some(runner_name),
        runner_environment: Some(runner_environment),
    })
}

/// Reads one required nonempty environment variable.
fn required_environment(name: &str) -> Result<String, String> {
    env::var(name)
        .map_err(|_| format!("{name} is required for hosted performance evidence"))
        .and_then(|value| {
            if value.trim().is_empty() {
                Err(format!("{name} must not be empty"))
            } else {
                Ok(value)
            }
        })
}

/// Captures Linux load averages and the caller's background-load declaration.
fn background_load() -> BackgroundLoad {
    let values = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|source| {
            source
                .split_whitespace()
                .take(3)
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
                .ok()
        })
        .unwrap_or_default();
    BackgroundLoad {
        one_minute: values.first().copied(),
        five_minutes: values.get(1).copied(),
        fifteen_minutes: values.get(2).copied(),
        declared_state: env::var("TERLAN_BENCH_BACKGROUND_LOAD")
            .unwrap_or_else(|_| "uncontrolled".to_string()),
    }
}

/// Returns the current Unix timestamp without panicking before the epoch.
fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Writes the host-specific report to its configured path.
fn write_report(report: &MulticorePerformanceReport) -> Result<(), String> {
    let path = env::var_os(REPORT_OUTPUT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPORT_OUTPUT));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create multicore report directory `{}`: {error}",
                parent.display()
            )
        })?;
    }
    let contents = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to serialize multicore report: {error}"))?;
    fs::write(&path, contents).map_err(|error| {
        format!(
            "failed to write multicore report `{}`: {error}",
            path.display()
        )
    })
}
