use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::model::{Comparison, LoadGeneratorHeadroom};

pub(super) fn validate_pair(aot: &Value, baseline: &Value) -> Result<(), String> {
    require(aot, "/schema", "terlan-http-aot-performance-v2")?;
    require(baseline, "/schema", "terlan-http-framework-performance-v2")?;
    require(aot, "/status", "completed")?;
    require(baseline, "/status", "completed")?;
    require(
        aot,
        "/benchmark_evidence/protocol_validation/status",
        "validated",
    )?;
    require(baseline, "/protocol_validation/status", "validated")?;
    let aot_hardware = string(aot, "/hardware/sha256")?;
    let baseline_hardware = string(baseline, "/hardware/sha256")?;
    if aot_hardware != baseline_hardware {
        return Err("paired HTTP reports have different hardware fingerprints".to_string());
    }
    for field in [
        "readiness_reactors",
        "sequential_requests",
        "concurrency",
        "requests_per_worker",
        "longevity_requests",
        "payload_bytes",
        "measurement_duration_ms",
    ] {
        let pointer = format!("/workload/{field}");
        if aot.pointer(&pointer) != baseline.pointer(&pointer) {
            return Err(format!("paired HTTP workload field `{field}` differs"));
        }
    }
    for field in [
        "server_cpu_list",
        "client_cpu_list",
        "reactor_count",
        "build_profile",
        "target_cpu",
        "lto",
        "codegen_units",
        "panic_strategy",
        "cargo_lock_sha256",
        "socket_policy",
    ] {
        let aot_pointer = format!("/benchmark_evidence/execution/{field}");
        let baseline_pointer = format!("/execution/{field}");
        if aot.pointer(&aot_pointer) != baseline.pointer(&baseline_pointer) {
            return Err(format!("paired HTTP execution field `{field}` differs"));
        }
    }
    let aot_metrics = throughput_metrics(aot, true)?;
    let baseline_metrics = throughput_metrics(baseline, false)?;
    if aot_metrics.keys().collect::<BTreeSet<_>>()
        != baseline_metrics.keys().collect::<BTreeSet<_>>()
    {
        return Err("paired HTTP workload matrix names differ".to_string());
    }
    Ok(())
}

pub(super) fn comparisons(
    reports: &[(&Value, &Value)],
    baseline: &'static str,
) -> Result<BTreeMap<String, Comparison>, String> {
    let mut ratios: BTreeMap<String, Vec<(f64, f64, f64)>> = BTreeMap::new();
    for (aot, axum) in reports {
        let aot_metrics = throughput_metrics(aot, true)?;
        let axum_metrics = throughput_metrics(axum, false)?;
        for (name, aot_rps) in aot_metrics {
            let axum_rps = *axum_metrics
                .get(&name)
                .ok_or_else(|| format!("Axum report lacks workload `{name}`"))?;
            ratios.entry(name).or_default().push((
                aot_rps,
                axum_rps,
                aot_rps / axum_rps.max(f64::EPSILON),
            ));
        }
    }
    ratios
        .into_iter()
        .map(|(name, values)| {
            let aot = values.iter().map(|value| value.0).collect::<Vec<_>>();
            let axum = values.iter().map(|value| value.1).collect::<Vec<_>>();
            let ratio = values.iter().map(|value| value.2).collect::<Vec<_>>();
            let median_ratio = median(&ratio);
            let interval = bootstrap_median_interval(&ratio, 10_000);
            let verdict = if interval[0] > 1.0 {
                "aot-faster"
            } else if interval[1] < 1.0 {
                "baseline-faster"
            } else {
                "inconclusive"
            };
            let clue = (verdict == "baseline-faster")
                .then(|| performance_clue(&name))
                .flatten();
            let comparison = Comparison {
                baseline,
                samples: values.len(),
                aot_median_requests_per_second: median(&aot),
                baseline_median_requests_per_second: median(&axum),
                median_aot_to_baseline_ratio: median_ratio,
                minimum_aot_to_baseline_ratio: ratio.iter().copied().fold(f64::INFINITY, f64::min),
                maximum_aot_to_baseline_ratio: ratio
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max),
                ratio_standard_deviation: standard_deviation(&ratio),
                ratio_95_percent_interval: interval,
                confidence_method: "deterministic-bootstrap-of-paired-medians",
                bootstrap_samples: 10_000,
                aot_wins: ratio.iter().filter(|value| **value > 1.0).count(),
                verdict,
                suspected_subsystem: clue.map(|value| value.0),
                source_location: clue.map(|value| value.1),
                next_optimization_hypothesis: clue.map(|value| value.2),
            };
            Ok((name, comparison))
        })
        .collect()
}

fn performance_clue(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match name {
        "large-body-64k" | "matrix-c4-1m" | "maintained-payload-64k" | "maintained-payload-1m" => (
            "request/response body materialization",
            "crates/terlan/src/commands/serve/handler/request_materialization.rs::{replace_vm_request_descriptor,vm_request_descriptor_owned}",
            "profile copy counts and retain body buffers across reusable service-actor calls",
        ),
        "matrix-headers-32" | "maintained-headers-32" | "maintained-metadata" => (
            "header projection and managed-value materialization",
            "crates/terlan/src/commands/serve/handler/request_materialization.rs::{replace_projected_map,replace_string_map}",
            "defer uncommon header projection and reuse owner-local header storage",
        ),
        "matrix-slow-reader-5ms" => (
            "socket readiness and backpressure",
            "crates/terlan/src/runtime/vm/protocol_task_executor.rs::{publish_readiness,run}",
            "profile write-interest registration and bounded slow-client queue occupancy",
        ),
        "pressure"
        | "matrix-oversubscribed-512"
        | "matrix-cores-4k"
        | "maintained-pressure"
        | "maintained-oversubscribed"
        | "maintained-concurrency-100"
        | "maintained-concurrency-1000" => (
            "shard admission and cross-shard dispatch",
            "crates/terlan/src/commands/serve/handler_cache/{invocation.rs::begin_request_invocation,shard_owner/owner_loop.rs::owner_loop}",
            "profile inbox publication, wake coalescing, and owner-loop batch size",
        ),
        "persistent-small-body"
        | "maintained-json"
        | "maintained-add"
        | "maintained-crud-create"
        | "maintained-crud-read"
        | "maintained-crud-update"
        | "maintained-crud-delete" => (
            "reusable service-actor dispatch",
            "crates/terlan/src/commands/serve/handler_cache/{http_response.rs::execute_suspendable_http_response,invocation.rs::begin_request_invocation}",
            "profile per-call heap release, actor wakeup, and response handoff",
        ),
        "sequential"
        | "longevity"
        | "empty-connection-churn"
        | "matrix-c1-empty"
        | "maintained-sequential"
        | "maintained-connection-close"
        | "maintained-empty"
        | "maintained-static"
        | "maintained-not-found"
        | "maintained-payload-4k" => (
            "connection lifecycle and request admission",
            "crates/terlan/src/runtime/vm/protocol_task_executor.rs::{serve_protocol_tasks,publish_readiness}; crates/terlan/src/commands/serve/server_lifecycle.rs::serve_bound_directory_vm_stream",
            "profile connection slab reuse, accept wakeups, and fixed-owner routing",
        ),
        _ => return None,
    })
}

pub(super) fn load_generator_headroom(
    aot: &Value,
    axum: &Value,
    minimum_ratio: f64,
) -> Result<Option<LoadGeneratorHeadroom>, String> {
    let Some(aot_external) = aot
        .pointer("/benchmark_evidence/external_load/requests_per_second")
        .and_then(Value::as_f64)
    else {
        return Ok(None);
    };
    let Some(axum_external) = axum
        .pointer("/external_load/requests_per_second")
        .and_then(Value::as_f64)
    else {
        return Ok(None);
    };
    let aot_internal = named_throughput(aot, "persistent-small-body")?;
    let axum_internal = named_throughput(axum, "persistent-small-body")?;
    let aot_ratio = aot_external / aot_internal.max(f64::EPSILON);
    let axum_ratio = axum_external / axum_internal.max(f64::EPSILON);
    if aot_ratio < minimum_ratio || axum_ratio < minimum_ratio {
        return Err(format!(
            "maintained load generator lacks headroom: AOT={aot_ratio:.3}, Axum={axum_ratio:.3}, required={minimum_ratio:.3}"
        ));
    }
    Ok(Some(LoadGeneratorHeadroom {
        aot_external_requests_per_second: aot_external,
        aot_internal_persistent_requests_per_second: aot_internal,
        aot_headroom_ratio: aot_ratio,
        axum_external_requests_per_second: axum_external,
        axum_internal_persistent_requests_per_second: axum_internal,
        axum_headroom_ratio: axum_ratio,
        status: "validated",
    }))
}

fn throughput_metrics(report: &Value, aot: bool) -> Result<BTreeMap<String, f64>, String> {
    let maintained_path = if aot {
        "/benchmark_evidence/maintained_workloads"
    } else {
        "/maintained_workloads"
    };
    if let Some(workloads) = report.pointer(maintained_path).and_then(Value::as_array) {
        if !workloads.is_empty() {
            return workloads
                .iter()
                .map(|workload| {
                    let name = string(workload, "/name")?.to_string();
                    let rps = number(workload, "/requests_per_second")?;
                    Ok((name, rps))
                })
                .collect();
        }
    }
    let paths = if aot {
        [
            ("sequential", "/sequential/throughput_requests_per_second"),
            (
                "pressure",
                "/pressure/timing/throughput_requests_per_second",
            ),
            (
                "longevity",
                "/longevity/timing/throughput_requests_per_second",
            ),
        ]
    } else {
        [
            ("sequential", "/sequential/throughput_requests_per_second"),
            ("pressure", "/pressure/throughput_requests_per_second"),
            ("longevity", "/longevity/throughput_requests_per_second"),
        ]
    };
    let mut metrics = BTreeMap::new();
    for (name, path) in paths {
        metrics.insert(name.to_string(), number(report, path)?);
    }
    let workloads = report
        .pointer("/additional_workloads")
        .and_then(Value::as_array)
        .ok_or_else(|| "HTTP report lacks additional workloads".to_string())?;
    for workload in workloads {
        let name = workload
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "HTTP workload lacks a name".to_string())?;
        let rps = number(workload, "/timing/throughput_requests_per_second")?;
        metrics.insert(name.to_string(), rps);
    }
    Ok(metrics)
}

fn named_throughput(report: &Value, requested: &str) -> Result<f64, String> {
    report
        .pointer("/additional_workloads")
        .and_then(Value::as_array)
        .and_then(|workloads| {
            workloads
                .iter()
                .find(|workload| workload.get("name").and_then(Value::as_str) == Some(requested))
        })
        .map(|workload| number(workload, "/timing/throughput_requests_per_second"))
        .transpose()?
        .ok_or_else(|| format!("HTTP report lacks workload `{requested}`"))
}

fn bootstrap_median_interval(values: &[f64], samples: usize) -> [f64; 2] {
    let mut seed = 0x6a09_e667_f3bc_c909_u64;
    let mut medians = Vec::with_capacity(samples);
    let mut sample = Vec::with_capacity(values.len());
    for _ in 0..samples {
        sample.clear();
        for _ in values {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            sample.push(values[seed as usize % values.len()]);
        }
        medians.push(median(&sample));
    }
    medians.sort_by(f64::total_cmp);
    let high = medians.len().saturating_mul(975).div_ceil(1000);
    [
        medians[medians.len().saturating_mul(25) / 1000],
        medians[high.saturating_sub(1).min(medians.len() - 1)],
    ]
}

fn median(values: &[f64]) -> f64 {
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    ordered[ordered.len() / 2]
}

fn standard_deviation(values: &[f64]) -> f64 {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt()
}

fn require(report: &Value, path: &str, expected: &str) -> Result<(), String> {
    let actual = string(report, path)?;
    if actual != expected {
        return Err(format!(
            "HTTP report `{path}` is `{actual}`, expected `{expected}`"
        ));
    }
    Ok(())
}

fn string<'a>(report: &'a Value, path: &str) -> Result<&'a str, String> {
    report
        .pointer(path)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("HTTP report lacks string `{path}`"))
}

fn number(report: &Value, path: &str) -> Result<f64, String> {
    report
        .pointer(path)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("HTTP report lacks number `{path}`"))
}
