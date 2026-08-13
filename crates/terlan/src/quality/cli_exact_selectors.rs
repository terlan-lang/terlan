use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::terlan_quality::{render_failure, QualityResult};

const EXACT_SELECTOR_SOURCES: &[&str] = &["crates/terlan/cli.mk", "Makefile"];

const REQUIRED_EXACT_SELECTORS: &[&str] = &[
    "runtime::vm::http::http_test::transport_fixtures::vm_http_roundtrips_request_and_response_over_vm_tcp_streams",
    "commands::serve::serve_test::dynamic_dispatch::vm_stream_request_executes_dynamic_handler_without_hyper",
    "commands::serve::serve_test::upgrades_and_acme::vm_stream_request_returns_websocket_upgrade_handshake_without_hyper",
];

/// Summary produced by the CLI exact-selector check.
///
/// Inputs:
/// - `selector_count`: number of exact selectors referenced by release gates.
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
/// - `root`: repository root containing release Makefiles.
///
/// Output:
/// - Success summary when every exact selector resolves to a Cargo test.
/// - Diagnostics when any selector is stale or Cargo test discovery fails.
///
/// Transformation:
/// - Extracts exact test selectors from the CLI Makefile and root Makefile.
/// - Discovers current `terlan` tests using Cargo's `--list` mode.
/// - Compares the two sets so Make gates cannot silently drift after test
///   extraction or module renames.
pub fn run_cli_exact_selectors(root: &Path) -> QualityResult<CliExactSelectorSummary> {
    let mut selectors = Vec::new();
    let mut grouped_filters = Vec::new();
    for source in EXACT_SELECTOR_SOURCES {
        let path = root.join(source);
        let text = fs::read_to_string(&path).map_err(|err| {
            format!(
                "{}: failed to read exact-selector source: {err}",
                path.display()
            )
        })?;
        selectors.extend(extract_cli_exact_selectors(&text)?);
        grouped_filters.extend(extract_grouped_test_filters(&text));
    }
    let tests = cargo_test_names(root)?;
    let mut missing = stale_selectors(&selectors, &tests);
    missing.extend(stale_grouped_filters(&grouped_filters, &tests));
    missing.extend(missing_required_test_coverage(&selectors, &grouped_filters));

    if !missing.is_empty() {
        return Err(render_failure("cli-exact-selector", &missing));
    }

    Ok(CliExactSelectorSummary {
        selector_count: selectors.len(),
    })
}

/// Extracts module-level Cargo test filters from Make recipes.
pub(crate) fn extract_grouped_test_filters(makefile_text: &str) -> Vec<String> {
    makefile_text
        .lines()
        .filter(|line| line.contains("RUST_TEST"))
        .filter_map(cargo_test_filter_from_make_recipe)
        .collect()
}

fn cargo_test_filter_from_make_recipe(line: &str) -> Option<String> {
    const OPTIONS_WITH_VALUE: &[&str] = &[
        "-p",
        "--package",
        "--features",
        "--bin",
        "--test",
        "--target",
        "--manifest-path",
        "--jobs",
        "-j",
        "--profile",
    ];

    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if !tokens.contains(&"--lib") || tokens.contains(&"--test") {
        return None;
    }
    let mut index = tokens
        .iter()
        .position(|token| token.contains("RUST_TEST)"))?
        + 1;
    while index < tokens.len() {
        let token = tokens[index];
        if matches!(token, "--" | "\\") {
            return None;
        }
        if OPTIONS_WITH_VALUE.contains(&token) {
            index += 2;
            continue;
        }
        if token.starts_with('-') || token.contains('=') {
            index += 1;
            continue;
        }
        return Some(token.to_string());
    }
    None
}

/// Extracts exact-test selectors from CLI Makefile text.
///
/// Inputs:
/// - `makefile_text`: contents of `crates/terlan/cli.mk`.
///
/// Output:
/// - Ordered selector strings as accepted by `cargo test -- --exact`.
///
/// Transformation:
/// - Reads exact-test recipe lines without interpreting shell commands or Make
///   variables.
pub(crate) fn extract_cli_exact_selectors(makefile_text: &str) -> QualityResult<Vec<String>> {
    Ok(makefile_text
        .lines()
        .filter(|line| {
            line.contains("TERLC_EXACT_TEST)")
                || line.contains("EXACT_CARGO_TEST)")
                || line.contains("scripts/run_exact_cargo_test.sh")
        })
        .filter_map(exact_selector_from_make_recipe)
        .collect())
}

/// Extracts one exact selector from a Make recipe line.
fn exact_selector_from_make_recipe(line: &str) -> Option<String> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let exact_marker = tokens
        .windows(2)
        .position(|window| window == ["--", "--exact"])?;
    exact_marker
        .checked_sub(1)
        .and_then(|index| tokens.get(index))
        .map(|selector| (*selector).to_string())
}

/// Discovers current `terlan` test names using Cargo.
///
/// Inputs:
/// - `root`: repository root used as Cargo's working directory.
///
/// Output:
/// - Set of fully qualified test names reported by Cargo.
///
/// Transformation:
/// - Lists the canonical library test owner with the feature-isolated editor,
///   quality, and benchmark modules enabled.
/// - Parses the standard test-list output into exact-selector names.
fn cargo_test_names(root: &Path) -> QualityResult<BTreeSet<String>> {
    cargo_test_names_for_args(
        root,
        &[
            "test",
            "-p",
            "terlan",
            "--lib",
            "--features",
            "editor-lsp,quality-tools,benchmark-tools",
            "--",
            "--list",
        ],
    )
}

fn cargo_test_names_for_args(root: &Path, args: &[&str]) -> QualityResult<BTreeSet<String>> {
    let output = Command::new("cargo")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run cargo test list for terlan: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "[cli-exact-selector] failed to list terlan tests:\n{}",
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

/// Returns grouped Cargo filters that no longer select a canonical library test.
pub(crate) fn stale_grouped_filters(filters: &[String], tests: &BTreeSet<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    filters
        .iter()
        .filter(|filter| seen.insert((*filter).clone()))
        .filter(|filter| !tests.iter().any(|test| test.contains(filter.as_str())))
        .map(|filter| format!("stale grouped test filter `{filter}`"))
        .collect()
}

/// Returns required tests not covered by exact selectors or grouped filters.
pub(crate) fn missing_required_test_coverage(
    selectors: &[String],
    grouped_filters: &[String],
) -> Vec<String> {
    let present = selectors
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    REQUIRED_EXACT_SELECTORS
        .iter()
        .filter(|selector| {
            !present.contains(**selector)
                && !grouped_filters
                    .iter()
                    .any(|filter| selector.contains(filter))
        })
        .map(|selector| format!("Makefile: missing required exact selector `{selector}`"))
        .collect()
}

#[cfg(test)]
#[path = "cli_exact_selectors_test.rs"]
#[cfg(test)]
mod cli_exact_selectors_test;
