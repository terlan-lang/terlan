use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use regex::Regex;

use crate::{render_failure, QualityResult};

/// Summary produced by the CLI exact-selector check.
///
/// Inputs:
/// - `selector_count`: number of exact selectors referenced by `cli.mk`.
///
/// Output:
/// - Stable success metric rendered by the command-line wrapper.
///
/// Transformation:
/// - Keeps the checked selector count separate from failure diagnostics so CI
///   output stays concise on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliExactSelectorSummary {
    pub selector_count: usize,
}

/// Runs the CLI exact-test selector validation.
///
/// Inputs:
/// - `root`: repository root containing `crates/terlan_cli/cli.mk`.
///
/// Output:
/// - Success summary when every exact selector resolves to a Cargo test.
/// - Diagnostics when any selector is stale or Cargo test discovery fails.
///
/// Transformation:
/// - Extracts `TERLC_EXACT_TEST` selectors from the CLI Makefile.
/// - Discovers current `terlan_cli` tests using Cargo's `--list` mode.
/// - Compares the two sets so Make gates cannot silently drift after test
///   extraction or module renames.
pub fn run_cli_exact_selectors(root: &Path) -> QualityResult<CliExactSelectorSummary> {
    let makefile = root.join("crates/terlan_cli/cli.mk");
    let makefile_text = fs::read_to_string(&makefile)
        .map_err(|err| format!("{}: failed to read CLI Makefile: {err}", makefile.display()))?;
    let selectors = extract_cli_exact_selectors(&makefile_text)?;
    let tests = cargo_test_names(root)?;
    let missing = stale_selectors(&selectors, &tests);

    if !missing.is_empty() {
        return Err(render_failure("cli-exact-selector", &missing));
    }

    Ok(CliExactSelectorSummary {
        selector_count: selectors.len(),
    })
}

/// Extracts exact-test selectors from CLI Makefile text.
///
/// Inputs:
/// - `makefile_text`: contents of `crates/terlan_cli/cli.mk`.
///
/// Output:
/// - Ordered selector strings as accepted by `cargo test -- --exact`.
///
/// Transformation:
/// - Applies the same Make-recipe regex as the original Python gate without
///   interpreting shell commands or Make variables.
pub(crate) fn extract_cli_exact_selectors(makefile_text: &str) -> QualityResult<Vec<String>> {
    let pattern = Regex::new(r"TERLC_EXACT_TEST\)\s+([^\s]+)\s+--\s+--exact")
        .map_err(|err| format!("failed to compile exact-selector regex: {err}"))?;
    Ok(pattern
        .captures_iter(makefile_text)
        .filter_map(|capture| capture.get(1).map(|selector| selector.as_str().to_owned()))
        .collect())
}

/// Discovers current `terlan_cli` test names using Cargo.
///
/// Inputs:
/// - `root`: repository root used as Cargo's working directory.
///
/// Output:
/// - Set of fully qualified test names reported by Cargo.
///
/// Transformation:
/// - Runs `cargo test -p terlan_cli -- --list`.
/// - Parses the standard test-list output into exact-selector names.
fn cargo_test_names(root: &Path) -> QualityResult<BTreeSet<String>> {
    let output = Command::new("cargo")
        .args(["test", "-p", "terlan_cli", "--", "--list"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run cargo test list for terlan_cli: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "[cli-exact-selector] failed to list terlan_cli tests:\n{}",
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_cargo_test_names(&stdout))
}

/// Parses Cargo test-list output into exact test names.
///
/// Inputs:
/// - `stdout`: text emitted by `cargo test -- --list`.
///
/// Output:
/// - Set of names from lines containing `: test`.
///
/// Transformation:
/// - Mirrors Cargo's stable test-list shape by keeping the text before the
///   first `: test` marker.
pub(crate) fn parse_cargo_test_names(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .filter_map(|line| {
            line.split_once(": test")
                .map(|(name, _)| name.trim().to_owned())
        })
        .collect()
}

/// Returns exact selectors that do not resolve to current tests.
///
/// Inputs:
/// - `selectors`: ordered exact selectors from the CLI Makefile.
/// - `tests`: Cargo's current fully qualified test names.
///
/// Output:
/// - Ordered stale selector diagnostics.
///
/// Transformation:
/// - Filters selectors not present in Cargo's test-name set while preserving
///   Makefile order for actionable diagnostics.
pub(crate) fn stale_selectors(selectors: &[String], tests: &BTreeSet<String>) -> Vec<String> {
    selectors
        .iter()
        .filter(|selector| !tests.contains(*selector))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "cli_exact_selectors_test.rs"]
mod cli_exact_selectors_test;
