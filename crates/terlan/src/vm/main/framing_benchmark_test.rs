use super::*;

#[test]
fn truncated_framing_benchmark_reports_expected_typed_failures() {
    let json = benchmark_in_memory_framing(3, 8, BenchmarkFramingWorkload::Truncated)
        .expect("truncated framing benchmark");
    let report = serde_json::from_str::<serde_json::Value>(&json).expect("benchmark JSON");
    assert_eq!(report["workload"], "truncated");
    assert_eq!(report["expected_typed_failure_count"], 3);
    assert_eq!(report["assertion"]["passed"], true);
    assert_eq!(
        report["measurement"]["name"],
        "vm_in_memory_length_prefixed_truncated_frame_rejection"
    );
}

#[test]
fn framing_workload_parser_rejects_unknown_names() {
    assert_eq!(
        BenchmarkFramingWorkload::parse("roundtrip"),
        Ok(BenchmarkFramingWorkload::Roundtrip)
    );
    assert_eq!(
        BenchmarkFramingWorkload::parse("truncated"),
        Ok(BenchmarkFramingWorkload::Truncated)
    );
    assert_eq!(
        BenchmarkFramingWorkload::parse("malformed-length"),
        Ok(BenchmarkFramingWorkload::MalformedLength)
    );
    assert_eq!(
        BenchmarkFramingWorkload::parse("invalid-utf8"),
        Ok(BenchmarkFramingWorkload::InvalidUtf8)
    );
    assert_eq!(
            BenchmarkFramingWorkload::parse("overflow").expect_err("unknown workload"),
            "terlan-vm benchmark-in-memory-framing --workload expects `roundtrip`, `truncated`, `malformed-length`, or `invalid-utf8`, got `overflow`"
        );
}

#[test]
fn adversarial_framing_matrix_reports_typed_failures() {
    for workload in [
        BenchmarkFramingWorkload::MalformedLength,
        BenchmarkFramingWorkload::InvalidUtf8,
    ] {
        let json =
            benchmark_in_memory_framing(3, 8, workload).expect("adversarial framing benchmark");
        let report = serde_json::from_str::<serde_json::Value>(&json).expect("benchmark JSON");
        assert_eq!(report["expected_typed_failure_count"], 3);
        assert_eq!(report["assertion"]["passed"], true);
    }
}

#[test]
fn framing_percentiles_use_nearest_rank_for_tail_samples() {
    let samples = [3, 7, 34];
    assert_eq!(percentile(&samples, 50), 7);
    assert_eq!(percentile(&samples, 95), 34);
    assert_eq!(percentile(&samples, 99), 34);
}
