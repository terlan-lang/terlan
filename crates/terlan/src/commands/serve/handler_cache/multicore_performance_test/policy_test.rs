//! Deterministic multicore confidence and observational policy tests.

use super::cpu_actor::{
    speedup_confidence, CpuBoundActorEvidence, CpuBoundSpeedupConfidence, CpuBoundWidthMeasurement,
    CPU_ITERATIONS_PER_ACTOR, CPU_SAMPLE_COUNT,
};
use super::mixed_tail::{
    MixedTailEvidence, MixedTailMeasurement, MIXED_CPU_PASSES, MIXED_LOAD_SCHEDULERS,
    MIXED_TAIL_METRICS, MIXED_TAIL_OPERATIONS,
};
use super::policy::{canonical_policy, evaluate_policy, parse_policy};
use super::TimingDistribution;

/// Canonical policy is strict and rejects unknown or weakened limits.
#[test]
fn canonical_multicore_policy_rejects_shape_and_budget_drift() {
    let policy = canonical_policy().expect("canonical policy");
    assert_eq!(policy.required_sample_count, CPU_SAMPLE_COUNT);
    assert_eq!(
        policy.required_iterations_per_actor,
        CPU_ITERATIONS_PER_ACTOR
    );
    assert!(policy.minimum_two_scheduler_median_speedup >= 1.5);
    assert_eq!(policy.mixed_tail_budgets.len(), MIXED_TAIL_METRICS.len());

    let unknown = br#"{
        "schema":"terlan-vm-multicore-performance-limits-v1",
        "unknown":true
    }"#;
    assert!(parse_policy(std::path::Path::new("unknown"), unknown).is_err());

    let mut weakened = policy;
    weakened.minimum_two_scheduler_median_speedup = 1.49;
    let bytes = serde_json::to_vec(&weakened).expect("serialize weakened policy");
    assert!(parse_policy(std::path::Path::new("weakened"), &bytes).is_err());

    weakened.minimum_two_scheduler_median_speedup = 1.5;
    weakened.minimum_two_scheduler_confidence_lower_bound = 1.24;
    let bytes = serde_json::to_vec(&weakened).expect("serialize weakened confidence policy");
    assert!(parse_policy(std::path::Path::new("weak-confidence"), &bytes).is_err());

    weakened.minimum_two_scheduler_confidence_lower_bound = 1.25;
    weakened.mixed_tail_budgets[0].maximum_p99_ratio = 2.01;
    let bytes = serde_json::to_vec(&weakened).expect("serialize weakened mixed-tail policy");
    assert!(parse_policy(std::path::Path::new("weak-tail"), &bytes).is_err());

    weakened.mixed_tail_budgets[0].maximum_p99_ratio = 2.0;
    weakened.mixed_tail_budgets[1].metric = weakened.mixed_tail_budgets[0].metric.clone();
    let bytes = serde_json::to_vec(&weakened).expect("serialize duplicate mixed-tail policy");
    assert!(parse_policy(std::path::Path::new("duplicate-tail"), &bytes).is_err());
}

/// Bootstrap confidence is deterministic for constant independent samples.
#[test]
fn cpu_speedup_confidence_is_reproducible() {
    let one = width(1, 1_000, 1_000_000);
    let two = width(2, 2_000, 1_000_000);
    let first = speedup_confidence(&one, &two).expect("first confidence");
    let second = speedup_confidence(&one, &two).expect("second confidence");
    assert_eq!(first.median_speedup_ratio, 2.0);
    assert_eq!(first.lower_bound, 2.0);
    assert_eq!(first.upper_bound, 2.0);
    assert_eq!(first.seed, second.seed);
    assert_eq!(first.lower_bound, second.lower_bound);
    assert_eq!(first.upper_bound, second.upper_bound);
}

/// Performance policy is observational on every host.
#[test]
fn performance_policy_never_rejects_a_measurement() {
    let policy = canonical_policy().expect("canonical policy");
    let strong = evidence(
        1.75,
        1.4,
        policy.reference_one_scheduler_actors_per_second,
        policy.reference_one_scheduler_p99_ns,
    );
    let mixed = mixed_tail_evidence(&policy);
    let record = evaluate_policy(&policy, &strong, &mixed).expect("record evidence");
    assert!(!record.enforced);
    assert_eq!(record.status, "record_only");

    let weak = evidence(1.49, 1.3, 1_200, 1_000_000);
    let weak_record =
        evaluate_policy(&policy, &weak, &mixed).expect("a slow observation must remain reportable");
    assert_eq!(weak_record.status, "record_only");
}

/// Deterministic workload-shape drift still fails closed.
#[test]
fn performance_policy_rejects_workload_drift() {
    let policy = canonical_policy().expect("canonical policy");
    let strong = evidence(
        1.75,
        1.4,
        policy.reference_one_scheduler_actors_per_second,
        policy.reference_one_scheduler_p99_ns,
    );
    let mixed = mixed_tail_evidence(&policy);
    let mut drifted = strong.clone();
    drifted.iterations_per_actor += 1;
    let error = evaluate_policy(&policy, &drifted, &mixed).expect_err("workload drift must fail");
    assert!(error.contains("workload_drift"), "{error}");

    let mut mixed_drift = mixed;
    mixed_drift.samples_per_metric -= 1;
    let error = evaluate_policy(&policy, &strong, &mixed_drift)
        .expect_err("mixed workload drift must fail");
    assert!(error.contains("workload_drift"), "{error}");

    let mut mixed_drift = mixed_tail_evidence(&policy);
    mixed_drift.measurements[0].operations_per_sample += 1;
    let error = evaluate_policy(&policy, &strong, &mixed_drift)
        .expect_err("mixed operation drift must fail");
    assert!(error.contains("workload_drift"), "{error}");

    let mut mixed_drift = mixed_tail_evidence(&policy);
    mixed_drift.cpu_overlap_proven = false;
    let error =
        evaluate_policy(&policy, &strong, &mixed_drift).expect_err("missing CPU overlap must fail");
    assert!(error.contains("workload_drift"), "{error}");
}

/// Builds one synthetic CPU-bound evidence record.
fn evidence(
    speedup: f64,
    lower_bound: f64,
    one_throughput: u128,
    one_p99_ns: u128,
) -> CpuBoundActorEvidence {
    CpuBoundActorEvidence {
        export: "app.MulticoreBenchmark.cpu_bound",
        iterations_per_actor: CPU_ITERATIONS_PER_ACTOR,
        warmup_samples_per_width: super::WARMUP_SAMPLE_COUNT,
        widths: vec![
            width(1, one_throughput, one_p99_ns),
            width(2, (one_throughput as f64 * speedup) as u128, one_p99_ns),
        ],
        width_one_to_two: CpuBoundSpeedupConfidence {
            median_speedup_ratio: speedup,
            confidence_level: 0.95,
            lower_bound,
            upper_bound: speedup + 0.1,
            resamples: 4_096,
            seed: 1,
            method: "synthetic",
        },
    }
}

/// Builds one synthetic fixed-width CPU measurement.
fn width(requested_schedulers: usize, throughput: u128, p99_ns: u128) -> CpuBoundWidthMeasurement {
    CpuBoundWidthMeasurement {
        requested_schedulers,
        samples: CPU_SAMPLE_COUNT,
        actors_per_sample: requested_schedulers,
        median_actors_per_second: throughput,
        maximum_simultaneously_active_schedulers: requested_schedulers,
        distinct_scheduler_owner_threads: Vec::new(),
        sample_durations_ns: vec![p99_ns; CPU_SAMPLE_COUNT],
        timing: TimingDistribution {
            minimum_ns: p99_ns,
            median_ns: p99_ns,
            p95_ns: p99_ns,
            p99_ns,
            maximum_ns: p99_ns,
            median_absolute_deviation_ns: 0,
        },
    }
}

/// Builds canonical synthetic mixed-load evidence at every policy reference.
fn mixed_tail_evidence(policy: &super::policy::MulticorePerformancePolicy) -> MixedTailEvidence {
    let measurements = MIXED_TAIL_METRICS
        .iter()
        .copied()
        .zip(MIXED_TAIL_OPERATIONS)
        .zip(&policy.mixed_tail_budgets)
        .map(
            |((metric, operations_per_sample), budget)| MixedTailMeasurement {
                metric,
                execution_scope: "synthetic",
                samples: CPU_SAMPLE_COUNT,
                operations_per_sample,
                timing: TimingDistribution {
                    minimum_ns: 1,
                    median_ns: 1,
                    p95_ns: budget.reference_p95_ns,
                    p99_ns: budget.reference_p99_ns,
                    maximum_ns: budget.reference_p99_ns,
                    median_absolute_deviation_ns: 0,
                },
            },
        )
        .collect();
    MixedTailEvidence {
        requested_schedulers: MIXED_LOAD_SCHEDULERS,
        cpu_export: "app.MulticoreBenchmark.mixed_cpu_load",
        cpu_iterations_per_actor: CPU_ITERATIONS_PER_ACTOR * MIXED_CPU_PASSES,
        maximum_simultaneously_active_schedulers: MIXED_LOAD_SCHEDULERS,
        cpu_overlap_proven: true,
        samples_per_metric: CPU_SAMPLE_COUNT,
        warmup_samples_per_metric: super::WARMUP_SAMPLE_COUNT,
        measurements,
    }
}
