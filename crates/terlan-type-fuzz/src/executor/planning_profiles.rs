use std::path::{Path, PathBuf};

use super::metrics;
use crate::{executor::oracle, Oracle, RunOptions};

const BROAD_WALL_TOLERANCE_MS: u64 = 1_000;
const BROAD_RSS_TOLERANCE_BYTES: u64 = 16 * 1024 * 1024;
const PLANNING_WALL_LIMIT_MS: u64 = 60_000;
const PLANNING_RSS_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

pub(super) fn run_source(
    profile: &str,
    compiler: &Path,
    source: &Path,
    wall_ms: u64,
    options: &RunOptions,
) -> Result<oracle::OracleResult, String> {
    let result = oracle::run(
        compiler,
        Oracle::Native,
        source,
        planning_wall_limit(wall_ms),
        planning_rss_limit(options.rss_limit_bytes),
        options.cpu_limit_ms,
    );
    ensure_passed(profile, &result)?;
    ensure_attributed(profile, &result)?;
    Ok(result)
}

pub(super) fn run_batch(
    profile: &str,
    compiler: &Path,
    root: &Path,
    sources: &[PathBuf],
    wall_ms: u64,
    options: &RunOptions,
) -> Result<oracle::OracleResult, String> {
    let result = oracle::run_batch(
        compiler,
        Oracle::Native,
        root,
        sources,
        planning_wall_limit(wall_ms),
        planning_rss_limit(options.rss_limit_bytes),
        options.cpu_limit_ms,
    );
    ensure_passed(profile, &result)?;
    ensure_attributed(profile, &result)?;
    Ok(result)
}

pub(super) fn ensure_warm_reuse(
    profile: &str,
    result: &oracle::OracleResult,
) -> Result<(), String> {
    if metrics::has_native_image_cache_hit(&metrics::measure(&result.stderr)) {
        Ok(())
    } else {
        Err(format!(
            "{profile} did not reuse a verified native image cache entry"
        ))
    }
}

fn ensure_attributed(profile: &str, result: &oracle::OracleResult) -> Result<(), String> {
    let gaps = metrics::attribution_gaps(&metrics::measure(&result.stderr));
    if gaps.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{profile} native planning telemetry is incomplete: {}",
            gaps.join(", ")
        ))
    }
}

pub(super) fn ensure_broad_regression_absent(
    broad: &oracle::OracleResult,
    narrow: &oracle::OracleResult,
) -> Result<(), String> {
    if !metrics::same_structural_work(
        &metrics::measure(&broad.stderr),
        &metrics::measure(&narrow.stderr),
    ) {
        return Err("unreachable native provider added compiler work".into());
    }
    if !within_increase(
        broad.resources.wall_elapsed_ms,
        narrow.resources.wall_elapsed_ms,
        BROAD_WALL_TOLERANCE_MS,
    ) {
        return Err("unreachable native provider exceeded wall tolerance".into());
    }
    if !within_increase(
        broad.resources.peak_rss_bytes,
        narrow.resources.peak_rss_bytes,
        BROAD_RSS_TOLERANCE_BYTES,
    ) {
        return Err("unreachable native provider exceeded RSS tolerance".into());
    }
    Ok(())
}

pub(super) fn ensure_passed(profile: &str, result: &oracle::OracleResult) -> Result<(), String> {
    if result.outcome == "passed" {
        Ok(())
    } else {
        let evidence = serde_json::json!({
            "schema": "terlan.type-fuzz-planning-failure.v1",
            "profile": profile,
            "outcome": result.outcome,
            "resources": result.resources,
            "compiler": metrics::measure(&result.stderr),
            "diagnostic": result.stderr,
        });
        Err(format!(
            "{profile} native planning probe failed: {evidence}"
        ))
    }
}

pub(super) fn planning_wall_limit(requested: u64) -> u64 {
    requested.min(PLANNING_WALL_LIMIT_MS)
}

pub(super) fn planning_rss_limit(requested: u64) -> u64 {
    requested.min(PLANNING_RSS_LIMIT_BYTES)
}

pub(super) fn within_increase(right: Option<u64>, left: Option<u64>, tolerance: u64) -> bool {
    match (right, left) {
        (Some(right), Some(left)) => right <= left.saturating_add(tolerance),
        (None, None) => true,
        _ => false,
    }
}
