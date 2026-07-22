//! Complete signed deltas for HTTP AOT performance evidence.

use serde_json::{json, Value};

use super::HttpPerformanceReport;

/// Publishes every latency, throughput, memory, and reload dimension.
pub(super) fn comparison_deltas(
    checked: &HttpPerformanceReport,
    native: &HttpPerformanceReport,
) -> Value {
    json!({
        "sequentialP50Percent": percent_delta(checked.sequential.p50_ns, native.sequential.p50_ns),
        "sequentialP95Percent": percent_delta(checked.sequential.p95_ns, native.sequential.p95_ns),
        "sequentialP99Percent": percent_delta(checked.sequential.p99_ns, native.sequential.p99_ns),
        "sequentialThroughputPercent": percent_delta(
            checked.sequential.throughput_requests_per_second,
            native.sequential.throughput_requests_per_second,
        ),
        "pressureP50Percent": percent_delta(
            checked.pressure.timing.p50_ns,
            native.pressure.timing.p50_ns,
        ),
        "pressureP95Percent": percent_delta(
            checked.pressure.timing.p95_ns,
            native.pressure.timing.p95_ns,
        ),
        "pressureP99Percent": percent_delta(
            checked.pressure.timing.p99_ns,
            native.pressure.timing.p99_ns,
        ),
        "pressureThroughputPercent": percent_delta(
            checked.pressure.timing.throughput_requests_per_second,
            native.pressure.timing.throughput_requests_per_second,
        ),
        "longevityP50Percent": percent_delta(
            checked.longevity.timing.p50_ns,
            native.longevity.timing.p50_ns,
        ),
        "longevityP95Percent": percent_delta(
            checked.longevity.timing.p95_ns,
            native.longevity.timing.p95_ns,
        ),
        "longevityP99Percent": percent_delta(
            checked.longevity.timing.p99_ns,
            native.longevity.timing.p99_ns,
        ),
        "longevityThroughputPercent": percent_delta(
            checked.longevity.timing.throughput_requests_per_second,
            native.longevity.timing.throughput_requests_per_second,
        ),
        "generationReloadPercent": percent_delta(
            checked.generation_overlap.reload_latency_ns,
            native.generation_overlap.reload_latency_ns,
        ),
        "peakResidentMemoryPercent": percent_delta(
            checked.allocation.peak_observed_bytes.unwrap_or_default() as u128,
            native.allocation.peak_observed_bytes.unwrap_or_default() as u128,
        ),
    })
}

/// Computes signed percentage change where positive means native grew.
fn percent_delta(reference: u128, native: u128) -> f64 {
    if reference == 0 {
        return 0.0;
    }
    (native as f64 - reference as f64) * 100.0 / reference as f64
}
