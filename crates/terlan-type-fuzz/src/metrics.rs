//! Deterministic aggregate metrics for one executable fuzz profile.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::{CaseManifest, Oracle};

const METRICS_SCHEMA: &str = "terlan.type-fuzz-profile-metrics.v1";

pub(crate) struct RunInput {
    pub label: String,
    pub root: PathBuf,
}

#[derive(Default)]
struct Aggregate {
    cases: u64,
    seeds: BTreeSet<u64>,
    families: BTreeMap<String, u64>,
    features: BTreeMap<String, u64>,
    ordered_pairs: BTreeSet<String>,
    unordered_pairs: BTreeSet<String>,
    oracles: BTreeMap<String, u64>,
    outcomes: BTreeMap<String, u64>,
    failures: BTreeMap<String, u64>,
    timing_events: u64,
    wall_total_ms: u64,
    wall_max_ms: u64,
    peak_rss_bytes: u64,
    cache_hits: u64,
    cache_misses: u64,
    cache_miss_reasons: BTreeMap<String, u64>,
    reductions: u64,
    reduced_failures: BTreeSet<String>,
}

pub(crate) fn publish(
    root: &Path,
    profile: &str,
    compiler_revision: &str,
    status: &str,
    runs: &[RunInput],
) -> Result<(), String> {
    let mut aggregate = Aggregate::default();
    for run in runs {
        collect_run(run, &mut aggregate)?;
    }
    let report = json!({
        "schema": METRICS_SCHEMA,
        "profile": profile,
        "compiler_revision": compiler_revision,
        "status": status,
        "runs": runs.iter().map(|run| run.label.as_str()).collect::<Vec<_>>(),
        "cases": aggregate.cases,
        "seeds": aggregate.seeds,
        "families": aggregate.families,
        "features": aggregate.features,
        "feature_pairs": {
            "ordered": aggregate.ordered_pairs,
            "unordered": aggregate.unordered_pairs,
        },
        "oracles": aggregate.oracles,
        "outcomes": aggregate.outcomes,
        "failures": aggregate.failures,
        "timing": {
            "events": aggregate.timing_events,
            "case_wall_total_ms": aggregate.wall_total_ms,
            "case_wall_max_ms": aggregate.wall_max_ms,
        },
        "rss": {"peak_bytes": aggregate.peak_rss_bytes},
        "cache": {
            "hits": aggregate.cache_hits,
            "misses": aggregate.cache_misses,
            "miss_reasons": aggregate.cache_miss_reasons,
        },
        "reductions": {
            "count": aggregate.reductions,
            "unique_failure_classes": aggregate.reduced_failures,
        },
    });
    let mut bytes = serde_json::to_vec(&report).expect("JSON values serialize to memory");
    bytes.push(b'\n');
    crate::executor::write_artifact(&root.join("metrics.json"), &bytes)
}

fn collect_run(run: &RunInput, aggregate: &mut Aggregate) -> Result<(), String> {
    let mut entries = fs::read_dir(&run.root)
        .map_err(|error| format!("read fuzz metric run `{}`: {error}", run.root.display()))?;
    collect_run_entries(run, aggregate, &mut entries)
}

fn collect_run_entries(
    run: &RunInput,
    aggregate: &mut Aggregate,
    entries: &mut dyn Iterator<Item = std::io::Result<fs::DirEntry>>,
) -> Result<(), String> {
    for entry in entries {
        let path = metric_entry_path(entry)?;
        if path.is_dir() && path.join("case.toml").is_file() {
            collect_case(&path, aggregate)?;
        }
    }
    if let Some(planning) = read_json(&run.root.join("planning/comparison.json"))? {
        collect_cache(&planning, aggregate);
    }
    Ok(())
}

fn metric_entry_path(entry: std::io::Result<fs::DirEntry>) -> Result<PathBuf, String> {
    entry
        .map(|entry| entry.path())
        .map_err(|error| format!("read fuzz metric entry: {error}"))
}

fn collect_case(root: &Path, aggregate: &mut Aggregate) -> Result<(), String> {
    let manifest_path = root.join("case.toml");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "read fuzz metric case `{}`: {error}",
            manifest_path.display()
        )
    })?;
    let manifest: CaseManifest = toml::from_str(&manifest_text).map_err(|error| {
        format!(
            "invalid fuzz metric case `{}`: {error}",
            manifest_path.display()
        )
    })?;
    aggregate.cases += 1;
    aggregate.seeds.insert(manifest.seed);
    increment(&mut aggregate.families, &manifest.family, 1);
    increment(&mut aggregate.oracles, oracle_name(manifest.oracle), 1);
    collect_features(&manifest.family, aggregate)?;
    if let Some(observed) = read_json(&root.join("observed.json"))? {
        collect_observation(&observed, aggregate);
    }
    if let Some(resources) = read_json(&root.join("resources.json"))? {
        let wall = number(&resources, "wall_elapsed_ms");
        aggregate.wall_total_ms = aggregate.wall_total_ms.saturating_add(wall);
        aggregate.wall_max_ms = aggregate.wall_max_ms.max(wall);
        aggregate.peak_rss_bytes = aggregate
            .peak_rss_bytes
            .max(number(&resources, "peak_rss_bytes"));
    }
    if let Some(timings) = read_json(&root.join("timings.json"))? {
        aggregate.timing_events = aggregate.timing_events.saturating_add(
            timings["events"]
                .as_array()
                .map_or(0, |events| events.len() as u64),
        );
    }
    if let Some(reduction) = read_json(&root.join("reduction.json"))? {
        aggregate.reductions += 1;
        if let Some(diagnostic) = reduction["failure"]["diagnostic"].as_str() {
            aggregate.reduced_failures.insert(diagnostic.to_string());
        }
    }
    Ok(())
}

fn collect_features(family: &str, aggregate: &mut Aggregate) -> Result<(), String> {
    let features = crate::generate::family_features(family)?;
    for feature in features {
        increment(&mut aggregate.features, feature, 1);
    }
    for (left_index, left) in features.iter().enumerate() {
        for right in &features[left_index + 1..] {
            aggregate.ordered_pairs.insert(format!("{left}->{right}"));
            let (first, second) = unordered_pair(left, right);
            aggregate
                .unordered_pairs
                .insert(format!("{first}+{second}"));
        }
    }
    Ok(())
}

fn unordered_pair<'a>(left: &'a str, right: &'a str) -> (&'a str, &'a str) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn collect_observation(value: &Value, aggregate: &mut Aggregate) {
    let outcome = value["outcome"].as_str().unwrap_or("missing");
    increment(&mut aggregate.outcomes, outcome, 1);
    if !matches!(outcome, "passed" | "not_run") {
        let diagnostic = value["stderr"]
            .as_str()
            .and_then(diagnostic_class)
            .unwrap_or("unclassified");
        increment(&mut aggregate.failures, diagnostic, 1);
    }
}

fn collect_cache(value: &Value, aggregate: &mut Aggregate) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                match key.as_str() {
                    "native_image_cache_hits" => {
                        aggregate.cache_hits = aggregate
                            .cache_hits
                            .saturating_add(child.as_u64().unwrap_or(0));
                    }
                    "native_image_cache_misses" => {
                        aggregate.cache_misses = aggregate
                            .cache_misses
                            .saturating_add(child.as_u64().unwrap_or(0));
                    }
                    "cache_miss_reasons" => {
                        if let Some(reasons) = child.as_object() {
                            for (reason, count) in reasons {
                                increment(
                                    &mut aggregate.cache_miss_reasons,
                                    reason,
                                    count.as_u64().unwrap_or(0),
                                );
                            }
                        }
                    }
                    _ => collect_cache(child, aggregate),
                }
            }
        }
        Value::Array(children) => {
            for child in children {
                collect_cache(child, aggregate);
            }
        }
        _ => {}
    }
}

fn read_json(path: &Path) -> Result<Option<Value>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    parse_json_result(path, fs::read_to_string(path)).map(Some)
}

fn attributed_read(path: &Path, result: std::io::Result<String>) -> Result<String, String> {
    result.map_err(|error| format!("read fuzz metric artifact `{}`: {error}", path.display()))
}

fn parse_json_result(path: &Path, result: std::io::Result<String>) -> Result<Value, String> {
    attributed_read(path, result).and_then(|text| {
        serde_json::from_str(&text)
            .map_err(|error| format!("invalid fuzz metric artifact `{}`: {error}", path.display()))
    })
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}

fn diagnostic_class(stderr: &str) -> Option<&str> {
    let start = stderr.find("error[")?;
    let suffix = &stderr[start..];
    let end = suffix.find(']')?;
    Some(&suffix[..=end])
}

fn increment(counts: &mut BTreeMap<String, u64>, key: &str, amount: u64) {
    let count = counts.entry(key.to_string()).or_default();
    *count = count.saturating_add(amount);
}

fn oracle_name(oracle: Oracle) -> &'static str {
    match oracle {
        Oracle::Check => "check",
        Oracle::JavaScript => "javascript",
        Oracle::Native => "native",
        Oracle::None => "none",
    }
}

#[cfg(test)]
#[path = "metrics_test.rs"]
mod tests;
