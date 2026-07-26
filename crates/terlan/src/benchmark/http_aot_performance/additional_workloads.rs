//! Repeated measurements for protocol and payload shapes outside the core lanes.

use std::env;
use std::time::Duration;

use super::harness::{
    measure_keep_alive_for_duration, measure_keep_alive_requests, measure_requests,
    measure_requests_for_duration, median_throughput_round,
};
use super::{HttpNamedWorkloadEvidence, HttpPerformanceWorkload, HttpTiming};

/// Records every auxiliary workload with the same median-of-rounds contract.
pub(super) fn measure(
    port: u16,
    workload: &HttpPerformanceWorkload,
) -> Result<Vec<HttpNamedWorkloadEvidence>, String> {
    let short_requests = workload.sequential_requests.clamp(64, 500);
    let concurrent_requests = workload.requests_per_worker.clamp(32, 200);
    let large_concurrency = workload.concurrency.min(4);
    let churn = rounds(workload.measurement_rounds, || {
        measure_close(port, 1, short_requests, 0, workload.measurement_duration_ms)
    })?;
    let persistent = rounds(workload.measurement_rounds, || {
        measure_keep_alive(
            port,
            workload.concurrency,
            concurrent_requests,
            workload.payload_bytes,
            workload.measurement_duration_ms,
        )
    })?;
    let large = rounds(workload.measurement_rounds, || {
        measure_close(
            port,
            large_concurrency,
            32,
            64 * 1024,
            workload.measurement_duration_ms,
        )
    })?;
    let mut workloads = vec![
        evidence(
            "empty-connection-churn",
            "close",
            1,
            short_requests,
            0,
            churn,
        ),
        evidence(
            "persistent-small-body",
            "keep-alive",
            workload.concurrency,
            workload.concurrency.saturating_mul(concurrent_requests),
            workload.payload_bytes,
            persistent,
        ),
        evidence(
            "large-body-64k",
            "close",
            large_concurrency,
            large_concurrency.saturating_mul(32),
            64 * 1024,
            large,
        ),
    ];
    if matrix_enabled() {
        let cores = workload.readiness_reactors.max(1);
        for (name, concurrency, payload_bytes) in [
            ("matrix-c1-empty", 1, 0),
            ("matrix-cores-4k", cores, 4 * 1024),
            ("matrix-oversubscribed-512", cores.saturating_mul(2), 512),
            ("matrix-c4-1m", 4.min(cores), 1024 * 1024),
        ] {
            let measured = rounds(workload.measurement_rounds, || {
                measure_close(
                    port,
                    concurrency,
                    32,
                    payload_bytes,
                    workload.measurement_duration_ms,
                )
            })?;
            workloads.push(evidence(
                name,
                "close",
                concurrency,
                concurrency.saturating_mul(32),
                payload_bytes,
                measured,
            ));
        }
        let headers = shaped_rounds(port, workload, 32, Duration::ZERO)?;
        workloads.push(evidence(
            "matrix-headers-32",
            "close",
            workload.concurrency,
            workload.concurrency.saturating_mul(32),
            workload.payload_bytes,
            headers,
        ));
        let slow = shaped_rounds(port, workload, 0, Duration::from_millis(5))?;
        workloads.push(evidence(
            "matrix-slow-reader-5ms",
            "close-delayed-read",
            workload.concurrency,
            workload.concurrency.saturating_mul(16),
            workload.payload_bytes,
            slow,
        ));
    }
    Ok(workloads)
}

fn shaped_rounds(
    port: u16,
    workload: &HttpPerformanceWorkload,
    extra_headers: usize,
    delay: Duration,
) -> Result<(HttpTiming, Vec<HttpTiming>), String> {
    rounds(workload.measurement_rounds, || {
        let measured = if workload.measurement_duration_ms == 0 {
            super::http_client::measure_shaped(
                port,
                workload.concurrency,
                if delay.is_zero() { 32 } else { 16 },
                workload.payload_bytes,
                "generation-one",
                extra_headers,
                delay,
            )?
        } else {
            super::http_client::measure_shaped_for_duration(
                port,
                workload.concurrency,
                Duration::from_millis(workload.measurement_duration_ms),
                workload.payload_bytes,
                "generation-one",
                extra_headers,
                delay,
            )?
        };
        HttpTiming::from_durations(&measured.durations, measured.wall)
    })
}

fn measure_close(
    port: u16,
    concurrency: usize,
    requests: usize,
    payload: usize,
    duration_ms: u64,
) -> Result<HttpTiming, String> {
    if duration_ms == 0 {
        measure_requests(port, concurrency, requests, payload)
    } else {
        measure_requests_for_duration(
            port,
            concurrency,
            Duration::from_millis(duration_ms),
            payload,
        )
    }
}

fn measure_keep_alive(
    port: u16,
    concurrency: usize,
    requests: usize,
    payload: usize,
    duration_ms: u64,
) -> Result<HttpTiming, String> {
    if duration_ms == 0 {
        measure_keep_alive_requests(port, concurrency, requests, payload)
    } else {
        measure_keep_alive_for_duration(
            port,
            concurrency,
            Duration::from_millis(duration_ms),
            payload,
        )
    }
}

fn matrix_enabled() -> bool {
    env::var("TERLAN_BENCH_HTTP_MATRIX")
        .map(|value| value != "0")
        .unwrap_or(true)
}

fn rounds(
    count: usize,
    mut measure: impl FnMut() -> Result<HttpTiming, String>,
) -> Result<(HttpTiming, Vec<HttpTiming>), String> {
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(measure()?);
    }
    Ok((median_throughput_round(&samples)?.clone(), samples))
}

fn evidence(
    name: &str,
    connection_mode: &str,
    concurrency: usize,
    _requests: usize,
    payload_bytes: usize,
    (timing, rounds): (HttpTiming, Vec<HttpTiming>),
) -> HttpNamedWorkloadEvidence {
    let requests = timing.sample_count;
    HttpNamedWorkloadEvidence {
        name: name.to_string(),
        connection_mode: connection_mode.to_string(),
        concurrency,
        requests,
        payload_bytes,
        timing,
        rounds,
    }
}
