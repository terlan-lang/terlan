//! VM-owned length-prefixed framing benchmark workloads.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::runtime::vm::framing::{VmFramingError, VmInMemoryFrameReader};
use crate::runtime::vm::tcp::VmTcpRuntime;

use super::unix_timestamp_seconds;

/// Framing workload selected by the standalone VM benchmark command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BenchmarkFramingWorkload {
    Roundtrip,
    Truncated,
    MalformedLength,
    InvalidUtf8,
}

impl BenchmarkFramingWorkload {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "roundtrip" => Ok(Self::Roundtrip),
            "truncated" => Ok(Self::Truncated),
            "malformed-length" => Ok(Self::MalformedLength),
            "invalid-utf8" => Ok(Self::InvalidUtf8),
            _ => Err(format!(
                "terlan-vm benchmark-in-memory-framing --workload expects `roundtrip`, `truncated`, `malformed-length`, or `invalid-utf8`, got `{value}`"
            )),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Roundtrip => "roundtrip",
            Self::Truncated => "truncated",
            Self::MalformedLength => "malformed-length",
            Self::InvalidUtf8 => "invalid-utf8",
        }
    }

    fn measurement_name(self) -> &'static str {
        match self {
            Self::Roundtrip => "vm_in_memory_length_prefixed_frame_roundtrip",
            Self::Truncated => "vm_in_memory_length_prefixed_truncated_frame_rejection",
            Self::MalformedLength => "vm_in_memory_malformed_length_rejection",
            Self::InvalidUtf8 => "vm_in_memory_invalid_utf8_rejection",
        }
    }

    fn assertion_detail(self) -> &'static str {
        match self {
            Self::Roundtrip => "every encoded frame decoded to the original payload",
            Self::Truncated => "every incomplete half-closed frame produced typed FramingEof",
            Self::MalformedLength => {
                "every impossible length prefix produced typed FramingOverflow"
            }
            Self::InvalidUtf8 => "every framed invalid text payload produced typed InvalidUtf8",
        }
    }
}

#[derive(Serialize)]
struct FramingBenchmarkReport {
    schema: &'static str,
    benchmark: &'static str,
    status: &'static str,
    workload: &'static str,
    timestamp_unix_seconds: u64,
    terlan_version: &'static str,
    runtime_stack: FramingRuntimeStack,
    iterations: usize,
    payload_bytes: usize,
    expected_typed_failure_count: usize,
    measurement: FramingMeasurement,
    assertion: FramingAssertion,
}

#[derive(Serialize)]
struct FramingRuntimeStack {
    runtime: &'static str,
    transport: &'static str,
    framing: &'static str,
    host_async_runtime: &'static str,
}

#[derive(Serialize)]
struct FramingMeasurement {
    name: &'static str,
    unit: &'static str,
    total_us: u128,
    mean_us: u128,
    p50_us: u128,
    p95_us: u128,
    min_us: u128,
    max_us: u128,
}

#[derive(Serialize)]
struct FramingAssertion {
    name: &'static str,
    passed: bool,
    detail: &'static str,
}

/// Runs one validated framing workload and renders its report as JSON.
pub(super) fn benchmark_in_memory_framing(
    iterations: usize,
    payload_bytes: usize,
    workload: BenchmarkFramingWorkload,
) -> Result<String, String> {
    if iterations == 0 {
        return Err("framing benchmark iterations must be greater than 0".to_string());
    }
    let durations = match workload {
        BenchmarkFramingWorkload::Roundtrip => benchmark_roundtrips(iterations, payload_bytes)?,
        BenchmarkFramingWorkload::Truncated => {
            benchmark_truncated_frames(iterations, payload_bytes)?
        }
        BenchmarkFramingWorkload::MalformedLength => {
            benchmark_malformed_length_frames(iterations, payload_bytes)?
        }
        BenchmarkFramingWorkload::InvalidUtf8 => {
            benchmark_invalid_utf8_frames(iterations, payload_bytes)?
        }
    };
    render_report(iterations, payload_bytes, workload, &durations)
}

fn benchmark_roundtrips(iterations: usize, payload_bytes: usize) -> Result<Vec<Duration>, String> {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("bench.framing")?;
    let client = tcp.connect("bench.framing", "benchmark.client")?;
    let server = tcp
        .accept(listener, "benchmark.server")?
        .ok_or_else(|| "benchmark stream was not queued".to_string())?;
    let buffer_limit = payload_bytes.saturating_add(4).max(8);
    let mut writer = VmInMemoryFrameReader::new(client, buffer_limit)
        .map_err(|error| format!("framing writer init failed: {error:?}"))?;
    let mut reader = VmInMemoryFrameReader::new(server, buffer_limit)
        .map_err(|error| format!("framing reader init failed: {error:?}"))?;
    let payload = deterministic_payload(payload_bytes);
    let mut durations = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        writer
            .write_length_prefixed(&mut tcp, payload.clone())
            .map_err(|error| format!("framing write failed: {error:?}"))?;
        let decoded = reader
            .read_length_prefixed(&mut tcp)
            .map_err(|error| format!("framing read failed: {error:?}"))?
            .ok_or_else(|| "framing benchmark expected immediate frame".to_string())?;
        if decoded != payload {
            return Err("framing benchmark decoded payload mismatch".to_string());
        }
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn benchmark_truncated_frames(
    iterations: usize,
    payload_bytes: usize,
) -> Result<Vec<Duration>, String> {
    let payload = deterministic_payload(payload_bytes);
    let declared_len = u32::try_from(payload_bytes.saturating_add(1))
        .map_err(|_| "truncated framing payload exceeds u32 length prefix".to_string())?;
    let buffer_limit = payload_bytes.saturating_add(5).max(8);
    let mut durations = Vec::with_capacity(iterations);

    for iteration in 0..iterations {
        let address = format!("bench.framing.truncated.{iteration}");
        let mut tcp = VmTcpRuntime::new();
        let listener = tcp.listen(&address)?;
        let client = tcp.connect(&address, "benchmark.client")?;
        let server = tcp
            .accept(listener, "benchmark.server")?
            .ok_or_else(|| "truncated benchmark stream was not queued".to_string())?;
        let mut writer = VmInMemoryFrameReader::new(client, buffer_limit)
            .map_err(|error| format!("truncated framing writer init failed: {error:?}"))?;
        let mut reader = VmInMemoryFrameReader::new(server, buffer_limit)
            .map_err(|error| format!("truncated framing reader init failed: {error:?}"))?;
        let mut frame = declared_len.to_be_bytes().to_vec();
        frame.extend_from_slice(&payload);

        let start = Instant::now();
        writer
            .write(&mut tcp, frame)
            .map_err(|error| format!("truncated framing write failed: {error:?}"))?;
        tcp.close_write(writer.stream())?;
        let failure = match reader.read_length_prefixed(&mut tcp) {
            Err(failure) => failure,
            Ok(Some(_)) => {
                return Err(
                    "truncated framing benchmark unexpectedly decoded a complete frame".to_string(),
                )
            }
            Ok(None) => {
                return Err(
                    "truncated framing benchmark remained pending after peer half-close"
                        .to_string(),
                )
            }
        };
        if failure != VmFramingError::FramingEof {
            return Err(format!(
                "truncated framing benchmark expected FramingEof, got {failure:?}"
            ));
        }
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn benchmark_malformed_length_frames(
    iterations: usize,
    payload_bytes: usize,
) -> Result<Vec<Duration>, String> {
    let buffer_limit = payload_bytes.saturating_add(4).max(8);
    let impossible_len = u32::try_from(buffer_limit.saturating_add(1))
        .map_err(|_| "malformed framing buffer exceeds u32 length prefix".to_string())?;
    let mut durations = Vec::with_capacity(iterations);

    for iteration in 0..iterations {
        let address = format!("bench.framing.malformed.{iteration}");
        let mut tcp = VmTcpRuntime::new();
        let listener = tcp.listen(&address)?;
        let client = tcp.connect(&address, "benchmark.client")?;
        let server = tcp
            .accept(listener, "benchmark.server")?
            .ok_or_else(|| "malformed benchmark stream was not queued".to_string())?;
        let mut writer = VmInMemoryFrameReader::new(client, buffer_limit)
            .map_err(|error| format!("malformed framing writer init failed: {error:?}"))?;
        let mut reader = VmInMemoryFrameReader::new(server, buffer_limit)
            .map_err(|error| format!("malformed framing reader init failed: {error:?}"))?;

        let start = Instant::now();
        writer
            .write(&mut tcp, impossible_len.to_be_bytes().to_vec())
            .map_err(|error| format!("malformed framing write failed: {error:?}"))?;
        let failure = reader
            .read_length_prefixed(&mut tcp)
            .expect_err("impossible length prefix must fail");
        if failure != VmFramingError::FramingOverflow {
            return Err(format!(
                "malformed framing benchmark expected FramingOverflow, got {failure:?}"
            ));
        }
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn benchmark_invalid_utf8_frames(
    iterations: usize,
    payload_bytes: usize,
) -> Result<Vec<Duration>, String> {
    let buffer_limit = payload_bytes.saturating_add(4).max(8);
    let invalid_len = payload_bytes.max(1);
    let mut payload = deterministic_payload(invalid_len);
    payload[0] = 0xff;
    let mut durations = Vec::with_capacity(iterations);

    for iteration in 0..iterations {
        let address = format!("bench.framing.invalid-utf8.{iteration}");
        let mut tcp = VmTcpRuntime::new();
        let listener = tcp.listen(&address)?;
        let client = tcp.connect(&address, "benchmark.client")?;
        let server = tcp
            .accept(listener, "benchmark.server")?
            .ok_or_else(|| "invalid UTF-8 benchmark stream was not queued".to_string())?;
        let mut writer = VmInMemoryFrameReader::new(client, buffer_limit)
            .map_err(|error| format!("invalid UTF-8 writer init failed: {error:?}"))?;
        let mut reader = VmInMemoryFrameReader::new(server, buffer_limit)
            .map_err(|error| format!("invalid UTF-8 reader init failed: {error:?}"))?;

        let start = Instant::now();
        writer
            .write_length_prefixed(&mut tcp, payload.clone())
            .map_err(|error| format!("invalid UTF-8 frame write failed: {error:?}"))?;
        let decoded = reader
            .read_length_prefixed(&mut tcp)
            .map_err(|error| format!("invalid UTF-8 frame read failed: {error:?}"))?
            .ok_or_else(|| "invalid UTF-8 frame remained pending".to_string())?;
        if std::str::from_utf8(&decoded).is_ok() {
            return Err("invalid UTF-8 benchmark unexpectedly decoded text".to_string());
        }
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn deterministic_payload(payload_bytes: usize) -> Vec<u8> {
    (0..payload_bytes)
        .map(|index| (index % 251) as u8)
        .collect()
}

fn percentile(sorted_values: &[u128], percentile: usize) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }
    let index = ((sorted_values.len() - 1) * percentile).div_ceil(100);
    sorted_values[index]
}

fn render_report(
    iterations: usize,
    payload_bytes: usize,
    workload: BenchmarkFramingWorkload,
    durations: &[Duration],
) -> Result<String, String> {
    let mut nanos = durations.iter().map(Duration::as_nanos).collect::<Vec<_>>();
    nanos.sort_unstable();
    let total_ns = nanos.iter().copied().sum::<u128>();
    let measurement_name = workload.measurement_name();
    let report = FramingBenchmarkReport {
        schema: "terlan.vm-in-memory-framing-benchmark.v2",
        benchmark: "vm-in-memory-length-prefixed-framing",
        status: "completed",
        workload: workload.as_str(),
        timestamp_unix_seconds: unix_timestamp_seconds(),
        terlan_version: env!("CARGO_PKG_VERSION"),
        runtime_stack: FramingRuntimeStack {
            runtime: "Terlan VM TCP fixture",
            transport: "in-memory VM stream",
            framing: "u32 big-endian length-prefixed",
            host_async_runtime: "absent from measured path",
        },
        iterations,
        payload_bytes,
        expected_typed_failure_count: if workload == BenchmarkFramingWorkload::Roundtrip {
            0
        } else {
            iterations
        },
        measurement: FramingMeasurement {
            name: measurement_name,
            unit: "microseconds",
            total_us: total_ns / 1_000,
            mean_us: total_ns / iterations as u128 / 1_000,
            p50_us: percentile(&nanos, 50) / 1_000,
            p95_us: percentile(&nanos, 95) / 1_000,
            min_us: nanos.first().copied().unwrap_or(0) / 1_000,
            max_us: nanos.last().copied().unwrap_or(0) / 1_000,
        },
        assertion: FramingAssertion {
            name: measurement_name,
            passed: true,
            detail: workload.assertion_detail(),
        },
    };
    serde_json::to_string_pretty(&report)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("failed to serialize framing benchmark report: {error}"))
}

#[cfg(test)]
#[path = "framing_benchmark_test.rs"]
mod tests;
