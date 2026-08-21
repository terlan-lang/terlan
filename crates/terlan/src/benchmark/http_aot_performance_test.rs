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

/// Verifies noisy outlier rounds cannot become the policy aggregate.
#[test]
fn repeated_measurement_selects_one_median_throughput_round() {
    let timing = |throughput| HttpTiming {
        sample_count: 1,
        total_wall_ns: 1,
        throughput_requests_per_second: throughput,
        min_ns: throughput,
        mean_ns: throughput,
        p50_ns: throughput,
        p95_ns: throughput,
        p99_ns: throughput,
        max_ns: throughput,
    };
    let rounds = vec![
        timing(10),
        timing(1_000),
        timing(20),
        timing(30),
        timing(40),
    ];
    assert_eq!(
        median_throughput_round(&rounds)
            .expect("median round")
            .throughput_requests_per_second,
        30
    );
}

/// Verifies the generated benchmark package explicitly owns its `app`
/// namespace so strict project-layout validation accepts the fixture.
#[test]
fn generated_http_package_declares_its_source_namespace() {
    let workspace = create_workspace().expect("workspace");
    let result = write_package(&workspace, "generation-one", 16);
    let manifest = fs::read_to_string(workspace.join("terlan.toml")).expect("manifest");
    let _ = fs::remove_dir_all(&workspace);

    result.expect("package");
    assert!(manifest.contains("namespace = \"app\""));
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

/// Verifies the retired checked-CoreIR v1 capture remains comparable without
/// weakening the native lane's v2 report contract.
#[test]
fn legacy_checked_coreir_report_adapts_only_recorded_measurements() {
    let mut value = serde_json::to_value(report(HttpExecutionLane::CheckedCoreir)).expect("report");
    value["schema"] = serde_json::json!(LEGACY_CHECKED_COREIR_SCHEMA);
    value.as_object_mut().expect("object").remove("measurement");
    let workload = value["workload"].as_object_mut().expect("workload");
    workload.remove("warmup_requests");
    workload.remove("measurement_rounds");
    workload.remove("readiness_reactors");
    let bytes = serde_json::to_vec(&value).expect("legacy bytes");
    let adapted = report_io::parse_report(Path::new("checked-coreir-v1.json"), &bytes)
        .expect("adapt legacy reference");
    assert_eq!(adapted.workload.measurement_rounds, 1);
    assert_eq!(adapted.measurement.sequential_rounds.len(), 1);
    assert_eq!(
        adapted.measurement.sequential_rounds[0].p50_ns,
        adapted.sequential.p50_ns
    );
}
