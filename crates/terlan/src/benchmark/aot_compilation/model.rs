//! Typed report model for AOT compilation benchmarks.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::super::hardware::HardwareFingerprint;
use super::percentile;

/// Complete same-machine Terlan and Go compilation report.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CompilationBenchmarkReport {
    /// Versioned report schema.
    pub(crate) schema: String,
    /// Stable completion state.
    pub(crate) status: String,
    /// Unix timestamp when recording completed.
    pub(crate) recorded_unix_seconds: u64,
    /// Machine identity shared by every sample.
    pub(crate) hardware: HardwareFingerprint,
    /// Compiler and reference toolchain identities.
    pub(crate) toolchains: CompilationToolchains,
    /// Content identity of the equivalent source fixtures.
    pub(crate) fixtures: CompilationFixtureIdentity,
    /// Number of samples recorded per timing row.
    pub(crate) sample_count: usize,
    /// Explicit cache-state semantics for cold and warm rows.
    pub(crate) cache_state: CompilationCacheState,
    /// Canonically ordered benchmark measurements.
    pub(crate) measurements: Vec<CompilationMeasurement>,
}

/// Toolchain versions and compiler binary identity used by one report.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CompilationToolchains {
    /// Active Rust compiler version inherited by the Terlan toolchain.
    pub(crate) rustc: String,
    /// Active Go compiler version.
    pub(crate) go: String,
    /// Canonical Terlan compiler executable path.
    pub(crate) terlc_path: String,
    /// SHA-256 of the measured Terlan compiler executable.
    pub(crate) terlc_sha256: String,
}

/// Stable source identity for the complete equivalent fixture tree.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CompilationFixtureIdentity {
    /// Repository-relative fixture directory.
    pub(crate) path: String,
    /// SHA-256 over sorted relative paths and file contents.
    pub(crate) sha256: String,
    /// Workloads guaranteed by the fixture tree.
    pub(crate) workloads: Vec<String>,
}

/// Cache semantics required to interpret the timing rows.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CompilationCacheState {
    /// Terlan cold-build cache policy.
    pub(crate) terlan_cold: String,
    /// Go cold-build cache policy.
    pub(crate) go_cold: String,
    /// Warm-build cache policy shared by both lanes.
    pub(crate) warm: String,
    /// Whether package download time is inside timed regions.
    pub(crate) dependency_downloads_timed: bool,
}

/// One named benchmark scenario with optional Go comparison.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CompilationMeasurement {
    /// Stable scenario identity.
    pub(crate) name: String,
    /// Exact work included in the timed region.
    pub(crate) scope: String,
    /// Terlan timing summary.
    pub(crate) terlan: CompilationTiming,
    /// Equivalent Go timing when the Go tool exposes the same operation.
    pub(crate) go: Option<CompilationTiming>,
    /// Terlan median divided by Go median.
    pub(crate) median_ratio: Option<f64>,
    /// Terlan p95 divided by Go p95.
    pub(crate) p95_ratio: Option<f64>,
    /// Explanation when no honest Go comparison exists.
    pub(crate) reference_note: Option<String>,
}

/// Sorted latency summary for one compiler and scenario.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CompilationTiming {
    /// Sorted individual wall-clock samples in nanoseconds.
    pub(crate) samples_ns: Vec<u128>,
    /// Nearest-rank median in nanoseconds.
    pub(crate) median_ns: u128,
    /// Nearest-rank 95th percentile in nanoseconds.
    pub(crate) p95_ns: u128,
    /// Minimum sample in nanoseconds.
    pub(crate) min_ns: u128,
    /// Maximum sample in nanoseconds.
    pub(crate) max_ns: u128,
}

impl CompilationTiming {
    /// Summarizes a non-empty duration set in deterministic sorted order.
    pub(super) fn from_durations(samples: Vec<Duration>) -> Result<Self, String> {
        if samples.is_empty() {
            return Err("compilation benchmark requires at least one sample".to_string());
        }
        let mut samples_ns = samples
            .into_iter()
            .map(|duration| duration.as_nanos().max(1))
            .collect::<Vec<_>>();
        samples_ns.sort_unstable();
        let median_ns = percentile(&samples_ns, 50);
        let p95_ns = percentile(&samples_ns, 95);
        Ok(Self {
            min_ns: samples_ns[0],
            max_ns: samples_ns[samples_ns.len() - 1],
            samples_ns,
            median_ns,
            p95_ns,
        })
    }
}
