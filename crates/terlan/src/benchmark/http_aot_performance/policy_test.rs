//! Tests for the versioned HTTP AOT performance budget.

use super::*;
use crate::benchmark::http_aot_performance::{fixture_report, HttpExecutionLane};

#[test]
fn canonical_policy_accepts_equal_complete_reports() {
    let policy = canonical_policy().expect("canonical policy");
    validate_performance(
        &fixture_report(HttpExecutionLane::CheckedCoreir),
        &fixture_report(HttpExecutionLane::NativeAot),
        &policy,
    )
    .expect("equal reports fit budget");
}

#[test]
fn performance_policy_rejects_every_over_budget_dimension() {
    let policy = canonical_policy().expect("canonical policy");
    let checked = fixture_report(HttpExecutionLane::CheckedCoreir);

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.sequential.p50_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("sequential_p50"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.sequential.p95_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("sequential_p95"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.sequential.p99_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("sequential_p99"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.sequential.throughput_requests_per_second /= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("sequential_throughput"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.pressure.timing.p50_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("pressure_p50"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.pressure.timing.p95_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("pressure_p95"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.pressure.timing.p99_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("pressure_p99"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.pressure.timing.throughput_requests_per_second /= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("pressure_throughput"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.longevity.timing.p50_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("longevity_p50"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.longevity.timing.p95_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("longevity_p95"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.longevity.timing.p99_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("longevity_p99"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.longevity.timing.throughput_requests_per_second /= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("longevity_throughput"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.allocation.peak_observed_bytes = Some(3);
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("peak_rss"));

    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.generation_overlap.reload_latency_ns *= 2;
    assert!(validate_performance(&checked, &native, &policy)
        .unwrap_err()
        .contains("generation_reload"));
}

#[test]
fn performance_policy_cannot_be_weakened_past_hard_ceilings() {
    let mut policy = canonical_policy().expect("canonical policy");
    policy.maximum_sequential_p99_ratio = 1.51;
    assert!(validate_policy(&policy)
        .unwrap_err()
        .contains("policy_weakened"));

    let mut policy = canonical_policy().expect("canonical policy");
    policy.minimum_sequential_throughput_ratio = 0.69;
    assert!(validate_policy(&policy)
        .unwrap_err()
        .contains("policy_weakened"));
}
