use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{BinaryProtocolScenarioReport, BinaryProtocolTransportScenarioReport};

const SNAPSHOT_ENV: &str = "TERLAN_BENCH_BINARY_PROTOCOL_SNAPSHOT";
const UPDATE_SNAPSHOT_ENV: &str = "TERLAN_BENCH_BINARY_PROTOCOL_UPDATE_SNAPSHOT";
const SNAPSHOT_RELATIVE_PATH: &str = "benchmarks/baselines/vm-binary-protocol-contract.json";

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct ProtocolContractSnapshot {
    schema: String,
    report_schema: String,
    benchmark: String,
    warm_sample_count: usize,
    scale_points: Vec<usize>,
    source_scenarios: Vec<StableScenario>,
    transport_scenarios: Vec<StableScenario>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct StableScenario {
    id: String,
    workload: Option<String>,
    workload_class: String,
    measurement_scope: String,
    scale: usize,
    operation_count: usize,
    concurrency: usize,
    payload_bytes: Option<usize>,
    requests_per_connection: Option<usize>,
    connection_count: Option<usize>,
    expected_typed_failure_count: usize,
    comparison_status: String,
    correctness: String,
}

pub(super) fn validate(
    source: &[BinaryProtocolScenarioReport],
    transport: &[BinaryProtocolTransportScenarioReport],
) -> Result<(), String> {
    let path = snapshot_path();
    let actual = build(source, transport);
    if env::var_os(UPDATE_SNAPSHOT_ENV).is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create binary protocol snapshot directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        let rendered = serde_json::to_string_pretty(&actual)
            .map_err(|error| format!("failed to serialize binary protocol snapshot: {error}"))?;
        fs::write(&path, format!("{rendered}\n")).map_err(|error| {
            format!(
                "failed to write binary protocol contract snapshot `{}`: {error}",
                path.display()
            )
        })?;
        return Ok(());
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "failed to read binary protocol contract snapshot `{}`: {error}",
            path.display()
        )
    })?;
    let expected = serde_json::from_str::<ProtocolContractSnapshot>(&text).map_err(|error| {
        format!(
            "invalid binary protocol contract snapshot `{}`: {error}",
            path.display()
        )
    })?;
    if actual != expected {
        return Err(format!(
            "binary protocol contract drifted from checked-in snapshot `{}`",
            path.display()
        ));
    }
    Ok(())
}

fn build(
    source: &[BinaryProtocolScenarioReport],
    transport: &[BinaryProtocolTransportScenarioReport],
) -> ProtocolContractSnapshot {
    ProtocolContractSnapshot {
        schema: "terlan.vm-binary-protocol-contract-snapshot.v2".to_string(),
        report_schema: "terlan.vm-binary-protocol-benchmark.v8".to_string(),
        benchmark: "vm-binary-protocol".to_string(),
        warm_sample_count: 3,
        scale_points: vec![1, 10, 100, 1_000],
        source_scenarios: source.iter().map(source_scenario).collect(),
        transport_scenarios: transport.iter().map(transport_scenario).collect(),
    }
}

fn source_scenario(report: &BinaryProtocolScenarioReport) -> StableScenario {
    StableScenario {
        id: report.id.clone(),
        workload: None,
        workload_class: report.workload_class.to_string(),
        measurement_scope: "cold-compiler-process-plus-vm;warm-load-once-vm-loop".to_string(),
        scale: report.scale,
        operation_count: report.operation_count,
        concurrency: report.concurrency,
        payload_bytes: None,
        requests_per_connection: None,
        connection_count: None,
        expected_typed_failure_count: report.expected_typed_failure_count,
        comparison_status: report.comparison_status.to_string(),
        correctness: report.correctness.to_string(),
    }
}

fn transport_scenario(report: &BinaryProtocolTransportScenarioReport) -> StableScenario {
    StableScenario {
        id: report.id.clone(),
        workload: Some(report.workload.to_string()),
        workload_class: report.workload_class.to_string(),
        measurement_scope: report.measurement_scope.to_string(),
        scale: report.scale,
        operation_count: report.operation_count,
        concurrency: report.concurrency,
        payload_bytes: Some(report.payload_bytes),
        requests_per_connection: None,
        connection_count: None,
        expected_typed_failure_count: report.expected_typed_failure_count,
        comparison_status: report.comparison_status.to_string(),
        correctness: report.correctness.to_string(),
    }
}

fn snapshot_path() -> PathBuf {
    env::var_os(SNAPSHOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join(SNAPSHOT_RELATIVE_PATH)
        })
}

#[cfg(test)]
mod tests {
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
}
