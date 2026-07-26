//! Shared provenance and independent protocol validation for HTTP benchmarks.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[path = "http_benchmark_support/metadata.rs"]
mod metadata;

use metadata::{command_line, cpu_governor, digest, rustflag_value};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct BenchmarkExecutionMetadata {
    pub(crate) schema: String,
    pub(crate) server_binary_sha256: String,
    pub(crate) server_binary_bytes: u64,
    pub(crate) server_cpu_list: String,
    pub(crate) client_cpu_list: String,
    pub(crate) reactor_count: usize,
    pub(crate) kernel: String,
    pub(crate) cpu_governor: String,
    pub(crate) rustflags: String,
    pub(crate) allocator: String,
    pub(crate) protocol_validator: String,
    pub(crate) load_generator: String,
    pub(crate) performance_counter_tool: String,
    pub(crate) performance_event_policy: String,
    pub(crate) host: String,
    pub(crate) numa_nodes: String,
    pub(crate) recorded_unix_seconds: u64,
    pub(crate) build_profile: String,
    pub(crate) target_cpu: String,
    pub(crate) lto: String,
    pub(crate) codegen_units: String,
    pub(crate) panic_strategy: String,
    pub(crate) cargo_lock_sha256: String,
    pub(crate) socket_policy: String,
}

impl BenchmarkExecutionMetadata {
    pub(crate) fn capture(binary: &Path, reactor_count: usize) -> Result<Self, String> {
        let bytes = fs::read(binary).map_err(|error| {
            format!(
                "cannot read benchmark server `{}`: {error}",
                binary.display()
            )
        })?;
        Ok(Self {
            schema: "terlan-http-benchmark-execution-v1".to_string(),
            server_binary_sha256: digest(&bytes),
            server_binary_bytes: bytes.len() as u64,
            server_cpu_list: env::var("TERLAN_BENCH_HTTP_CPU_LIST")
                .unwrap_or_else(|_| "unrestricted".to_string()),
            client_cpu_list: env::var("TERLAN_BENCH_HTTP_CLIENT_CPU_LIST")
                .unwrap_or_else(|_| "inherited".to_string()),
            reactor_count,
            kernel: command_line("uname", &["-sr"]),
            cpu_governor: cpu_governor(),
            rustflags: env::var("RUSTFLAGS").unwrap_or_else(|_| "default".to_string()),
            allocator: env::var("TERLAN_BENCH_ALLOCATOR").unwrap_or_else(|_| "system".to_string()),
            protocol_validator: command_line("curl", &["--version"]),
            load_generator: command_line("wrk", &["-v"]),
            performance_counter_tool: command_line("perf", &["--version"]),
            performance_event_policy: fs::read_to_string("/proc/sys/kernel/perf_event_paranoid")
                .map(|value| value.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            host: command_line("hostname", &[]),
            numa_nodes: fs::read_to_string("/sys/devices/system/node/possible")
                .map(|value| value.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            recorded_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            build_profile: env::var("TERLAN_BENCH_BUILD_PROFILE")
                .unwrap_or_else(|_| "release".to_string()),
            target_cpu: env::var("TERLAN_BENCH_TARGET_CPU")
                .unwrap_or_else(|_| rustflag_value("target-cpu").unwrap_or("default".to_string())),
            lto: env::var("TERLAN_BENCH_LTO").unwrap_or_else(|_| "false".to_string()),
            codegen_units: env::var("TERLAN_BENCH_CODEGEN_UNITS")
                .unwrap_or_else(|_| "16-release-default".to_string()),
            panic_strategy: env::var("TERLAN_BENCH_PANIC").unwrap_or_else(|_| "unwind".to_string()),
            cargo_lock_sha256: fs::read("Cargo.lock")
                .map(|bytes| digest(&bytes))
                .unwrap_or_else(|_| "unavailable".to_string()),
            socket_policy: env::var("TERLAN_BENCH_HTTP_SOCKET_POLICY")
                .unwrap_or_else(|_| "framework-defaults; client-tcp-nodelay".to_string()),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProtocolValidationEvidence {
    pub(crate) validator: String,
    pub(crate) protocol: String,
    pub(crate) status: String,
    pub(crate) response_body_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProtocolScenarioEvidence {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LifecycleEvidence {
    pub(crate) status: String,
    pub(crate) scenarios: Vec<LifecycleScenarioEvidence>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LifecycleScenarioEvidence {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) detail: String,
}

pub(crate) fn validate_with_curl(
    port: u16,
    payload_bytes: usize,
    generation: &str,
) -> Result<ProtocolValidationEvidence, String> {
    let payload = "x".repeat(payload_bytes);
    let expected = format!("{generation}:{payload}");
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--http1.1",
            "--request",
            "POST",
            "--header",
            "content-type: text/plain",
            "--data-binary",
            &payload,
            &format!("http://127.0.0.1:{port}/api/bench"),
        ])
        .output()
        .map_err(|error| format!("cannot run maintained curl validator: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl protocol validation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout != expected.as_bytes() {
        return Err("curl protocol validation returned an unexpected response body".to_string());
    }
    Ok(ProtocolValidationEvidence {
        validator: command_line("curl", &["--version"]),
        protocol: "HTTP/1.1".to_string(),
        status: "validated".to_string(),
        response_body_sha256: digest(&output.stdout),
    })
}

pub(crate) fn protocol_scenarios(port: u16) -> Result<Vec<ProtocolScenarioEvidence>, String> {
    let status = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
            "--http1.1",
            &format!("http://127.0.0.1:{port}/missing-benchmark-route"),
        ])
        .output()
        .map_err(|error| format!("cannot run curl error-response validation: {error}"))?;
    if !status.status.success() || status.stdout != b"404" {
        return Err(format!(
            "HTTP error-response validation expected 404, got `{}`",
            String::from_utf8_lossy(&status.stdout)
        ));
    }
    let mut scenarios = vec![
        ProtocolScenarioEvidence {
            name: "http-1.1".to_string(),
            status: "validated".to_string(),
            detail: "maintained curl client validated request, response, and body".to_string(),
        },
        ProtocolScenarioEvidence {
            name: "error-response-404".to_string(),
            status: "validated".to_string(),
            detail: "maintained curl client validated the missing-route response".to_string(),
        },
    ];
    if let Ok(url) = env::var("TERLAN_BENCH_HTTP_TLS_URL") {
        let output = Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--insecure",
                "--http2",
                "--output",
                "/dev/null",
                "--write-out",
                "%{http_version}",
                &url,
            ])
            .output()
            .map_err(|error| format!("cannot run TLS/HTTP2 validator: {error}"))?;
        let validated = output.status.success() && output.stdout.starts_with(b"2");
        scenarios.push(ProtocolScenarioEvidence {
            name: "tls".to_string(),
            status: if output.status.success() {
                "validated".to_string()
            } else {
                "failed".to_string()
            },
            detail: format!("maintained curl TLS probe against {url}"),
        });
        scenarios.push(ProtocolScenarioEvidence {
            name: "http-2".to_string(),
            status: if validated {
                "validated".to_string()
            } else {
                "failed".to_string()
            },
            detail: format!(
                "curl negotiated HTTP version `{}`",
                String::from_utf8_lossy(&output.stdout)
            ),
        });
    } else {
        scenarios.extend([
            ProtocolScenarioEvidence {
                name: "tls".to_string(),
                status: "not-configured".to_string(),
                detail: "set TERLAN_BENCH_HTTP_TLS_URL to validate a shared TLS endpoint"
                    .to_string(),
            },
            ProtocolScenarioEvidence {
                name: "http-2".to_string(),
                status: "not-configured".to_string(),
                detail: "TLS/HTTP2 is a separate protocol lane, never inferred from HTTP/1.1"
                    .to_string(),
            },
        ]);
    }
    Ok(scenarios)
}

pub(crate) fn lifecycle_scenarios(
    port: u16,
    payload_bytes: usize,
    generation: &str,
    maintained: &[MaintainedWorkloadEvidence],
) -> Result<LifecycleEvidence, String> {
    let invalid = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--output",
            "-",
            "--write-out",
            "\n%{http_code}",
            "--request",
            "POST",
            "--header",
            "content-type: application/json",
            "--data-binary",
            "{invalid",
            &format!("http://127.0.0.1:{port}/api/json"),
        ])
        .output()
        .map_err(|error| format!("cannot validate invalid JSON lifecycle: {error}"))?;
    let invalid_body = String::from_utf8_lossy(&invalid.stdout);
    if !invalid.status.success() || invalid_body.trim() != "invalid-json\n400" {
        return Err(format!(
            "invalid JSON lifecycle expected invalid-json/400, got `{}`",
            invalid_body.trim()
        ));
    }
    let upload = env::temp_dir().join(format!("terlan-aborted-upload-{}", std::process::id()));
    fs::write(&upload, vec![b'x'; payload_bytes.max(256 * 1024)])
        .map_err(|error| format!("cannot prepare aborted upload: {error}"))?;
    let aborted = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "0.05",
            "--limit-rate",
            "1k",
            "--request",
            "POST",
            "--data-binary",
            &format!("@{}", upload.display()),
            &format!("http://127.0.0.1:{port}/api/bench"),
        ])
        .output();
    let _ = fs::remove_file(upload);
    let cancellation_observed = aborted
        .as_ref()
        .map(|output| !output.status.success())
        .unwrap_or(false);
    validate_with_curl(port, payload_bytes, generation)?;
    let oversubscribed = maintained
        .iter()
        .find(|workload| workload.name == "maintained-oversubscribed");
    let overload_status = oversubscribed.map_or("not-measured", |workload| {
        if workload.transport_errors == 0 && workload.validation_errors == 0 {
            "validated"
        } else {
            "failed"
        }
    });
    Ok(LifecycleEvidence {
        status: if cancellation_observed && overload_status != "failed" {
            "validated".to_string()
        } else {
            "partial".to_string()
        },
        scenarios: vec![
            LifecycleScenarioEvidence {
                name: "malformed-request".to_string(),
                status: "validated".to_string(),
                detail: "invalid JSON produced an explicit 400 response".to_string(),
            },
            LifecycleScenarioEvidence {
                name: "cancelled-slow-upload".to_string(),
                status: if cancellation_observed {
                    "validated".to_string()
                } else {
                    "not-observed".to_string()
                },
                detail: "curl aborted a rate-limited body; a subsequent request succeeded"
                    .to_string(),
            },
            LifecycleScenarioEvidence {
                name: "oversubscribed-overload".to_string(),
                status: overload_status.to_string(),
                detail: oversubscribed
                    .map(|workload| {
                        format!(
                            "{} requests completed at {:.1} req/s without corruption",
                            workload.completed_requests, workload.requests_per_second
                        )
                    })
                    .unwrap_or_else(|| {
                        "enable TERLAN_BENCH_HTTP_WRK_MATRIX_SECONDS to measure".to_string()
                    }),
            },
        ],
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ExternalLoadEvidence {
    pub(crate) generator: String,
    pub(crate) duration_seconds: u64,
    pub(crate) concurrency: usize,
    pub(crate) requests_per_second: f64,
    pub(crate) raw_summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MaintainedWorkloadEvidence {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) connection_mode: String,
    pub(crate) concurrency: usize,
    pub(crate) payload_bytes: usize,
    pub(crate) duration_seconds: u64,
    pub(crate) completed_requests: u64,
    pub(crate) requests_per_second: f64,
    pub(crate) validation_errors: u64,
    pub(crate) transport_errors: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OpenLoopEvidence {
    pub(crate) status: String,
    pub(crate) generator: String,
    pub(crate) duration_seconds: u64,
    pub(crate) points: Vec<OpenLoopPoint>,
    pub(crate) diagnostic: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OpenLoopPoint {
    pub(crate) offered_requests_per_second: u64,
    pub(crate) achieved_requests_per_second: f64,
    pub(crate) validation_errors: u64,
    pub(crate) transport_errors: u64,
}

struct MaintainedWorkloadSpec {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    concurrency: usize,
    body: String,
    expected_status: u16,
    expected_body: Option<String>,
    close: bool,
    headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HardwareCounterEvidence {
    pub(crate) status: String,
    pub(crate) duration_seconds: u64,
    pub(crate) counters: BTreeMap<String, f64>,
    pub(crate) syscall_counter_status: String,
    pub(crate) diagnostic: String,
}

pub(crate) fn run_wrk_probe(
    port: u16,
    concurrency: usize,
    payload_bytes: usize,
) -> Result<Option<ExternalLoadEvidence>, String> {
    let duration_seconds = env::var("TERLAN_BENCH_HTTP_WRK_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if duration_seconds == 0 {
        return Ok(None);
    }
    let script = env::temp_dir().join(format!(
        "terlan-http-wrk-{}-{}.lua",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let body = "x".repeat(payload_bytes);
    fs::write(
        &script,
        format!(
            "wrk.method = \"POST\"\nwrk.body = {:?}\nwrk.headers[\"Content-Type\"] = \"text/plain\"\n",
            body
        ),
    )
    .map_err(|error| format!("cannot write wrk script: {error}"))?;
    let output = Command::new("wrk")
        .arg(format!("-t{}", concurrency.clamp(1, 8)))
        .arg(format!("-c{}", concurrency.max(1)))
        .arg(format!("-d{duration_seconds}s"))
        .arg("--latency")
        .arg("-s")
        .arg(&script)
        .arg(format!("http://127.0.0.1:{port}/api/bench"))
        .output()
        .map_err(|error| format!("cannot run maintained wrk load generator: {error}"));
    let _ = fs::remove_file(&script);
    let output = output?;
    if !output.status.success() {
        return Err(format!(
            "wrk probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let summary = String::from_utf8(output.stdout)
        .map_err(|error| format!("wrk output was not UTF-8: {error}"))?;
    let requests_per_second = summary
        .lines()
        .find_map(|line| line.trim().strip_prefix("Requests/sec:"))
        .and_then(|value| value.trim().parse::<f64>().ok())
        .ok_or_else(|| "wrk output did not contain Requests/sec".to_string())?;
    Ok(Some(ExternalLoadEvidence {
        generator: command_line("wrk", &["-v"]),
        duration_seconds,
        concurrency,
        requests_per_second,
        raw_summary: summary,
    }))
}

pub(crate) fn run_maintained_workload_matrix(
    port: u16,
    reactors: usize,
    concurrency: usize,
    payload_bytes: usize,
    generation: &str,
) -> Result<Vec<MaintainedWorkloadEvidence>, String> {
    let duration_seconds = env::var("TERLAN_BENCH_HTTP_WRK_MATRIX_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if duration_seconds == 0 {
        return Ok(Vec::new());
    }
    let payload = "x".repeat(payload_bytes);
    let json = r#"{"value":"benchmark","count":7}"#.to_string();
    let metadata_body = "metadata-body".to_string();
    let mut header_pressure = Vec::new();
    for index in 0..32 {
        header_pressure.push((
            format!("X-Terlan-Benchmark-{index}"),
            format!("value-{index}"),
        ));
    }
    let cores = reactors.max(1);
    let specs = vec![
        echo_spec(
            "maintained-sequential",
            1,
            payload.clone(),
            generation,
            false,
        ),
        echo_spec(
            "maintained-pressure",
            concurrency,
            payload.clone(),
            generation,
            false,
        ),
        echo_spec(
            "maintained-oversubscribed",
            concurrency.max(cores.saturating_mul(8)),
            payload.clone(),
            generation,
            false,
        ),
        echo_spec(
            "maintained-connection-close",
            concurrency,
            payload.clone(),
            generation,
            true,
        ),
        echo_spec("maintained-empty", cores, String::new(), generation, false),
        echo_spec(
            "maintained-payload-4k",
            cores,
            "x".repeat(4 * 1024),
            generation,
            false,
        ),
        echo_spec(
            "maintained-payload-64k",
            cores.min(4),
            "x".repeat(64 * 1024),
            generation,
            false,
        ),
        echo_spec(
            "maintained-payload-1m",
            cores.min(4),
            "x".repeat(1024 * 1024),
            generation,
            false,
        ),
        MaintainedWorkloadSpec {
            name: "maintained-headers-32",
            headers: header_pressure,
            ..echo_spec(
                "maintained-headers-32",
                concurrency,
                payload,
                generation,
                false,
            )
        },
        MaintainedWorkloadSpec {
            name: "maintained-json",
            method: "POST",
            path: "/api/json",
            concurrency,
            expected_status: 200,
            expected_body: Some(json.clone()),
            body: json,
            close: false,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
        },
        MaintainedWorkloadSpec {
            name: "maintained-metadata",
            method: "POST",
            path: "/api/metadata?page=1",
            concurrency,
            expected_status: 200,
            expected_body: Some(format!(
                "POST:page=1:application/json:session=abc:{metadata_body}"
            )),
            body: metadata_body,
            close: false,
            headers: vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("Cookie".to_string(), "session=abc".to_string()),
            ],
        },
        MaintainedWorkloadSpec {
            name: "maintained-static",
            method: "GET",
            path: "/api/static",
            concurrency,
            expected_status: 200,
            expected_body: Some("static-benchmark-response".to_string()),
            body: String::new(),
            close: false,
            headers: Vec::new(),
        },
        MaintainedWorkloadSpec {
            name: "maintained-not-found",
            method: "GET",
            path: "/missing-benchmark-route",
            concurrency,
            expected_status: 404,
            expected_body: None,
            body: String::new(),
            close: false,
            headers: Vec::new(),
        },
    ];
    specs
        .into_iter()
        .map(|spec| run_wrk_workload(port, duration_seconds, spec))
        .collect()
}

pub(crate) fn run_open_loop_saturation(
    port: u16,
    concurrency: usize,
    payload_bytes: usize,
    generation: &str,
    maintained: &[MaintainedWorkloadEvidence],
) -> Result<OpenLoopEvidence, String> {
    let duration_seconds = env::var("TERLAN_BENCH_HTTP_OPEN_LOOP_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if duration_seconds == 0 {
        return Ok(OpenLoopEvidence {
            status: "disabled".to_string(),
            generator: "wrk2".to_string(),
            duration_seconds,
            points: Vec::new(),
            diagnostic: "set TERLAN_BENCH_HTTP_OPEN_LOOP_SECONDS to enable".to_string(),
        });
    }
    let program = env::var("TERLAN_BENCH_HTTP_WRK2_BIN").unwrap_or_else(|_| "wrk2".to_string());
    let available = Command::new(&program)
        .arg("--help")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        return Ok(OpenLoopEvidence {
            status: "unavailable".to_string(),
            generator: program,
            duration_seconds,
            points: Vec::new(),
            diagnostic: "install wrk2 or set TERLAN_BENCH_HTTP_WRK2_BIN".to_string(),
        });
    }
    let capacity = maintained
        .iter()
        .find(|workload| workload.name == "maintained-pressure")
        .map(|workload| workload.requests_per_second)
        .ok_or_else(|| "open-loop saturation requires maintained-pressure evidence".to_string())?;
    let body = "x".repeat(payload_bytes);
    let spec = echo_spec("open-loop-pressure", concurrency, body, generation, false);
    let mut points = Vec::new();
    for fraction in [0.50, 0.75, 0.90, 1.00, 1.20] {
        let offered = (capacity * fraction).round().max(1.0) as u64;
        let script = write_validating_wrk_script(&spec)?;
        let output = Command::new(&program)
            .arg(format!("-t{}", concurrency.clamp(1, 8)))
            .arg(format!("-c{}", concurrency.max(1)))
            .arg(format!("-d{duration_seconds}s"))
            .arg(format!("-R{offered}"))
            .arg("--latency")
            .arg("-s")
            .arg(&script)
            .arg(format!("http://127.0.0.1:{port}/api/bench"))
            .output()
            .map_err(|error| format!("cannot run open-loop workload: {error}"));
        let _ = fs::remove_file(&script);
        let output = output?;
        if !output.status.success() {
            return Err(format!(
                "open-loop workload failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let summary = String::from_utf8(output.stdout)
            .map_err(|error| format!("wrk2 output was not UTF-8: {error}"))?;
        points.push(OpenLoopPoint {
            offered_requests_per_second: offered,
            achieved_requests_per_second: parse_prefixed_f64(&summary, "Requests/sec:")
                .unwrap_or(0.0),
            validation_errors: summary
                .lines()
                .filter_map(|line| line.trim().strip_prefix("TERLAN_VALIDATION_ERRORS:"))
                .filter_map(|value| value.parse::<u64>().ok())
                .sum(),
            transport_errors: parse_wrk_transport_errors(&summary),
        });
    }
    if points.iter().any(|point| point.validation_errors != 0) {
        return Err("open-loop saturation recorded response-integrity errors".to_string());
    }
    let overload_observed = points.iter().any(|point| point.transport_errors != 0);
    Ok(OpenLoopEvidence {
        status: if overload_observed {
            "measured-with-overload".to_string()
        } else {
            "measured".to_string()
        },
        generator: program,
        duration_seconds,
        points,
        diagnostic: "offered-load sweep corrects closed-loop coordinated omission; transport errors are measured overload behavior, response corruption remains fatal".to_string(),
    })
}

fn echo_spec(
    name: &'static str,
    concurrency: usize,
    body: String,
    generation: &str,
    close: bool,
) -> MaintainedWorkloadSpec {
    MaintainedWorkloadSpec {
        name,
        method: "POST",
        path: "/api/bench",
        concurrency,
        expected_status: 200,
        expected_body: Some(format!("{generation}:{body}")),
        body,
        close,
        headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
    }
}

fn run_wrk_workload(
    port: u16,
    duration_seconds: u64,
    spec: MaintainedWorkloadSpec,
) -> Result<MaintainedWorkloadEvidence, String> {
    let script = write_validating_wrk_script(&spec)?;
    let output = Command::new("wrk")
        .arg(format!("-t{}", spec.concurrency.clamp(1, 8)))
        .arg(format!("-c{}", spec.concurrency.max(1)))
        .arg(format!("-d{duration_seconds}s"))
        .arg("--latency")
        .arg("-s")
        .arg(&script)
        .arg(format!("http://127.0.0.1:{port}{}", spec.path))
        .output()
        .map_err(|error| format!("cannot run wrk workload `{}`: {error}", spec.name));
    let _ = fs::remove_file(&script);
    let output = output?;
    if !output.status.success() {
        return Err(format!(
            "wrk workload `{}` failed: {}",
            spec.name,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let summary = String::from_utf8(output.stdout)
        .map_err(|error| format!("wrk workload output was not UTF-8: {error}"))?;
    let requests_per_second = parse_prefixed_f64(&summary, "Requests/sec:")
        .ok_or_else(|| format!("wrk workload `{}` lacks Requests/sec", spec.name))?;
    let completed_requests = summary
        .lines()
        .find_map(|line| line.trim().split_once(" requests in"))
        .and_then(|(value, _)| value.replace(',', "").parse::<u64>().ok())
        .unwrap_or(0);
    let validation_errors = summary
        .lines()
        .filter_map(|line| line.trim().strip_prefix("TERLAN_VALIDATION_ERRORS:"))
        .filter_map(|value| value.parse::<u64>().ok())
        .sum();
    let transport_errors = parse_wrk_transport_errors(&summary);
    if completed_requests == 0 || validation_errors != 0 || transport_errors != 0 {
        return Err(format!(
            "wrk workload `{}` failed integrity: requests={completed_requests}, validation_errors={validation_errors}, transport_errors={transport_errors}",
            spec.name
        ));
    }
    Ok(MaintainedWorkloadEvidence {
        name: spec.name.to_string(),
        path: spec.path.to_string(),
        connection_mode: if spec.close { "close" } else { "keep-alive" }.to_string(),
        concurrency: spec.concurrency,
        payload_bytes: spec.body.len(),
        duration_seconds,
        completed_requests,
        requests_per_second,
        validation_errors,
        transport_errors,
    })
}

fn write_validating_wrk_script(
    spec: &MaintainedWorkloadSpec,
) -> Result<std::path::PathBuf, String> {
    let script = unique_wrk_script();
    let headers = spec
        .headers
        .iter()
        .map(|(name, value)| format!("wrk.headers[{name:?}] = {value:?}\n"))
        .collect::<String>();
    let expected_body = spec
        .expected_body
        .as_ref()
        .map(|body| format!("{body:?}"))
        .unwrap_or_else(|| "nil".to_string());
    fs::write(
        &script,
        format!(
            "wrk.method = {:?}\nwrk.body = {:?}\nwrk.path = {:?}\n{}{}local errors = 0\nlocal expected_body = {}\nfunction response(status, headers, body)\n  if status ~= {} or (expected_body ~= nil and body ~= expected_body) then errors = errors + 1 end\nend\nfunction done(summary, latency, requests)\n  io.write(\"TERLAN_VALIDATION_ERRORS:\" .. errors .. \"\\n\")\nend\n",
            spec.method,
            spec.body,
            spec.path,
            headers,
            if spec.close {
                "wrk.headers[\"Connection\"] = \"close\"\n"
            } else {
                ""
            },
            expected_body,
            spec.expected_status,
        ),
    )
    .map_err(|error| format!("cannot write validating wrk script: {error}"))?;
    Ok(script)
}

fn unique_wrk_script() -> std::path::PathBuf {
    env::temp_dir().join(format!(
        "terlan-http-wrk-{}-{}.lua",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn parse_prefixed_f64(summary: &str, prefix: &str) -> Option<f64> {
    summary
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix))
        .and_then(|value| value.trim().parse().ok())
}

fn parse_wrk_transport_errors(summary: &str) -> u64 {
    summary
        .lines()
        .find_map(|line| line.trim().strip_prefix("Socket errors:"))
        .map(|line| {
            line.split(',')
                .filter_map(|field| field.split_whitespace().last())
                .filter_map(|value| value.parse::<u64>().ok())
                .sum()
        })
        .unwrap_or(0)
}

pub(crate) fn run_hardware_counter_probe(
    pid: u32,
    port: u16,
    concurrency: usize,
    payload_bytes: usize,
) -> Result<HardwareCounterEvidence, String> {
    let duration_seconds = env::var("TERLAN_BENCH_HTTP_PERF_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if duration_seconds == 0 {
        return Ok(HardwareCounterEvidence {
            status: "disabled".to_string(),
            duration_seconds,
            counters: BTreeMap::new(),
            syscall_counter_status: "disabled".to_string(),
            diagnostic: "set TERLAN_BENCH_HTTP_PERF_SECONDS to enable".to_string(),
        });
    }
    let perf = match Command::new("perf")
        .args([
            "stat",
            "-x,",
            "-e",
            "cycles,instructions,cache-misses,context-switches",
            "-p",
            &pid.to_string(),
            "--",
            "sleep",
            &duration_seconds.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Ok(HardwareCounterEvidence {
                status: "unavailable".to_string(),
                duration_seconds,
                counters: BTreeMap::new(),
                syscall_counter_status: "unavailable-without-intrusive-ptrace".to_string(),
                diagnostic: error.to_string(),
            });
        }
    };
    thread::sleep(Duration::from_millis(100));
    run_wrk_for_duration(port, concurrency, payload_bytes, duration_seconds)?;
    let output = perf
        .wait_with_output()
        .map_err(|error| format!("cannot collect perf counters: {error}"))?;
    let diagnostic = String::from_utf8_lossy(&output.stderr).to_string();
    let counters = diagnostic
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(',');
            let value = fields.next()?.trim().replace(',', "");
            let event = fields.nth(1)?.trim();
            value
                .parse::<f64>()
                .ok()
                .map(|value| (event.to_string(), value))
        })
        .collect::<BTreeMap<_, _>>();
    Ok(HardwareCounterEvidence {
        status: if output.status.success() && !counters.is_empty() {
            "measured".to_string()
        } else {
            "unavailable".to_string()
        },
        duration_seconds,
        counters,
        syscall_counter_status: "unavailable-without-intrusive-ptrace".to_string(),
        diagnostic,
    })
}

fn run_wrk_for_duration(
    port: u16,
    concurrency: usize,
    payload_bytes: usize,
    duration_seconds: u64,
) -> Result<(), String> {
    let script = write_wrk_script(payload_bytes)?;
    let output = Command::new("wrk")
        .arg(format!("-t{}", concurrency.clamp(1, 8)))
        .arg(format!("-c{}", concurrency.max(1)))
        .arg(format!("-d{duration_seconds}s"))
        .arg("-s")
        .arg(&script)
        .arg(format!("http://127.0.0.1:{port}/api/bench"))
        .output()
        .map_err(|error| format!("cannot run wrk for perf probe: {error}"));
    let _ = fs::remove_file(&script);
    let output = output?;
    if !output.status.success() {
        return Err(format!(
            "wrk perf probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn write_wrk_script(payload_bytes: usize) -> Result<std::path::PathBuf, String> {
    let script = env::temp_dir().join(format!(
        "terlan-http-wrk-{}-{}.lua",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let body = "x".repeat(payload_bytes);
    fs::write(
        &script,
        format!(
            "wrk.method = \"POST\"\nwrk.body = {:?}\nwrk.headers[\"Content-Type\"] = \"text/plain\"\n",
            body
        ),
    )
    .map_err(|error| format!("cannot write wrk script: {error}"))?;
    Ok(script)
}
