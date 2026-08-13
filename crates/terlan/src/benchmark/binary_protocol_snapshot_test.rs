use super::*;

#[test]
fn checked_in_snapshot_has_exact_schema_and_dimensions() {
    let path = snapshot_path();
    let text = fs::read_to_string(&path).expect("read checked-in contract snapshot");
    let snapshot = serde_json::from_str::<ProtocolContractSnapshot>(&text)
        .expect("parse checked-in contract snapshot");
    assert_eq!(
        snapshot.schema,
        "terlan.vm-binary-protocol-contract-snapshot.v2"
    );
    assert_eq!(
        snapshot.report_schema,
        "terlan.vm-binary-protocol-benchmark.v8"
    );
    assert_eq!(snapshot.scale_points, [1, 10, 100, 1_000]);
    assert_eq!(snapshot.source_scenarios.len(), 20);
    assert_eq!(snapshot.transport_scenarios.len(), 16);
}

#[test]
fn stable_scenario_rejects_dimension_and_comparison_drift() {
    let left = StableScenario {
        id: "frame-10".to_string(),
        workload: Some("roundtrip".to_string()),
        workload_class: "success".to_string(),
        measurement_scope: "vm-owned-in-memory-tcp-framing".to_string(),
        scale: 10,
        operation_count: 10,
        concurrency: 1,
        payload_bytes: Some(128),
        requests_per_connection: None,
        connection_count: None,
        expected_typed_failure_count: 0,
        comparison_status: "unsupported-no-equivalent-baseline".to_string(),
        correctness: "validated-every-frame".to_string(),
    };
    let mut changed = StableScenario {
        operation_count: 9,
        ..left.clone()
    };
    assert_ne!(left, changed);
    changed.operation_count = 10;
    changed.comparison_status = "comparable".to_string();
    assert_ne!(left, changed);
}
