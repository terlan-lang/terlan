//! Versioned observation references for multicore actor scaling.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::cpu_actor::{CpuBoundActorEvidence, CPU_ITERATIONS_PER_ACTOR, CPU_SAMPLE_COUNT};
use super::mixed_tail::{
    MixedTailEvidence, MIXED_CPU_PASSES, MIXED_LOAD_SCHEDULERS, MIXED_TAIL_METRICS,
    MIXED_TAIL_OPERATIONS,
};
use super::WARMUP_SAMPLE_COUNT;

const POLICY_SCHEMA: &str = "terlan-vm-multicore-performance-limits-v1";
const CANONICAL_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../benchmarks/baselines/vm-multicore-performance-limits.json"
));

/// Versioned multicore workload shape and historical observation references.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MulticorePerformancePolicy {
    /// Versioned policy document schema.
    pub(super) schema: String,
    /// Historical runner label attached to the reference measurements.
    pub(super) dedicated_runner_label: String,
    /// Required target operating system.
    pub(super) required_target_os: String,
    /// Required target architecture.
    pub(super) required_target_arch: String,
    /// Minimum effective CPUs after affinity and cgroup limits.
    pub(super) minimum_effective_cpus: usize,
    /// Exact number of CPU-bound samples required by the policy.
    pub(super) required_sample_count: usize,
    /// Exact generated integer-mixing iterations per actor.
    pub(super) required_iterations_per_actor: usize,
    /// Minimum width-two median throughput divided by width-one throughput.
    pub(super) minimum_two_scheduler_median_speedup: f64,
    /// Minimum accepted lower confidence bound for width-two speedup.
    pub(super) minimum_two_scheduler_confidence_lower_bound: f64,
    /// Recorded reference width-one median actor throughput.
    pub(super) reference_one_scheduler_actors_per_second: u128,
    /// Minimum current/reference width-one throughput ratio.
    pub(super) minimum_one_scheduler_throughput_ratio: f64,
    /// Recorded reference width-one p99 batch duration.
    pub(super) reference_one_scheduler_p99_ns: u128,
    /// Maximum current/reference width-one p99 ratio.
    pub(super) maximum_one_scheduler_p99_ratio: f64,
    /// Versioned p95 and p99 limits for every mixed-load runtime path.
    pub(super) mixed_tail_budgets: Vec<MixedTailBudget>,
}

/// Versioned latency references and accepted ratios for one mixed-load path.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MixedTailBudget {
    /// Stable metric identity.
    pub(super) metric: String,
    /// Recorded reference p95 latency.
    pub(super) reference_p95_ns: u128,
    /// Maximum current/reference p95 ratio.
    pub(super) maximum_p95_ratio: f64,
    /// Recorded reference p99 latency.
    pub(super) reference_p99_ns: u128,
    /// Maximum current/reference p99 ratio.
    pub(super) maximum_p99_ratio: f64,
}

/// Machine-readable result of recording an observation against references.
#[derive(Clone, Debug, Serialize)]
pub(super) struct MulticorePerformancePolicyEvidence {
    /// Policy schema applied to this report.
    pub(super) schema: String,
    /// Historical runner label attached to the reference measurements.
    pub(super) dedicated_runner_label: String,
    /// Runner label requested by the environment.
    pub(super) requested_runner_label: Option<String>,
    /// Whether hardware-specific budgets were enforced.
    pub(super) enforced: bool,
    /// Stable policy outcome.
    pub(super) status: &'static str,
    /// Reason enforcement was not activated on an ordinary host.
    pub(super) record_only_reason: Option<&'static str>,
    /// Observed width-one to width-two median speedup.
    pub(super) observed_two_scheduler_median_speedup: f64,
    /// Observed lower 95% confidence bound.
    pub(super) observed_two_scheduler_confidence_lower_bound: f64,
    /// Observed width-one median actors per second.
    pub(super) observed_one_scheduler_actors_per_second: u128,
    /// Observed width-one p99 batch duration.
    pub(super) observed_one_scheduler_p99_ns: u128,
    /// Observed mixed-load tails joined to their versioned references.
    pub(super) mixed_tail: Vec<MixedTailPolicyEvidence>,
}

/// Machine-readable policy evidence for one mixed-load runtime path.
#[derive(Clone, Debug, Serialize)]
pub(super) struct MixedTailPolicyEvidence {
    /// Stable metric identity.
    pub(super) metric: String,
    /// Observed p95 latency.
    pub(super) observed_p95_ns: u128,
    /// Observed p99 latency.
    pub(super) observed_p99_ns: u128,
    /// Observed/reference p95 ratio.
    pub(super) p95_ratio: f64,
    /// Observed/reference p99 ratio.
    pub(super) p99_ratio: f64,
}

/// Parses and validates a strict multicore performance policy.
pub(super) fn parse_policy(
    path: &Path,
    bytes: &[u8],
) -> Result<MulticorePerformancePolicy, String> {
    let policy: MulticorePerformancePolicy = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "failed to parse multicore performance policy `{}`: {error}",
            path.display()
        )
    })?;
    validate_policy(&policy)?;
    Ok(policy)
}

/// Loads the compile-time canonical multicore performance policy.
pub(super) fn canonical_policy() -> Result<MulticorePerformancePolicy, String> {
    parse_policy(
        Path::new("embedded canonical multicore performance policy"),
        CANONICAL_POLICY.as_bytes(),
    )
}

/// Returns canonical policy bytes for report fingerprinting.
pub(super) const fn canonical_policy_bytes() -> &'static [u8] {
    CANONICAL_POLICY.as_bytes()
}

/// Records measurements and rejects deterministic workload-shape drift only.
pub(super) fn evaluate_policy(
    policy: &MulticorePerformancePolicy,
    cpu: &CpuBoundActorEvidence,
    mixed_tail: &MixedTailEvidence,
) -> Result<MulticorePerformancePolicyEvidence, String> {
    validate_policy(policy)?;
    let width_one = cpu.width(1)?;
    let mixed_tail_evidence = policy
        .mixed_tail_budgets
        .iter()
        .map(|budget| {
            let observed = mixed_tail.metric(&budget.metric)?;
            Ok(MixedTailPolicyEvidence {
                metric: budget.metric.clone(),
                observed_p95_ns: observed.timing.p95_ns,
                observed_p99_ns: observed.timing.p99_ns,
                p95_ratio: observed.timing.p95_ns as f64 / budget.reference_p95_ns as f64,
                p99_ratio: observed.timing.p99_ns as f64 / budget.reference_p99_ns as f64,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let observed = MulticorePerformancePolicyEvidence {
        schema: policy.schema.clone(),
        dedicated_runner_label: policy.dedicated_runner_label.clone(),
        requested_runner_label: None,
        enforced: false,
        status: "record_only",
        record_only_reason: Some("performance measurements are diagnostic-only"),
        observed_two_scheduler_median_speedup: cpu.width_one_to_two.median_speedup_ratio,
        observed_two_scheduler_confidence_lower_bound: cpu.width_one_to_two.lower_bound,
        observed_one_scheduler_actors_per_second: width_one.median_actors_per_second,
        observed_one_scheduler_p99_ns: width_one.timing.p99_ns,
        mixed_tail: mixed_tail_evidence,
    };
    if cpu.iterations_per_actor != policy.required_iterations_per_actor
        || cpu.warmup_samples_per_width != WARMUP_SAMPLE_COUNT
        || width_one.samples != policy.required_sample_count
        || mixed_tail.requested_schedulers != MIXED_LOAD_SCHEDULERS
        || mixed_tail.samples_per_metric != policy.required_sample_count
        || mixed_tail.warmup_samples_per_metric != WARMUP_SAMPLE_COUNT
        || mixed_tail.cpu_iterations_per_actor
            != CPU_ITERATIONS_PER_ACTOR.saturating_mul(MIXED_CPU_PASSES)
        || mixed_tail.maximum_simultaneously_active_schedulers < MIXED_LOAD_SCHEDULERS
        || !mixed_tail.cpu_overlap_proven
        || mixed_tail.measurements.len() != MIXED_TAIL_METRICS.len()
        || mixed_tail
            .measurements
            .iter()
            .zip(MIXED_TAIL_OPERATIONS)
            .any(|(measurement, operations)| {
                measurement.samples != policy.required_sample_count
                    || measurement.operations_per_sample != operations
            })
    {
        return Err(
            "error[vm.multicore.performance.workload_drift]: CPU workload no longer matches policy"
                .to_string(),
        );
    }
    Ok(observed)
}

/// Rejects malformed or weakened canonical policy limits.
fn validate_policy(policy: &MulticorePerformancePolicy) -> Result<(), String> {
    if policy.schema != POLICY_SCHEMA
        || policy.required_target_os != "linux"
        || policy.required_target_arch != "x86_64"
        || policy.minimum_effective_cpus < 2
        || policy.required_sample_count != CPU_SAMPLE_COUNT
        || policy.required_iterations_per_actor != CPU_ITERATIONS_PER_ACTOR
        || policy.minimum_two_scheduler_median_speedup < 1.5
        || policy.minimum_two_scheduler_confidence_lower_bound < 1.25
        || policy.reference_one_scheduler_actors_per_second == 0
        || policy.minimum_one_scheduler_throughput_ratio < 0.8
        || policy.reference_one_scheduler_p99_ns == 0
        || policy.maximum_one_scheduler_p99_ratio > 2.0
        || policy.mixed_tail_budgets.len() != MIXED_TAIL_METRICS.len()
    {
        return Err(
            "error[vm.multicore.performance.policy_shape]: invalid or weakened multicore performance policy"
                .to_string(),
        );
    }
    for (budget, expected) in policy.mixed_tail_budgets.iter().zip(MIXED_TAIL_METRICS) {
        if budget.metric != expected
            || budget.reference_p95_ns == 0
            || budget.reference_p99_ns == 0
            || !budget.maximum_p95_ratio.is_finite()
            || budget.maximum_p95_ratio <= 0.0
            || budget.maximum_p95_ratio > 2.0
            || !budget.maximum_p99_ratio.is_finite()
            || budget.maximum_p99_ratio <= 0.0
            || budget.maximum_p99_ratio > 2.0
        {
            return Err(
                "error[vm.multicore.performance.policy_shape]: invalid or weakened mixed-load tail policy"
                    .to_string(),
            );
        }
    }
    Ok(())
}
