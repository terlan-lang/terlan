use super::*;

/// Builds a valid report for pure comparison tests.
fn report(lane: HttpExecutionLane) -> HttpPerformanceReport {
    fixture_report(lane)
}

/// Verifies percentile and throughput summaries retain ordered tails.
#[test]
fn timing_summary_records_tail_latency_and_throughput() {
    let timing = HttpTiming::from_durations(
        &[
            Duration::from_nanos(10),
            Duration::from_nanos(20),
            Duration::from_nanos(40),
        ],
        Duration::from_nanos(100),
    )
    .expect("timing");
    assert_eq!(timing.p50_ns, 20);
    assert_eq!(timing.p95_ns, 40);
    assert_eq!(timing.p99_ns, 40);
    assert_eq!(timing.throughput_requests_per_second, 30_000_000);
}

/// Verifies comparable complete reports produce a stable comparison.
#[test]
fn comparison_accepts_matching_complete_lane_reports() {
    let comparison = compare_reports(
        report(HttpExecutionLane::CheckedCoreir),
        report(HttpExecutionLane::NativeAot),
        "checked".to_string(),
        "native".to_string(),
    )
    .expect("comparison");
    assert_eq!(comparison.status, "completed");
    assert_eq!(comparison.hardware_fingerprint_sha256, "hardware");
}

/// Verifies mixed-machine benchmark reports cannot be compared.
#[test]
fn comparison_rejects_different_hardware_fingerprints() {
    let checked = report(HttpExecutionLane::CheckedCoreir);
    let mut native = report(HttpExecutionLane::NativeAot);
    native.hardware.sha256 = "different".to_string();
    let error = compare_reports(checked, native, "checked".to_string(), "native".to_string())
        .expect_err("mixed hardware must fail");
    assert!(error.contains("hardware fingerprints differ"));
}

/// Verifies incomplete lifecycle evidence fails before publication.
#[test]
fn comparison_rejects_incomplete_pressure_evidence() {
    let checked = report(HttpExecutionLane::CheckedCoreir);
    let mut native = report(HttpExecutionLane::NativeAot);
    native.pressure.completed_requests = 1;
    let error = compare_reports(checked, native, "checked".to_string(), "native".to_string())
        .expect_err("incomplete pressure must fail");
    assert!(error.contains("lifecycle evidence is incomplete"));
}
