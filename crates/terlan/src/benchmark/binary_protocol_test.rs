use std::time::Duration;

use super::*;

#[test]
fn percentile_selects_nearest_rank_without_mutating_input() {
    let samples = [30, 10, 20];
    assert_eq!(percentile(&samples, 50), 20);
    assert_eq!(percentile(&samples, 95), 30);
    assert_eq!(percentile(&samples, 99), 30);
    assert_eq!(samples, [30, 10, 20]);
}

#[test]
fn warm_source_samples_remove_one_process_overhead_and_split_load_once_loop() {
    let samples =
        steady_state_warm_samples(Duration::from_micros(130), Duration::from_micros(100), 3);
    assert_eq!(samples, vec![Duration::from_micros(10); 3]);
    let floor = steady_state_warm_samples(Duration::from_micros(90), Duration::from_micros(100), 3);
    assert_eq!(floor, vec![Duration::from_micros(1); 3]);
}

#[test]
fn scenario_report_separates_cold_warm_tail_and_adversarial_counts() {
    let report = scenario_report(
        WORKLOADS[2],
        10,
        "adversarial_invalid_width_protocol_benchmark_10".to_string(),
        Duration::from_micros(40),
        vec![
            Duration::from_micros(30),
            Duration::from_micros(10),
            Duration::from_micros(20),
        ],
    );
    assert_eq!(report.cold_end_to_end_us, 40);
    assert_eq!(report.warm_end_to_end_samples_us, vec![30, 10, 20]);
    assert_eq!(report.warm_mean_end_to_end_us, 20);
    assert_eq!(report.warm_median_end_to_end_us, 20);
    assert_eq!(report.warm_p95_end_to_end_us, 30);
    assert_eq!(report.warm_p99_end_to_end_us, 30);
    assert_eq!(report.expected_typed_failure_count, 10);
    assert_eq!(report.unexpected_error_count, 0);
    assert_eq!(
        report.comparison_status,
        "unsupported-no-equivalent-baseline"
    );
    assert_eq!(report.winner, "not-comparable");
    assert_eq!(report.relative_delta_percent, None);
    assert_eq!(report.correctness, "validated-every-frame");
    assert_eq!(report.warm_median_operations_per_second, 500_000.0);
}

#[test]
fn transport_scenario_report_tracks_vm_framing_latency_and_throughput() {
    let report = transport_scenario_report(
        FRAMING_WORKLOADS[0],
        100,
        128,
        Duration::from_micros(80),
        vec![
            Duration::from_micros(60),
            Duration::from_micros(40),
            Duration::from_micros(50),
        ],
    );
    assert_eq!(report.id, "vm_tcp_length_prefixed_framing-100");
    assert_eq!(report.workload, "roundtrip");
    assert_eq!(report.operation_count, 100);
    assert_eq!(report.payload_bytes, 128);
    assert_eq!(report.cold_measurement_us, 80);
    assert_eq!(report.warm_measurement_samples_us, vec![60, 40, 50]);
    assert_eq!(report.warm_mean_measurement_us, 50);
    assert_eq!(report.warm_median_measurement_us, 50);
    assert_eq!(report.warm_p95_measurement_us, 60);
    assert_eq!(report.warm_p99_measurement_us, 60);
    assert_eq!(report.warm_median_operations_per_second, 2_000_000.0);
    assert_eq!(report.expected_typed_failure_count, 0);
    assert_eq!(report.correctness, "validated-every-frame");
}

#[test]
fn transport_scenario_report_counts_adversarial_framing_failures() {
    let report = transport_scenario_report(
        FRAMING_WORKLOADS[1],
        10,
        128,
        Duration::from_micros(80),
        vec![Duration::from_micros(30); 3],
    );
    assert_eq!(report.id, "vm_tcp_truncated_framing-10");
    assert_eq!(report.workload, "truncated");
    assert_eq!(report.workload_class, "adversarial");
    assert_eq!(report.expected_typed_failure_count, 10);
    assert_eq!(report.unexpected_error_count, 0);
    assert_eq!(report.correctness, "validated-every-typed-failure");
}

#[test]
fn framing_measurement_requires_exact_dimensions_and_correctness() {
    let valid = r#"{
        "benchmark": "vm-in-memory-length-prefixed-framing",
        "status": "completed",
        "workload": "roundtrip",
        "iterations": 10,
        "payload_bytes": 128,
        "expected_typed_failure_count": 0,
        "measurement": {
            "name": "vm_in_memory_length_prefixed_frame_roundtrip",
            "total_us": 42
        },
        "assertion": {"passed": true}
    }"#;
    assert_eq!(
        parse_framing_measurement(valid, FRAMING_WORKLOADS[0], 10, 128)
            .expect("valid framing report"),
        Duration::from_micros(42)
    );

    let wrong_dimensions = valid.replace("\"iterations\": 10", "\"iterations\": 9");
    assert!(
        parse_framing_measurement(&wrong_dimensions, FRAMING_WORKLOADS[0], 10, 128)
            .expect_err("wrong dimensions must fail")
            .contains("dimensions changed")
    );

    let failed_assertion = valid.replace("\"passed\": true", "\"passed\": false");
    assert_eq!(
        parse_framing_measurement(&failed_assertion, FRAMING_WORKLOADS[0], 10, 128)
            .expect_err("failed correctness assertion must fail"),
        "VM framing benchmark correctness assertion failed for workload `roundtrip`"
    );

    let wrong_failure_count = valid.replace(
        "\"expected_typed_failure_count\": 0",
        "\"expected_typed_failure_count\": 1",
    );
    assert!(
        parse_framing_measurement(&wrong_failure_count, FRAMING_WORKLOADS[0], 10, 128)
            .expect_err("wrong typed failure count must fail")
            .contains("typed failure count changed")
    );

    assert!(
        parse_framing_measurement("not-json", FRAMING_WORKLOADS[0], 10, 128)
            .expect_err("malformed JSON must fail")
            .contains("invalid VM framing benchmark JSON")
    );
}

#[test]
fn fixture_declares_every_workload_at_every_scale_point() {
    let source = std::fs::read_to_string(fixture_path()).expect("read benchmark fixture");
    for workload in WORKLOADS {
        for scale in SCALE_POINTS {
            let anchor = format!("pub {}_{}(): Bool", workload.test_prefix, scale);
            assert!(
                source.contains(&anchor),
                "missing fixture anchor `{anchor}`"
            );
        }
    }
}
