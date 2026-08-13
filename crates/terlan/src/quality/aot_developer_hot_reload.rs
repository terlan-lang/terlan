use std::path::{Path, PathBuf};

use serde_json::json;

use super::support::{validate_required_terms, write_json_report};
use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/aot-developer-hot-reload-report.json";

const DOC_TERMS: &[&str] = &[
    "persistent compiler daemon",
    "structured reload event stream",
    "templates, styles, package inputs, and generated binding metadata",
    "proven import dependents",
    "NativeIR-to-Cranelift object path",
    "JIT, interpreter, generated application Rust, CoreIR runtime",
    "single synced `active.json`",
    "in-flight calls and continuations",
    "process-state shape",
    "TERLAN_SERVE_RESTART_INCOMPATIBLE=1",
    "retained runtime state",
    "adversarial partial and stale generation rejection",
    "direct-call optimization",
];

const IMPLEMENTATION_TERMS: &[&str] = &[
    "compile_source_candidate",
    "persist_generation_batch",
    "validate_compatibility",
    "activate_persisted_generation",
    "DeveloperReloadEvent",
    "CompilerInvalidation",
    "RuntimeActivation",
    "failed_build_continuity",
    "advance_cache_epoch",
];

const TEST_TERMS: &[&str] = &[
    "developer_reload_is_atomic_compatible_and_failed_edit_safe",
    "incompatible_state",
    "old cache remains active",
    "in-flight call keeps its native image",
    "partial generation must fail closed",
    "corrected edit activates",
];

#[derive(Clone, Debug, Eq, PartialEq)]
/// Evidence summary for the direct-AOT developer hot-reload contract.
pub struct AotDeveloperHotReloadSummary {
    /// Number of documentation and implementation contract terms validated.
    pub contract_count: usize,
    /// Number of required adversarial test terms validated.
    pub adversarial_case_count: usize,
    /// Repository-relative path to the generated evidence report.
    pub report_path: PathBuf,
}

/// Validates and records the direct-AOT developer hot-reload evidence.
pub fn run_aot_developer_hot_reload(root: &Path) -> QualityResult<AotDeveloperHotReloadSummary> {
    let mut diagnostics = validate_required_terms(
        root,
        "docs/compiler/AOT_DEVELOPER_HOT_RELOAD.md",
        DOC_TERMS,
        "direct-AOT reload contract",
    )?;
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/handler_cache/source_generation.rs",
        IMPLEMENTATION_TERMS,
        "transactional reload implementation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/handler_cache_generation_test.rs",
        TEST_TERMS,
        "direct-AOT reload test",
    )?);
    if !diagnostics.is_empty() {
        return Err(format!(
            "error[aot.developer_hot_reload]:\n  - {}",
            diagnostics.join("\n  - ")
        ));
    }
    let report_path = root.join(REPORT_PATH);
    write_json_report(
        &report_path,
        &json!({
            "schema": "terlan.aot-developer-hot-reload.v1",
            "compiler_session": "persistent",
            "publication": "atomic-active-generation-pointer",
            "runtime": "direct-aot-cranelift",
            "state_policy": "retain-compatible-reject-incompatible-with-explicit-restart",
            "event_consumers": ["runtime", "browser", "debugger", "editor", "vm-tui"],
            "tests": [
                "compatible-handler-and-template-edit",
                "incompatible-state-edit",
                "broken-edit-old-service-continuity",
                "corrected-activation",
                "in-flight-generation-pinning",
                "partial-and-stale-generation-rejection"
            ],
            "fallbacks": {"jit": false, "interpreter": false, "generated_rust": false}
        }),
    )?;
    Ok(AotDeveloperHotReloadSummary {
        contract_count: DOC_TERMS.len() + IMPLEMENTATION_TERMS.len(),
        adversarial_case_count: TEST_TERMS.len(),
        report_path: PathBuf::from(REPORT_PATH),
    })
}

#[cfg(test)]
#[path = "aot_developer_hot_reload_test.rs"]
mod aot_developer_hot_reload_test;
