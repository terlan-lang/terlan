use std::fs;
use std::path::{Path, PathBuf};

use super::oracle::{self, OracleResult};
use crate::{Oracle, RunOptions};

const NARROW_SOURCE: &str = "src/type_fuzz/narrow/NarrowTest.terl";
const BROAD_SOURCE: &str = "src/type_fuzz/broad/BroadTest.terl";

pub(super) fn run(options: &RunOptions, compiler: &Path, remaining_ms: u64) -> Result<(), String> {
    if options.oracle != Oracle::Native {
        return Ok(());
    }
    let root = options.out_dir.join("planning");
    let narrow = root.join("narrow");
    let broad = root.join("broad");
    write_fixture(&narrow, false)?;
    write_fixture(&broad, true)?;
    let wall_ms = options.case_timeout_ms.min(remaining_ms / 2).max(1);
    let narrow_result = oracle::run(
        compiler,
        Oracle::Native,
        &narrow.join(NARROW_SOURCE),
        wall_ms,
        options.rss_limit_bytes,
        options.cpu_limit_ms,
    );
    ensure_passed("narrow", &narrow_result)?;
    let broad_result = oracle::run(
        compiler,
        Oracle::Native,
        &broad.join(BROAD_SOURCE),
        wall_ms,
        options.rss_limit_bytes,
        options.cpu_limit_ms,
    );
    ensure_passed("broad", &broad_result)?;
    let evidence = serde_json::json!({
        "schema": "terlan.type-fuzz-planning.v1",
        "narrow": observation(&narrow_result),
        "broad_unreachable_native": observation(&broad_result),
        "delta": {
            "wall_elapsed_ms": signed_delta(
                broad_result.resources.wall_elapsed_ms,
                narrow_result.resources.wall_elapsed_ms,
            ),
            "peak_rss_bytes": signed_delta(
                broad_result.resources.peak_rss_bytes,
                narrow_result.resources.peak_rss_bytes,
            ),
            "timing_events": signed_count_delta(&broad_result.stderr, &narrow_result.stderr),
            "native_units_built": signed_prefix_delta(
                &broad_result.stderr,
                &narrow_result.stderr,
                "terlc native-aot: module-unit.begin.",
            ),
        }
    });
    let bytes = serde_json::to_vec(&evidence).expect("planning evidence JSON serializes");
    fs::write(root.join("comparison.json"), bytes)
        .map_err(|error| format!("write planning comparison: {error}"))
}

fn ensure_passed(profile: &str, result: &OracleResult) -> Result<(), String> {
    if result.outcome == "passed" {
        Ok(())
    } else {
        Err(format!(
            "{profile} native planning probe ended as {}: {}",
            result.outcome, result.stderr
        ))
    }
}

fn observation(result: &OracleResult) -> serde_json::Value {
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
