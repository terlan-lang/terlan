use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(super) struct PairedReport {
    pub(super) schema: &'static str,
    pub(super) status: &'static str,
    pub(super) environment: EnvironmentDecision,
    pub(super) pair_count: usize,
    pub(super) accepted_pair_count: usize,
    pub(super) configuration: Configuration,
    pub(super) pairs: Vec<PairEvidence>,
    pub(super) comparisons: BTreeMap<String, Comparison>,
    pub(super) hyper_comparisons: BTreeMap<String, Comparison>,
    pub(super) isolation: Vec<IsolationEvidence>,
}

#[derive(Serialize)]
pub(super) struct EnvironmentDecision {
    pub(super) mode: &'static str,
    pub(super) status: &'static str,
    pub(super) reasons: Vec<String>,
    pub(super) cpu_governor: String,
    pub(super) server_cpu_list: String,
    pub(super) client_cpu_list: String,
    pub(super) irq_default_affinity: String,
    pub(super) numa_topology: String,
}

#[derive(Serialize)]
pub(super) struct Configuration {
    pub(super) aot_benchmark_binary: String,
    pub(super) aot_server_binary: String,
    pub(super) axum_benchmark_binary: String,
    pub(super) axum_server_binary: String,
    pub(super) hyper_benchmark_binary: String,
    pub(super) hyper_server_binary: String,
    pub(super) measurement_duration_ms: u64,
    pub(super) soak_seconds: u64,
    pub(super) contamination_tick_limit: u64,
    pub(super) minimum_accepted_pairs: usize,
    pub(super) minimum_load_generator_headroom_ratio: f64,
    pub(super) rotating_order: bool,
    pub(super) schedule_fingerprint_sha256: String,
}

#[derive(Serialize)]
pub(super) struct PairEvidence {
    pub(super) index: usize,
    pub(super) order: Vec<&'static str>,
    pub(super) accepted: bool,
    pub(super) contamination: super::process::ContaminationEvidence,
    pub(super) aot_report_path: String,
    pub(super) axum_report_path: String,
    pub(super) hyper_report_path: String,
    pub(super) axum_load_generator_headroom: Option<LoadGeneratorHeadroom>,
    pub(super) hyper_load_generator_headroom: Option<LoadGeneratorHeadroom>,
    pub(super) aot: Value,
    pub(super) axum: Value,
    pub(super) hyper: Value,
}

#[derive(Serialize)]
pub(super) struct LoadGeneratorHeadroom {
    pub(super) aot_external_requests_per_second: f64,
    pub(super) aot_internal_persistent_requests_per_second: f64,
    pub(super) aot_headroom_ratio: f64,
    pub(super) axum_external_requests_per_second: f64,
    pub(super) axum_internal_persistent_requests_per_second: f64,
    pub(super) axum_headroom_ratio: f64,
    pub(super) status: &'static str,
}

#[derive(Serialize)]
pub(super) struct Comparison {
    pub(super) baseline: &'static str,
    pub(super) samples: usize,
    pub(super) aot_median_requests_per_second: f64,
    pub(super) baseline_median_requests_per_second: f64,
    pub(super) median_aot_to_baseline_ratio: f64,
    pub(super) minimum_aot_to_baseline_ratio: f64,
    pub(super) maximum_aot_to_baseline_ratio: f64,
    pub(super) ratio_standard_deviation: f64,
    pub(super) ratio_95_percent_interval: [f64; 2],
    pub(super) confidence_method: &'static str,
    pub(super) bootstrap_samples: usize,
    pub(super) aot_wins: usize,
    pub(super) verdict: &'static str,
    pub(super) suspected_subsystem: Option<&'static str>,
    pub(super) next_optimization_hypothesis: Option<&'static str>,
}

#[derive(Serialize)]
pub(super) struct IsolationEvidence {
    pub(super) name: String,
    pub(super) command: String,
    pub(super) output: String,
    pub(super) status: String,
}
