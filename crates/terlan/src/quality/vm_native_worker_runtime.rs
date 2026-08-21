use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-native-worker-runtime-report.json";

const REQUIRED_WORKER_SKELETON_ANCHORS: &[&str] = &[
    "NativeBoundaryWorker",
    "NativeBoundaryCommand",
    "NativeBoundaryReply",
    "NativeBoundaryValue",
    "NativeBoundaryHandle",
    "Register { request_id",
    "Call { request_id",
    "Dispose { request_id",
    "Stop",
    "DEFAULT_CREDIT_WINDOW",
    "credit_window",
    "request_stop",
    "send_and_recv",
    "validate_args",
    "validate_handle",
    "stale_native_handle",
    "native_worker_stopped",
    "native_operation_unimplemented",
    "native_operation_unknown",
    "#![forbid(unsafe_code)]",
];

const REQUIRED_PROCESS_ANCHORS: &[&str] = &[
    "block",
    "wake",
    "request_cancellation",
    "charge_reductions",
    "cancellation_requested",
    "resource_handles",
    "exit",
];

const REQUIRED_RESOURCE_ANCHORS: &[&str] = &[
    "VmResourceTable",
    "VmResourceTransferPolicy",
    "register",
    "get_for_owner",
    "transfer",
    "release",
    "cleanup_owner",
    "stale native resource handle",
];

const REQUIRED_SCHEDULER_ANCHORS: &[&str] = &[
    "VmScheduler",
    "wake_process",
    "request_cancellation",
    "charge_reductions",
    "VmSchedulerDecision",
    "VmSchedulerOutcome",
    "Cancelled",
    "reductions_charged",
];

const REQUIRED_BOUNDARY_DOC_TERMS: &[&str] = &[
    "capability validation",
    "resource ownership",
    "scheduler accounting",
    "cancellation",
    "backpressure",
    "typed failure propagation",
    "VM-owned semantics",
];

const REQUIRED_CANONICAL_RUST_TESTS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/emit_native_metadata/artifacts_test.rs",
        &[
            "native_boundary_rust_stub_contains_actor_bridge_contract",
            "native_boundary_rust_stub_satisfies_validator",
            "native_boundary_rust_stub_compiles_as_library",
        ],
    ),
    (
        "crates/terlan/src/runtime/native_boundary/worker_test.rs",
        &[
            "worker_begin_request_rejects_backpressure_limit",
            "worker_begin_request_rejects_duplicate_request_id",
            "worker_finish_request_rejects_mismatched_request_id",
            "worker_cancel_request_releases_credit_and_rejects_late_reply",
            "worker_timeout_request_releases_credit_and_rejects_late_reply",
            "worker_cancel_request_rejects_unknown_request",
            "worker_duplicate_dispose_returns_stale_handle_and_releases_credit",
        ],
    ),
    (
        "crates/terlan/src/runtime/native_boundary/runtime_test.rs",
        &[
            "runtime_rejects_disposed_handles_through_terms",
            "runtime_rejects_duplicate_dispose_as_stale_handle",
            "runtime_rejects_malformed_payload_with_typed_error",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource_cancellation_test.rs",
        &["cancelled_process_resource_cleanup_makes_handles_stale"],
    ),
    (
        "crates/terlan/src/runtime/vm/process_test.rs",
        &["process_resource_removal_cancellation_and_reduction_accounting_are_stable"],
    ),
];

const WORKER_POLICIES: &[&str] = &[
    "nonblocking-synchronous",
    "blocking-worker",
    "cancellable-async-worker",
    "sandboxed-worker",
    "long-running-worker",
    "streaming-worker",
];

const TRACE_CASES: &[&str] = &[
    "typed worker skeleton request/reply ids",
    "credit-window backpressure surface",
    "resource registration and disposal",
    "stale native handle rejection",
    "worker stop typed failure",
    "VM process cancellation flag",
    "scheduler reduction accounting",
    "resource cleanup on actor exit",
];

const REJECTED_RUNTIME_PATHS: &[&str] = &[
    "scheduler-integrated native dispatch",
    "actor park/resume continuation",
    "stale result delivery suppression",
    "worker pool saturation runtime",
    "streaming native result delivery",
    "panic/error conversion from concrete native adapters",
    "Tokio-owned native runtime",
    "NIF-owned native runtime",
];

/// Summary produced by the VM native worker runtime gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmNativeWorkerRuntimeSummary {
    pub policy_count: usize,
    pub trace_case_count: usize,
    pub rejected_runtime_count: usize,
    pub canonical_rust_test_count: usize,
    pub report_path: PathBuf,
}

/// Runs the VM native worker runtime quality check.
///
/// Inputs:
/// - `root`: repository root containing generated NativeBoundary worker skeletons,
///   VM process/resource/scheduler primitives, boundary docs, and Makefile
///   exact-test wiring.
///
/// Output:
/// - Success summary and a report when the current worker-runtime baseline is
///   explicit and gated.
/// - Stable diagnostics if generated worker contracts, VM ownership primitives,
///   or exact selector gates drift.
///
/// Transformation:
/// - Freezes the current baseline as generated typed worker skeletons plus
///   VM-owned process/resource/scheduler accounting, while rejecting full
///   scheduler-integrated native dispatch paths until they are implemented.
pub fn run_vm_native_worker_runtime(root: &Path) -> QualityResult<VmNativeWorkerRuntimeSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/emit_native_metadata/artifacts.rs",
        REQUIRED_WORKER_SKELETON_ANCHORS,
        "native worker runtime",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/process.rs",
        REQUIRED_PROCESS_ANCHORS,
        "VM process runtime",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/resource.rs",
        REQUIRED_RESOURCE_ANCHORS,
        "VM resource runtime",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/scheduler.rs",
        REQUIRED_SCHEDULER_ANCHORS,
        "VM scheduler runtime",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "docs/runtime/NATIVE_BOUNDARY_GLOSSARY.md",
        REQUIRED_BOUNDARY_DOC_TERMS,
        "NativeBoundary glossary",
    )?);
    for (relative, required_tests) in REQUIRED_CANONICAL_RUST_TESTS {
        diagnostics.extend(validate_required_test_functions(
            root,
            relative,
            required_tests,
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;

    Ok(VmNativeWorkerRuntimeSummary {
        policy_count: WORKER_POLICIES.len(),
        trace_case_count: TRACE_CASES.len(),
        rejected_runtime_count: REJECTED_RUNTIME_PATHS.len(),
        canonical_rust_test_count: REQUIRED_CANONICAL_RUST_TESTS
            .iter()
            .map(|(_, tests)| tests.len())
            .sum(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    required_terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = read_repo_text(root, relative)?;
    let normalized = normalize_text(&text);
    Ok(required_terms
        .iter()
        .filter(|term| !normalized.contains(&normalize_text(term)))
        .map(|term| format!("{relative}: missing {label} term `{term}`"))
        .collect())
}

fn validate_required_test_functions(
    root: &Path,
    relative: &str,
    required_tests: &[&str],
) -> QualityResult<Vec<String>> {
    let text = read_repo_text(root, relative)?;
    Ok(required_tests
        .iter()
        .filter(|test_name| !text.contains(&format!("fn {test_name}(")))
        .map(|test_name| format!("{relative}: missing canonical Rust test `{test_name}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read_repo_text(root, "Makefile")?;
    let mut diagnostics = Vec::new();
    if !makefile.contains("vm-native-worker-runtime-check:") {
        diagnostics.push("Makefile: missing `vm-native-worker-runtime-check` target".to_string());
    }
    if !makefile.contains(
        "vm-native-worker-runtime-check: terlan-vm-artifact-format-check stdlib-native-artifacts-check",
    ) {
        diagnostics.push(
            "Makefile: VM native worker gate must depend on the native TVM ABI and native package artifact gates"
                .to_string(),
        );
    }
    if !makefile.contains("-- vm-native-worker-runtime") {
        diagnostics.push(
            "Makefile: missing `terlan-quality ... -- vm-native-worker-runtime` invocation"
                .to_string(),
        );
    }
    Ok(diagnostics)
}

fn write_report(path: &Path) -> QualityResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-native-worker-runtime-report-v2",
        "baseline": "generated typed NativeBoundary worker skeleton plus VM process/resource/scheduler ownership primitives",
        "workerPolicyMatrix": WORKER_POLICIES,
        "actorParkResumeTraces": [
            "process block/wake primitives are VM-owned",
            "scheduler wake_process is VM-owned",
            "full native continuation park/resume remains rejected until dispatch integration"
        ],
        "cancellationCases": [
            "process request_cancellation",
            "scheduler cancellation outcome",
            "resource cleanup on actor exit",
            "worker stop typed failure"
        ],
        "backpressureCases": [
            "generated worker credit window",
            "worker pool saturation runtime remains rejected",
            "streaming native delivery remains rejected"
        ],
        "resourceOwnershipChecks": [
            "typed NativeBoundaryHandle validation",
            "VM resource owner-only access",
            "resource transfer policy",
            "stale resource and stale native-handle diagnostics"
        ],
        "requestLifecycleAdversarialSelectors": [
            "worker_begin_request_rejects_backpressure_limit",
            "worker_begin_request_rejects_duplicate_request_id",
            "worker_finish_request_rejects_mismatched_request_id",
            "worker_cancel_request_releases_credit_and_rejects_late_reply",
            "worker_timeout_request_releases_credit_and_rejects_late_reply",
            "worker_cancel_request_rejects_unknown_request",
            "worker_duplicate_dispose_returns_stale_handle_and_releases_credit",
            "runtime_rejects_disposed_handles_through_terms",
            "runtime_rejects_duplicate_dispose_as_stale_handle",
            "runtime_rejects_malformed_payload_with_typed_error"
        ],
        "schedulerAccounting": [
            "scheduler reductions charged through process accounting",
            "native worker dispatch cannot be external-runtime owned",
            "full NativeBoundary reduction charging remains a follow-up runtime path"
        ],
        "staleResultRejection": [
            "stale native handle validation is generated",
            "resource cleanup makes handles stale after actor exit",
            "stale native result delivery suppression remains rejected until continuations land"
        ],
        "traceCases": TRACE_CASES,
        "rejectedRuntimePaths": REJECTED_RUNTIME_PATHS,
        "canonicalRustTests": REQUIRED_CANONICAL_RUST_TESTS
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM native worker runtime report: {err}"))?;
    fs::write(path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write VM native worker runtime report: {err}",
            path.display()
        )
    })
}

fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|err| format!("{relative}: failed to read file: {err}"))
}

fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-native-worker-runtime] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_native_worker_runtime_test.rs"]
#[cfg(test)]
mod vm_native_worker_runtime_test;
