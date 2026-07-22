use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the VM diagnostics quality gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmDiagnosticsQualitySummary {
    pub required_contract_term_count: usize,
    pub exact_selector_count: usize,
}

const DIAGNOSTICS_CONTRACT_DOC: &str = "docs/runtime/VM_DIAGNOSTICS_QUALITY.md";

const VM_MAIN_SOURCES: &[&str] = &[
    "crates/terlan/src/vm/main_part_001.rs",
    "crates/terlan/src/vm/main_part_002.rs",
    "crates/terlan/src/vm/main/inspection.rs",
];

const REQUIRED_CONTRACT_TERMS: &[&str] = &[
    "rust panic output",
    "raw backend errors",
    "internal stack dumps",
    "stable diagnostic code",
    "vm_load",
    "vm_execute",
    "native_boundary",
    "debugger_source_map",
    "project_migration",
    "text diagnostics",
    "json diagnostics",
    "native debug data",
    "source spans",
    "malformed native images",
    "missing exports",
    "descriptor mismatches",
    "unsupported native targets",
    "stale resources",
    "nativeboundary failures",
    "duplicate nativeboundary request ids",
    "typed diagnostic probes",
];

const REQUIRED_VM_MAIN_TERMS: &[&str] = &[
    "DiagnosticFormat",
    "Json",
    "error[tvm_json_runtime_removed]",
    "error[vm_inspect_not_found]",
];

const REQUIRED_REPL_TERMS: &[&str] = &[
    "render_repl_json_event",
    "\"schema\"",
    "\"kind\"",
    "\"text\"",
];

const REQUIRED_MAKE_SELECTORS: &[&str] = &[
    "runtime::native_image::native_image_test::native_inspection_rejects_json_and_non_executables",
    "runtime::native_image::native_image_test::descriptor_rejects_tampering_and_noncanonical_records",
    "runtime::vm::actor::actor_test::actor_runtime_reports_missing_and_exited_context_diagnostics",
    "runtime::vm::code_server::code_server_test::code_server_reports_missing_module_and_exited_process_diagnostics",
    "runtime::vm::resource::resource_test::resource_table_reports_stale_handle_after_release",
    "runtime::native_boundary::runtime::runtime_test::runtime_rejects_malformed_payload_with_typed_error",
    "runtime::native_boundary::worker::worker_test::worker_begin_request_rejects_duplicate_request_id",
    "commands::repl::repl_test::repl_json_event_without_extra_fields_is_valid_json",
    "runtime::vm::io_diagnostics::io_diagnostics_test::diagnostic_probe_latches_only_post_install_typed_resource_fault",
    "runtime::vm::io_diagnostics::io_diagnostics_test::diagnostic_probe_enforces_log_identity_and_close_lifecycle",
];

/// Runs the VM diagnostics quality gate.
///
/// Inputs:
/// - `root`: repository root containing runtime docs, VM CLI source, REPL
///   event source, and Makefile gates.
///
/// Output:
/// - Success summary when diagnostic contracts and adversarial selectors are
///   present.
/// - Stable diagnostics when user-facing VM error quality can regress.
///
/// Transformation:
/// - Validates the checked diagnostics contract, source-level diagnostic
///   capabilities, and exact release selectors that exercise malformed
///   native image, descriptor, missing runtime object, stale resource, and
///   NativeBoundary failure paths.
pub fn run_vm_diagnostics_quality(root: &Path) -> QualityResult<VmDiagnosticsQualitySummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        DIAGNOSTICS_CONTRACT_DOC,
        REQUIRED_CONTRACT_TERMS,
    )?);
    diagnostics.extend(validate_required_terms_across_files(
        root,
        VM_MAIN_SOURCES,
        REQUIRED_VM_MAIN_TERMS,
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/repl/event.rs",
        REQUIRED_REPL_TERMS,
    )?);
    diagnostics.extend(validate_makefile_selectors(root)?);

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(VmDiagnosticsQualitySummary {
        required_contract_term_count: REQUIRED_CONTRACT_TERMS.len(),
        exact_selector_count: REQUIRED_MAKE_SELECTORS.len(),
    })
}

/// Validates required terms in a repository file.
fn validate_required_terms(
    root: &Path,
    relative: &str,
    required_terms: &[&str],
) -> QualityResult<Vec<String>> {
    let text = read_repo_text(root, relative)?;
    Ok(validate_required_terms_text(
        relative,
        &text,
        required_terms,
    ))
}

/// Validates terms that intentionally span focused source modules.
fn validate_required_terms_across_files(
    root: &Path,
    relatives: &[&str],
    required_terms: &[&str],
) -> QualityResult<Vec<String>> {
    let mut text = String::new();
    for relative in relatives {
        text.push_str(&read_repo_text(root, relative)?);
        text.push('\n');
    }
    Ok(validate_required_terms_text(
        &relatives.join(", "),
        &text,
        required_terms,
    ))
}

/// Validates required terms in already-read text.
fn validate_required_terms_text(
    relative: &str,
    text: &str,
    required_terms: &[&str],
) -> Vec<String> {
    let normalized = normalize_text(text);
    required_terms
        .iter()
        .filter(|term| !normalized.contains(&normalize_text(term)))
        .map(|term| format!("{relative}: missing VM diagnostics term `{term}`"))
        .collect()
}

/// Validates the Makefile wires exact diagnostic selectors.
fn validate_makefile_selectors(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read_repo_text(root, "Makefile")?;
    let mut diagnostics = Vec::new();
    if !makefile.contains("vm-diagnostics-quality-check:") {
        diagnostics.push("Makefile: missing `vm-diagnostics-quality-check` target".to_string());
    }
    for selector in REQUIRED_MAKE_SELECTORS {
        if !makefile.contains(selector) {
            diagnostics.push(format!(
                "Makefile: missing VM diagnostics exact selector `{selector}`"
            ));
        }
    }
    Ok(diagnostics)
}

/// Reads one repository text file.
fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|err| format!("{relative}: failed to read file: {err}"))
}

/// Normalizes text for stable contract matching.
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders VM diagnostics quality failures.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-diagnostics-quality] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_diagnostics_quality_test.rs"]
mod vm_diagnostics_quality_test;
