//! Extended, additive evidence for reproducible HTTP measurements.

use serde::{Deserialize, Serialize};

use super::{harness, HttpPerformanceWorkload};
use super::{http_benchmark_support, http_client, HttpTiming};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct HttpIntegrityEvidence {
    pub(super) attempted_requests: usize,
    pub(super) completed_requests: usize,
    pub(super) failed_requests: usize,
    pub(super) response_body_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct HttpSoakEvidence {
    pub(super) duration_seconds: u64,
    pub(super) timing: HttpTiming,
    pub(super) memory_before: Option<http_client::ProcessMemorySnapshot>,
    pub(super) memory_after: Option<http_client::ProcessMemorySnapshot>,
    pub(super) resident_growth_bytes: Option<i64>,
    pub(super) maximum_growth_bytes: u64,
    pub(super) status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct HttpExtendedBenchmarkEvidence {
    pub(super) execution: http_benchmark_support::BenchmarkExecutionMetadata,
    pub(super) integrity: HttpIntegrityEvidence,
    pub(super) protocol_validation: http_benchmark_support::ProtocolValidationEvidence,
    pub(super) protocol_scenarios: Vec<http_benchmark_support::ProtocolScenarioEvidence>,
    pub(super) external_load: Option<http_benchmark_support::ExternalLoadEvidence>,
    pub(super) maintained_workloads: Vec<http_benchmark_support::MaintainedWorkloadEvidence>,
    pub(super) open_loop: http_benchmark_support::OpenLoopEvidence,
    pub(super) lifecycle: http_benchmark_support::LifecycleEvidence,
    pub(super) memory_attribution: http_client::MemoryAttributionEvidence,
    pub(super) hardware_counters: http_benchmark_support::HardwareCounterEvidence,
    pub(super) efficiency: Option<http_client::ProcessEfficiencyEvidence>,
    pub(super) soak: Option<HttpSoakEvidence>,
}

pub(super) fn fixture() -> HttpExtendedBenchmarkEvidence {
    HttpExtendedBenchmarkEvidence {
        execution: http_benchmark_support::BenchmarkExecutionMetadata {
            schema: "terlan-http-benchmark-execution-v1".to_string(),
            server_binary_sha256: "0".repeat(64),
            server_binary_bytes: 1,
            server_cpu_list: "test".to_string(),
            client_cpu_list: "test".to_string(),
            reactor_count: 2,
            kernel: "test".to_string(),
            cpu_governor: "test".to_string(),
            rustflags: "test".to_string(),
            allocator: "test".to_string(),
            protocol_validator: "test".to_string(),
            load_generator: "test".to_string(),
            performance_counter_tool: "test".to_string(),
            performance_event_policy: "test".to_string(),
            host: "test".to_string(),
            numa_nodes: "test".to_string(),
            recorded_unix_seconds: 1,
            build_profile: "test".to_string(),
            target_cpu: "test".to_string(),
            lto: "test".to_string(),
            codegen_units: "test".to_string(),
            panic_strategy: "test".to_string(),
            cargo_lock_sha256: "0".repeat(64),
            socket_policy: "test".to_string(),
        },
        integrity: HttpIntegrityEvidence {
            attempted_requests: 6,
            completed_requests: 6,
            failed_requests: 0,
            response_body_verified: true,
        },
        protocol_validation: http_benchmark_support::ProtocolValidationEvidence {
            validator: "test".to_string(),
            protocol: "HTTP/1.1".to_string(),
            status: "validated".to_string(),
            response_body_sha256: "0".repeat(64),
        },
        protocol_scenarios: ["http-1.1", "error-response-404"]
            .into_iter()
            .map(|name| http_benchmark_support::ProtocolScenarioEvidence {
                name: name.to_string(),
                status: "validated".to_string(),
                detail: "test".to_string(),
            })
            .collect(),
        external_load: None,
        maintained_workloads: Vec::new(),
        open_loop: http_benchmark_support::OpenLoopEvidence {
            status: "disabled".to_string(),
            generator: "test".to_string(),
            duration_seconds: 0,
            points: Vec::new(),
            diagnostic: "test".to_string(),
        },
        lifecycle: http_benchmark_support::LifecycleEvidence {
            status: "validated".to_string(),
            scenarios: Vec::new(),
        },
        memory_attribution: http_client::MemoryAttributionEvidence {
            idle_rss_bytes: Some(1),
            idle_pss_bytes: Some(1),
            warmed_rss_bytes: Some(1),
            peak_rss_bytes: Some(1),
            retained_rss_delta_bytes: Some(0),
            retained_pss_delta_bytes: Some(0),
            peak_rss_bytes_per_completed_request: Some(1.0),
            thread_count_peak: Some(1),
            attribution: "test".to_string(),
        },
        hardware_counters: http_benchmark_support::HardwareCounterEvidence {
            status: "disabled".to_string(),
            duration_seconds: 0,
            counters: Default::default(),
            syscall_counter_status: "disabled".to_string(),
            diagnostic: "test".to_string(),
        },
        efficiency: None,
        soak: None,
    }
}

pub(super) fn measure_soak(
    port: u16,
    pid: u32,
    workload: &HttpPerformanceWorkload,
) -> Result<Option<HttpSoakEvidence>, String> {
    if workload.soak_seconds == 0 {
        return Ok(None);
    }
    let memory_before = http_client::process_memory_snapshot(pid, "before_soak");
    let timing = harness::measure_keep_alive_for_duration(
        port,
        workload.concurrency,
        std::time::Duration::from_secs(workload.soak_seconds),
        workload.payload_bytes,
    )?;
    let memory_after = http_client::process_memory_snapshot(pid, "after_soak");
    let maximum_growth_bytes = std::env::var("TERLAN_BENCH_HTTP_SOAK_MAX_GROWTH_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(16 * 1024 * 1024);
    let resident_growth_bytes = memory_before
        .as_ref()
        .and_then(|before| before.rss_bytes)
        .zip(memory_after.as_ref().and_then(|after| after.rss_bytes))
        .map(|(before, after)| after as i64 - before as i64);
    if resident_growth_bytes
        .is_some_and(|growth| growth > i64::try_from(maximum_growth_bytes).unwrap_or(i64::MAX))
    {
        return Err(format!(
            "HTTP AOT soak RSS growth {:?} exceeds {} bytes",
            resident_growth_bytes, maximum_growth_bytes
        ));
    }
    Ok(Some(HttpSoakEvidence {
        duration_seconds: workload.soak_seconds,
        timing,
        memory_before,
        memory_after,
        resident_growth_bytes,
        maximum_growth_bytes,
        status: "stable".to_string(),
    }))
}
