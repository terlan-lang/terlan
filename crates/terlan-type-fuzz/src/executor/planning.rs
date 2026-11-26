use std::fs;
use std::path::{Path, PathBuf};

use super::oracle::OracleResult;
use crate::{Oracle, RunOptions};

#[path = "planning_metrics.rs"]
mod metrics;
#[path = "planning_profiles.rs"]
mod profiles;

const NARROW_SOURCE: &str = "src/type_fuzz/narrow/NarrowTest.terl";
const COMBINED_FIRST: &str = "src/type_fuzz/combined/FirstTest.terl";
const COMBINED_SECOND: &str = "src/type_fuzz/combined/SecondTest.terl";
const BROAD_SOURCE: &str = "src/type_fuzz/broad/BroadTest.terl";

pub(super) fn run(options: &RunOptions, compiler: &Path, remaining_ms: u64) -> Result<(), String> {
    if options.oracle != Oracle::Native {
        return Ok(());
    }
    let root = options.out_dir.join("planning");
    reset_root(&root)?;
    let narrow = root.join("narrow");
    let combined = root.join("combined");
    let broad = root.join("broad");
    write_fixtures(&narrow, &combined, &broad)?;
    let wall_ms = options.case_timeout_ms.min(remaining_ms / 6).max(1);
    let narrow_cold = profiles::run_source(
        "narrow cold",
        compiler,
        &narrow.join(NARROW_SOURCE),
        wall_ms,
        options,
    )?;
    let narrow_warm = profiles::run_source(
        "narrow warm",
        compiler,
        &narrow.join(NARROW_SOURCE),
        wall_ms,
        options,
    )?;
    let combined_sources = [
        PathBuf::from(COMBINED_FIRST),
        PathBuf::from(COMBINED_SECOND),
    ];
    let combined_cold = profiles::run_batch(
        "combined cold",
        compiler,
        &combined,
        &combined_sources,
        wall_ms,
        options,
    )?;
    let combined_warm = profiles::run_batch(
        "combined warm",
        compiler,
        &combined,
        &combined_sources,
        wall_ms,
        options,
    )?;
    let broad_cold = profiles::run_source(
        "broad cold",
        compiler,
        &broad.join(BROAD_SOURCE),
        wall_ms,
        options,
    )?;
    let broad_warm = profiles::run_source(
        "broad warm",
        compiler,
        &broad.join(BROAD_SOURCE),
        wall_ms,
        options,
    )?;
    profiles::ensure_broad_regression_absent(&broad_cold, &narrow_cold)?;
    let evidence = serde_json::json!({
        "schema": "terlan.type-fuzz-planning.v1",
        "narrow": {
            "cold": observation(&narrow_cold),
            "warm": observation(&narrow_warm),
        },
        "broad_unreachable_native": {
            "cold": observation(&broad_cold),
            "warm": observation(&broad_warm),
        },
        "combined_root": {
            "cold": observation(&combined_cold),
            "warm": observation(&combined_warm),
        },
        "delta": {
            "cold_broad_minus_narrow": comparison(&broad_cold, &narrow_cold),
            "warm_broad_minus_narrow": comparison(&broad_warm, &narrow_warm),
            "narrow_warm_minus_cold": comparison(&narrow_warm, &narrow_cold),
            "broad_warm_minus_cold": comparison(&broad_warm, &broad_cold),
            "combined_cold_minus_narrow": comparison(&combined_cold, &narrow_cold),
            "combined_warm_minus_narrow": comparison(&combined_warm, &narrow_warm),
            "combined_warm_minus_cold": comparison(&combined_warm, &combined_cold),
        }
    });
    publish(&root, &evidence)
}

fn write_fixtures(narrow: &Path, combined: &Path, broad: &Path) -> Result<(), String> {
    write_fixture(narrow, false)?;
    write_combined_fixture(combined)?;
    write_fixture(broad, true)
}

fn write_combined_fixture(root: &Path) -> Result<(), String> {
    write(
        root.join("terlan.toml"),
        concat!(
            "[package]\nname = \"type-fuzz-combined\"\nversion = \"0.0.0\"\n",
            "namespace = \"type_fuzz.combined\"\n\n[build]\nsource_roots = [\"src\"]\n",
            "artifact = \"library\"\n",
        ),
    )?;
    write(
        root.join(COMBINED_FIRST),
        "module type_fuzz.combined.FirstTest.\n\n@test\npub first_is_stable(): Bool ->\n    7 == 7.\n",
    )?;
    write(
        root.join(COMBINED_SECOND),
        "module type_fuzz.combined.SecondTest.\n\n@test\npub second_is_stable(): Bool ->\n    9 == 9.\n",
    )
}

fn reset_root(root: &Path) -> Result<(), String> {
    if root.exists() {
        fs::remove_dir_all(root).map_err(|error| format!("reset planning fixtures: {error}"))?;
    }
    Ok(())
}

fn publish(root: &Path, evidence: &serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(&evidence).expect("planning evidence JSON serializes");
    fs::write(root.join("comparison.json"), bytes)
        .map_err(|error| format!("write planning comparison: {error}"))
}

fn observation(result: &OracleResult) -> serde_json::Value {
    let compiler = metrics::measure(&result.stderr);
    serde_json::json!({
        "outcome": result.outcome,
        "wall_elapsed_ms": result.resources.wall_elapsed_ms,
        "peak_rss_bytes": result.resources.peak_rss_bytes,
        "peak_cpu_ms": result.resources.peak_cpu_ms,
        "timing_events": timing_count(&result.stderr),
        "native_units_built": prefix_count(
            &result.stderr,
            "terlc native-aot: module-unit.begin.",
        ),
        "compiler": compiler,
    })
}

fn comparison(right: &OracleResult, left: &OracleResult) -> serde_json::Value {
    serde_json::json!({
        "wall_elapsed_ms": signed_delta(
            right.resources.wall_elapsed_ms,
            left.resources.wall_elapsed_ms,
        ),
        "peak_rss_bytes": signed_delta(
            right.resources.peak_rss_bytes,
            left.resources.peak_rss_bytes,
        ),
        "timing_events": signed_count_delta(&right.stderr, &left.stderr),
        "native_units_built": signed_prefix_delta(
            &right.stderr,
            &left.stderr,
            "terlc native-aot: module-unit.begin.",
        ),
        "compiler_work": metrics::delta(
            &metrics::measure(&right.stderr),
            &metrics::measure(&left.stderr),
        ),
    })
}

fn signed_delta(right: Option<u64>, left: Option<u64>) -> Option<i128> {
    Some(i128::from(right?) - i128::from(left?))
}

fn signed_count_delta(right: &str, left: &str) -> i128 {
    count_delta(timing_count(right), timing_count(left))
}

fn signed_prefix_delta(right: &str, left: &str, prefix: &str) -> i128 {
    count_delta(prefix_count(right, prefix), prefix_count(left, prefix))
}

fn count_delta(right: usize, left: usize) -> i128 {
    right as i128 - left as i128
}

fn timing_count(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|line| line.starts_with("terlc timing:") || line.starts_with("terlc native-aot:"))
        .count()
}

fn prefix_count(stderr: &str, prefix: &str) -> usize {
    stderr
        .lines()
        .filter(|line| line.starts_with(prefix))
        .count()
}

fn write_fixture(root: &Path, broad: bool) -> Result<(), String> {
    let (source, manifest) = if broad {
        (
            BROAD_SOURCE,
            concat!(
                "[package]\nname = \"type-fuzz-broad\"\nversion = \"0.0.0\"\n",
                "namespace = \"type_fuzz.broad\"\n\n[build]\nsource_roots = [\"src\"]\n",
                "artifact = \"library\"\n\n[dependencies]\n",
                "unused-native = { path = \"unused-native\" }\n",
            ),
        )
    } else {
        (
            NARROW_SOURCE,
            concat!(
                "[package]\nname = \"type-fuzz-narrow\"\nversion = \"0.0.0\"\n",
                "namespace = \"type_fuzz.narrow\"\n\n[build]\nsource_roots = [\"src\"]\n",
                "artifact = \"library\"\n",
            ),
        )
    };
    write(root.join("terlan.toml"), manifest)?;
    let module = if broad {
        "type_fuzz.broad.BroadTest"
    } else {
        "type_fuzz.narrow.NarrowTest"
    };
    write(
        root.join(source),
        &format!("module {module}.\n\n@test\npub value_is_stable(): Bool ->\n    7 == 7.\n"),
    )?;
    if broad {
        write(
            root.join("unused-native/terlan.toml"),
            concat!(
                "[package]\nname = \"unused-native\"\nversion = \"0.0.0\"\n",
                "namespace = \"type_fuzz.unused_native\"\n\n[build]\n",
                "source_roots = [\"src\"]\nartifact = \"library\"\n\n[native.rust]\n",
                "crate = \"type_fuzz_unused_native\"\npath = \"native\"\n",
                "helper = \"type-fuzz-unused-native-helper\"\n",
                "helper_env = \"TERLAN_TYPE_FUZZ_UNUSED_NATIVE_HELPER\"\n",
            ),
        )?;
        write(
            root.join("unused-native/src/type_fuzz/unused_native/Unused.terl"),
            "module type_fuzz.unused_native.Unused.\n\npub unused(): Int ->\n    9.\n",
        )?;
    }
    Ok(())
}

fn write(path: PathBuf, contents: &str) -> Result<(), String> {
    let parent = path.parent().expect("planning fixture path has a parent");
    fs::create_dir_all(parent).map_err(|error| format!("create planning fixture: {error}"))?;
    fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
#[path = "planning_test.rs"]
mod tests;

#[cfg(test)]
#[path = "planning_cache_test.rs"]
mod cache_tests;
