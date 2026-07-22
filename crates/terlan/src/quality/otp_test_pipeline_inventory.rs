use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the OTP test and pipeline inventory gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpTestPipelineInventorySummary {
    pub inventory_row_count: usize,
    pub scanned_surface_count: usize,
}

const DOC_PATH: &str = "docs/runtime/OTP_TEST_PIPELINE_INVENTORY.md";
const BUILD_TEST_DIR: &str = "crates/terlan/src/commands/build/build_test/tests";

const REQUIRED_TERMS: &[&str] = &[
    "0.0.7 test and pipeline otp exit inventory",
    "no default release gate may require stock otp",
    "no new otp-dependent test or pipeline may be added without this inventory",
    "terlan-vm",
    "corev0",
    "default-release-gate",
    "migration-lane",
    "reference-only",
    "remove",
    "historical",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

const REQUIRED_INVENTORY_PATHS: &[&str] = &[
    "Makefile",
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
    "crates/terlan/cli.mk",
    "std/stdlib.mk",
    "std/scripts/check_native_artifacts.py",
    "crates/terlan/src/commands/emit_native_metadata",
    "scripts/check_release_boundary.sh",
    "crates/terlan/src/commands/build/build_test/tests",
    "crates/terlan/src/commands/build/build_test/tests/artifact_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/args_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/dependency_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/executable_vm_artifact_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/import_constructor_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/mobile_build_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/project_layout_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/shape_js_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/std_runtime_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/wasm_artifact_metadata_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/wasm_build_target_test.rs",
    "crates/terlan/src/commands/run",
    "crates/terlan/src/commands/test",
    "crates/terlan/src/commands/repl",
    "crates/terlan/src/commands/serve",
    "crates/terlan/src/validation/target_profile",
    "tools/check_http_runtime_stack.py",
];

const CLOSED_DEFAULT_RELEASE_ROWS: &[&str] = &[
    "Makefile",
    "crates/terlan/src/commands/build/build_test/tests",
    "crates/terlan/src/commands/build/build_test/tests/artifact_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/args_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/dependency_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/executable_vm_artifact_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/import_constructor_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/mobile_build_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/project_layout_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/shape_js_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/std_runtime_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/wasm_artifact_metadata_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/wasm_build_target_test.rs",
    "crates/terlan/src/commands/run",
    "crates/terlan/src/commands/test",
    "crates/terlan/src/commands/repl",
    "crates/terlan/src/commands/emit_native_metadata",
];

const ALLOWED_MIGRATION_ROWS: &[&str] = &[];

const VM_DEFAULT_FIXTURE_FREE_PATHS: &[&str] = &[
    "crates/terlan/src/commands/build/build_test/tests/args_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/artifact_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/dependency_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/executable_vm_artifact_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/import_constructor_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/mobile_build_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/project_layout_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/shape_js_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/std_runtime_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/wasm_artifact_metadata_test.rs",
    "crates/terlan/src/commands/build/build_test/tests/wasm_build_target_test.rs",
];

const SELECTED_SURFACES: &[&str] = &[
    "Makefile",
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
    "crates/terlan/cli.mk",
    "std/stdlib.mk",
    "std/scripts/run_release_tests.sh",
    "std/scripts/check_negative_api_tests.sh",
    "std/scripts/check_io_negative_api_tests.sh",
    "std/scripts/check_native_artifacts.py",
    "crates/terlan/src/commands/emit_native_metadata",
    "scripts/check_release_boundary.sh",
    "tools/check_http_runtime_stack.py",
];

const OTP_MARKERS: &[&str] = &[
    "Erlang",
    "BEAM",
    "OTP",
    "--target erlang",
    "beam-thin",
    "rebar3",
];

/// Runs the OTP test and pipeline inventory gate.
///
/// Inputs:
/// - `root`: repository root containing runtime docs and pipeline files.
///
/// Output:
/// - Success summary when every selected OTP-dependent surface is classified.
/// - Stable diagnostics when a pipeline or test surface contains OTP wording
///   without a matching inventory row.
///
/// Transformation:
/// - Converts the 0.0.7 no-OTP-default policy into a checked inventory for
///   tests, scripts, Make targets, and CI workflow files.
pub fn run_otp_test_pipeline_inventory(
    root: &Path,
) -> QualityResult<OtpTestPipelineInventorySummary> {
    let text = read_repo_text(root, DOC_PATH)?;
    let diagnostics = validate_inventory(root, &text)?;
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(OtpTestPipelineInventorySummary {
        inventory_row_count: REQUIRED_INVENTORY_PATHS.len(),
        scanned_surface_count: SELECTED_SURFACES.len(),
    })
}

/// Validates the checked-in inventory text and selected repository surfaces.
fn validate_inventory(root: &Path, text: &str) -> QualityResult<Vec<String>> {
    let mut diagnostics = validate_inventory_text(text);
    diagnostics.extend(validate_selected_surface_rows(root, text)?);
    diagnostics.extend(validate_no_stale_vm_fixture_beam_artifacts(root)?);
    diagnostics.extend(validate_build_test_file_rows(root, text)?);
    Ok(diagnostics)
}

/// Validates required inventory terms and rows.
fn validate_inventory_text(text: &str) -> Vec<String> {
    let normalized = normalize(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&normalize(term)) {
            diagnostics.push(format!("missing OTP test/pipeline inventory term `{term}`"));
        }
    }
    for path in REQUIRED_INVENTORY_PATHS {
        if !contains_inventory_path(text, path) {
            diagnostics.push(format!("missing OTP test/pipeline inventory row `{path}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(&normalize(placeholder)) {
            diagnostics.push(format!(
                "placeholder OTP test/pipeline inventory text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics.extend(validate_closed_default_release_rows(text));
    diagnostics.extend(validate_allowed_migration_rows(text));
    diagnostics
}

/// Validates rows that have already moved off removed runtime lanes.
fn validate_closed_default_release_rows(text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for path in CLOSED_DEFAULT_RELEASE_ROWS {
        let Some(row) = inventory_row_for_path(text, path) else {
            continue;
        };
        if !row.contains("default-release-gate") {
            diagnostics.push(format!(
                "closed VM/default release row `{path}` must be classified as default-release-gate"
            ));
        }
        if row.contains("migration-lane") {
            diagnostics.push(format!(
                "closed VM/default release row `{path}` must not remain classified as migration-lane"
            ));
        }
    }
    diagnostics
}

/// Validates remaining migration lanes are the checked concrete rows only.
fn validate_allowed_migration_rows(text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for line in text.lines() {
        if !line.trim_start().starts_with('|') || !line.contains("migration-lane") {
            continue;
        }
        if ALLOWED_MIGRATION_ROWS
            .iter()
            .any(|path| contains_inventory_path(line, path))
        {
            continue;
        }
        diagnostics.push(format!(
            "unexpected migration-lane inventory row `{}`",
            line.trim()
        ));
    }
    for path in ALLOWED_MIGRATION_ROWS {
        let Some(row) = inventory_row_for_path(text, path) else {
            continue;
        };
        if !row.contains("migration-lane") {
            diagnostics.push(format!(
                concat!(
                    "allowed migration row `{path}` must remain migration-lane until replaced or ",
                    "removed from ALLOWED_MIGRATION_ROWS"
                ),
                path = path
            ));
        }
    }
    diagnostics
}

/// Validates selected files with OTP markers have inventory rows.
fn validate_selected_surface_rows(root: &Path, inventory: &str) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for surface in SELECTED_SURFACES {
        let text = read_selected_surface_text(root, surface)?;
        if contains_otp_marker(&text) && !contains_inventory_path(inventory, surface) {
            diagnostics.push(format!(
                "`{surface}` contains OTP/Erlang/BEAM markers but is not classified"
            ));
        }
    }
    Ok(diagnostics)
}

/// Reads a selected inventory surface, concatenating Rust files for directories.
fn read_selected_surface_text(root: &Path, surface: &str) -> QualityResult<String> {
    let path = root.join(surface);
    if path.is_dir() {
        let mut text = String::new();
        let mut entries = fs::read_dir(&path)
            .map_err(|err| format!("{surface}: failed to read selected surface directory: {err}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                format!("{surface}: failed to read selected surface directory entry: {err}")
            })?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let file_path = entry.path();
            if file_path
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("rs")
            {
                continue;
            }
            text.push_str(&fs::read_to_string(&file_path).map_err(|err| {
                format!(
                    "{}: failed to read selected surface file: {err}",
                    file_path.display()
                )
            })?);
            text.push('\n');
        }
        return Ok(text);
    }
    read_repo_text(root, surface)
}

/// Validates cleaned VM-default fixture files do not regain VM artifact text.
fn validate_no_stale_vm_fixture_beam_artifacts(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for path in VM_DEFAULT_FIXTURE_FREE_PATHS {
        let text = read_repo_text(root, path)?;
        if text.contains("beam-thin") {
            diagnostics.push(format!(
                "`{path}` must not contain stale `beam-thin` fixtures in VM-default tests"
            ));
        }
    }
    Ok(diagnostics)
}

/// Validates every build-test Rust file has an explicit inventory row.
fn validate_build_test_file_rows(root: &Path, inventory: &str) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    let mut paths = Vec::new();
    for entry in fs::read_dir(root.join(BUILD_TEST_DIR))
        .map_err(|err| format!("{BUILD_TEST_DIR}: failed to read build-test directory: {err}"))?
    {
        let entry = entry
            .map_err(|err| format!("{BUILD_TEST_DIR}: failed to read directory entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        paths.push(format!("{BUILD_TEST_DIR}/{file_name}"));
    }
    paths.sort();
    for path in paths {
        if !contains_inventory_path(inventory, &path) {
            diagnostics.push(format!(
                "build-test file `{path}` must have an explicit OTP test/pipeline inventory row"
            ));
        }
    }
    Ok(diagnostics)
}

/// Returns true when text contains one of the tracked OTP markers.
fn contains_otp_marker(text: &str) -> bool {
    OTP_MARKERS.iter().any(|marker| text.contains(marker))
        || text
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|word| word == "erl" || word == "erlc")
}

/// Returns true when an inventory Markdown table names a path.
fn contains_inventory_path(text: &str, path: &str) -> bool {
    text.contains(&format!("`{path}`"))
}

/// Returns the Markdown table row for a checked inventory path.
fn inventory_row_for_path<'a>(text: &'a str, path: &str) -> Option<&'a str> {
    let needle = format!("`{path}`");
    text.lines().find(|line| line.contains(&needle))
}

/// Reads a repository-relative UTF-8 text file.
fn read_repo_text(root: &Path, path: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(path))
        .map_err(|err| format!("{path}: failed to read repository text file: {err}"))
}

/// Normalizes prose for term checks.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Renders gate diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[otp-test-pipeline-inventory] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "otp_test_pipeline_inventory_test.rs"]
mod otp_test_pipeline_inventory_test;
