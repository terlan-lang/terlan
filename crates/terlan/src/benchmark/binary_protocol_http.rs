use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{duration_micros, run_required_command, warm_statistics};

const BENCHMARK: &str = "vm-http-vm-stream-http1-vm-owned-async";
const MEASUREMENT: &str = "vm_http_vm_stream_http1_vm_owned_async";
const REQUEST_MIX: &str = "crud";

#[derive(Debug, Serialize)]
pub(super) struct HttpLifecycleScenarioReport {
    pub(super) id: String,
    pub(super) workload_class: &'static str,
    pub(super) measurement_scope: &'static str,
    protocol: &'static str,
    transport: &'static str,
    parser: &'static str,
    handler: &'static str,
    pub(super) request_mix: &'static str,
    pub(super) scale: usize,
    pub(super) operation_count: usize,
    pub(super) concurrency: usize,
    pub(super) payload_bytes: usize,
    pub(super) requests_per_connection: usize,
    pub(super) connection_count: usize,
    cold_measurement_us: u64,
    warm_measurement_samples_us: Vec<u64>,
    warm_mean_measurement_us: u64,
    warm_median_measurement_us: u64,
    warm_p95_measurement_us: u64,
    warm_p99_measurement_us: u64,
    warm_median_operations_per_second: f64,
    unexpected_error_count: usize,
    unexpected_error_rate_percent: f64,
    pub(super) comparison_status: &'static str,
    winner: &'static str,
    relative_delta_percent: Option<f64>,
    pub(super) correctness: &'static str,
}

#[derive(Debug, Deserialize)]
struct VmHttpBenchmarkOutput {
    benchmark: String,
    status: String,
    runtime_stack: VmHttpRuntimeStack,
    iterations: usize,
    payload_bytes: usize,
    requests_per_connection: usize,
    connection_count: usize,
    request_mix: String,
    server_state: VmHttpServerState,
    replay_determinism: VmHttpReplayEvidence,
    measurement: VmHttpMeasurement,
    assertion: VmHttpAssertion,
}

#[derive(Debug, Deserialize)]
struct VmHttpRuntimeStack {
    transport: String,
    server: String,
    protocol_parser: String,
    handler: String,
    host_socket_runtime: String,
    host_async_runtime: String,
}

#[derive(Debug, Deserialize)]
struct VmHttpServerState {
    listener_queued_accepts: usize,
    listener_waiting_acceptors: usize,
    accepted_total: usize,
    completed_total: usize,
    active_handlers: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VmHttpReplayEvidence {
    schema: String,
    execution_validated: bool,
    request_count: usize,
}

#[derive(Debug, Deserialize)]
struct VmHttpMeasurement {
    name: String,
    total_us: u64,
}

#[derive(Debug, Deserialize)]
struct VmHttpAssertion {
    name: String,
    passed: bool,
}

pub(super) fn run(
    vm_binary: &Path,
    scales: &[usize],
    sample_count: usize,
    payload_bytes: usize,
) -> Result<Vec<HttpLifecycleScenarioReport>, String> {
    let mut reports = Vec::with_capacity(scales.len());
    for &scale in scales {
        let requests_per_connection = 1;
        let cold = run_once(vm_binary, scale, payload_bytes, requests_per_connection)?;
        let mut warm_samples = Vec::with_capacity(sample_count);
        for _ in 0..sample_count {
            warm_samples.push(run_once(
                vm_binary,
                scale,
                payload_bytes,
                requests_per_connection,
            )?);
        }
        reports.push(scenario_report(
            scale,
            payload_bytes,
            requests_per_connection,
            cold,
            warm_samples,
        ));
    }
    Ok(reports)
}

fn run_once(
    vm_binary: &Path,
    iterations: usize,
    payload_bytes: usize,
    requests_per_connection: usize,
) -> Result<Duration, String> {
    let iterations_arg = iterations.to_string();
    let payload_bytes_arg = payload_bytes.to_string();
    let requests_per_connection_arg = requests_per_connection.to_string();
    let output = run_required_command(
        vm_binary,
        &[
            "benchmark-http-vm-stream",
            "--iterations",
            &iterations_arg,
            "--payload-bytes",
            &payload_bytes_arg,
            "--requests-per-connection",
            &requests_per_connection_arg,
            "--request-mix",
            REQUEST_MIX,
        ],
    )?;
    parse_measurement(
        &output.stdout,
        iterations,
        payload_bytes,
        requests_per_connection,
    )
}

fn parse_measurement(
    stdout: &str,
    expected_iterations: usize,
    expected_payload_bytes: usize,
    expected_requests_per_connection: usize,
) -> Result<Duration, String> {
    let report = serde_json::from_str::<VmHttpBenchmarkOutput>(stdout)
        .map_err(|error| format!("invalid VM HTTP lifecycle benchmark JSON: {error}"))?;
    validate_identity(&report)?;
    validate_dimensions(
        &report,
        expected_iterations,
        expected_payload_bytes,
        expected_requests_per_connection,
    )?;
    validate_runtime_stack(&report.runtime_stack)?;
    validate_completion(&report)?;
    Ok(Duration::from_micros(report.measurement.total_us))
}

fn validate_identity(report: &VmHttpBenchmarkOutput) -> Result<(), String> {
    if report.benchmark != BENCHMARK
        || report.status != "completed"
        || report.request_mix != REQUEST_MIX
        || report.measurement.name != MEASUREMENT
        || report.assertion.name != MEASUREMENT
    {
        return Err(format!(
            "unexpected VM HTTP lifecycle benchmark contract: benchmark=`{}`, status=`{}`, mix=`{}`, measurement=`{}`, assertion=`{}`",
            report.benchmark,
            report.status,
            report.request_mix,
            report.measurement.name,
            report.assertion.name
        ));
    }
    Ok(())
}

fn validate_dimensions(
    report: &VmHttpBenchmarkOutput,
    iterations: usize,
    payload_bytes: usize,
    requests_per_connection: usize,
) -> Result<(), String> {
    let connection_count = iterations / requests_per_connection;
    if report.iterations != iterations
        || report.payload_bytes != payload_bytes
        || report.requests_per_connection != requests_per_connection
        || report.connection_count != connection_count
    {
        return Err(format!(
            "VM HTTP lifecycle benchmark dimensions changed: expected iterations={iterations}, payload_bytes={payload_bytes}, requests_per_connection={requests_per_connection}, connection_count={connection_count}; got iterations={}, payload_bytes={}, requests_per_connection={}, connection_count={}",
            report.iterations,
            report.payload_bytes,
            report.requests_per_connection,
            report.connection_count
        ));
    }
    Ok(())
}

fn validate_runtime_stack(stack: &VmHttpRuntimeStack) -> Result<(), String> {
    if stack.transport != "VmTcpRuntime logical streams"
        || stack.server != "VmHttpTcpServer"
        || stack.protocol_parser != "httparse HTTP/1 request parser"
        || stack.handler != "TerlanVm in-process evaluator"
        || stack.host_socket_runtime != "absent from measured path"
        || stack.host_async_runtime != "absent from measured path"
    {
        return Err("VM HTTP lifecycle runtime stack changed".to_string());
    }
    Ok(())
}

fn validate_completion(report: &VmHttpBenchmarkOutput) -> Result<(), String> {
    let server = &report.server_state;
    let replay = &report.replay_determinism;
    if server.accepted_total != report.connection_count
        || server.completed_total != report.iterations
        || server.active_handlers != 0
        || server.listener_queued_accepts != 0
        || server.listener_waiting_acceptors != 0
    {
        return Err("VM HTTP lifecycle server accounting is incomplete".to_string());
    }
    if replay.schema != "terlan-vm-http-replay-v1"
        || !replay.execution_validated
        || replay.request_count != report.iterations
    {
        return Err("VM HTTP lifecycle replay evidence is invalid".to_string());
    }
    if !report.assertion.passed {
        return Err("VM HTTP lifecycle correctness assertion failed".to_string());
    }
    Ok(())
}

fn scenario_report(
    scale: usize,
    payload_bytes: usize,
    requests_per_connection: usize,
    cold: Duration,
    warm_samples: Vec<Duration>,
) -> HttpLifecycleScenarioReport {
    let statistics = warm_statistics(warm_samples, scale);
    HttpLifecycleScenarioReport {
        id: format!("vm-http-lifecycle-crud-{scale}"),
        workload_class: "success",
        measurement_scope: "vm-owned-in-memory-tcp-http-lifecycle",
        protocol: "HTTP/1.1",
        transport: "VmTcpRuntime logical streams",
        parser: "httparse",
        handler: "TerlanVm in-process evaluator",
        request_mix: REQUEST_MIX,
        scale,
        operation_count: scale,
        concurrency: scale / requests_per_connection,
        payload_bytes,
        requests_per_connection,
        connection_count: scale / requests_per_connection,
        cold_measurement_us: duration_micros(cold),
        warm_mean_measurement_us: statistics.mean_us,
        warm_median_measurement_us: statistics.median_us,
        warm_p95_measurement_us: statistics.p95_us,
        warm_p99_measurement_us: statistics.p99_us,
        warm_measurement_samples_us: statistics.samples_us,
        warm_median_operations_per_second: statistics.median_operations_per_second,
        unexpected_error_count: 0,
        unexpected_error_rate_percent: 0.0,
        comparison_status: "unsupported-no-equivalent-baseline",
        winner: "not-comparable",
        relative_delta_percent: None,
        correctness: "validated-every-request-response",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_report() -> String {
        r#"{
            "benchmark":"vm-http-vm-stream-http1-vm-owned-async",
            "status":"completed",
            "runtime_stack":{
                "transport":"VmTcpRuntime logical streams",
                "server":"VmHttpTcpServer",
                "protocol_parser":"httparse HTTP/1 request parser",
                "handler":"TerlanVm in-process evaluator",
                "host_socket_runtime":"absent from measured path",
                "host_async_runtime":"absent from measured path"
            },
            "iterations":10,
            "payload_bytes":128,
            "requests_per_connection":10,
            "connection_count":1,
            "request_mix":"crud",
            "server_state":{
                "listener_queued_accepts":0,
                "listener_waiting_acceptors":0,
                "accepted_total":1,
                "completed_total":10,
                "active_handlers":0
            },
            "replay_determinism":{
                "schema":"terlan-vm-http-replay-v1",
                "executionValidated":true,
                "requestCount":10
            },
            "measurement":{"name":"vm_http_vm_stream_http1_vm_owned_async","total_us":42},
            "assertion":{"name":"vm_http_vm_stream_http1_vm_owned_async","passed":true}
        }"#
        .to_string()
    }

    #[test]
    fn parser_accepts_exact_vm_http_lifecycle_contract() {
        assert_eq!(
            parse_measurement(&valid_report(), 10, 128, 10).expect("valid report"),
            Duration::from_micros(42)
        );
    }

    #[test]
    fn parser_rejects_contract_dimension_and_runtime_drift() {
        let identity = valid_report().replace(
            "vm-http-vm-stream-http1-vm-owned-async",
            "different-benchmark",
        );
        assert!(parse_measurement(&identity, 10, 128, 10)
            .expect_err("identity drift must fail")
            .contains("unexpected VM HTTP lifecycle benchmark contract"));

        let dimensions = valid_report().replace("\"iterations\":10", "\"iterations\":9");
        assert!(parse_measurement(&dimensions, 10, 128, 10)
            .expect_err("dimension drift must fail")
            .contains("dimensions changed"));

        let runtime =
            valid_report().replace("absent from measured path", "present in measured path");
        assert_eq!(
            parse_measurement(&runtime, 10, 128, 10).expect_err("runtime drift must fail"),
            "VM HTTP lifecycle runtime stack changed"
        );
    }

    #[test]
    fn parser_rejects_incomplete_execution_evidence() {
        let accounting = valid_report().replace("\"completed_total\":10", "\"completed_total\":9");
        assert_eq!(
            parse_measurement(&accounting, 10, 128, 10)
                .expect_err("incomplete server accounting must fail"),
            "VM HTTP lifecycle server accounting is incomplete"
        );

        let replay = valid_report().replace(
            "\"executionValidated\":true",
            "\"executionValidated\":false",
        );
        assert_eq!(
            parse_measurement(&replay, 10, 128, 10).expect_err("invalid replay evidence must fail"),
            "VM HTTP lifecycle replay evidence is invalid"
        );

        let assertion = valid_report().replace("\"passed\":true", "\"passed\":false");
        assert_eq!(
            parse_measurement(&assertion, 10, 128, 10).expect_err("failed assertion must fail"),
            "VM HTTP lifecycle correctness assertion failed"
        );

        assert!(parse_measurement("not-json", 10, 128, 10)
            .expect_err("malformed JSON must fail")
            .contains("invalid VM HTTP lifecycle benchmark JSON"));
    }

    #[test]
    fn report_tracks_keep_alive_dimensions_and_warm_statistics() {
        let report = scenario_report(
            100,
            128,
            10,
            Duration::from_micros(80),
            vec![
                Duration::from_micros(60),
                Duration::from_micros(40),
                Duration::from_micros(50),
            ],
        );
        assert_eq!(report.id, "vm-http-lifecycle-crud-100");
        assert_eq!(report.connection_count, 10);
        assert_eq!(report.requests_per_connection, 10);
        assert_eq!(report.cold_measurement_us, 80);
        assert_eq!(report.warm_measurement_samples_us, vec![60, 40, 50]);
        assert_eq!(report.warm_mean_measurement_us, 50);
        assert_eq!(report.warm_median_measurement_us, 50);
        assert_eq!(report.warm_p95_measurement_us, 60);
        assert_eq!(report.warm_p99_measurement_us, 60);
        assert_eq!(report.warm_median_operations_per_second, 2_000_000.0);
        assert_eq!(report.correctness, "validated-every-request-response");
    }
}
