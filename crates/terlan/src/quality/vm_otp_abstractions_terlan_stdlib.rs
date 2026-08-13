use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::QualityResult;

const BEHAVIOR_MODULES: &[(&str, &str)] = &[
    ("std/vm/Agent.terl", "std.vm.Agent"),
    ("std/vm/GenServer.terl", "std.vm.GenServer"),
    ("std/vm/Supervisor.terl", "std.vm.Supervisor"),
    ("std/vm/Task.terl", "std.vm.Task"),
];

const SOURCE_IMPLEMENTED_MODULES: &[&str] = &[
    "std/vm/Agent.terl",
    "std/vm/GenServer.terl",
    "std/vm/Supervisor.terl",
    "std/vm/Task.terl",
];

const EXECUTABLE_EVIDENCE: &[(&str, &str)] = &[
    (
        "tests/language/VmServiceActorTest.terl",
        "agent_service_orders_state_and_discards_stale_replies",
    ),
    (
        "tests/language/VmServiceActorTest.terl",
        "gen_server_init_call_cast_timeout_terminate_and_policy_wrappers_execute",
    ),
    (
        "tests/language/VmServiceActorTest.terl",
        "monitored_child_crash_does_not_crash_the_watcher",
    ),
    (
        "tests/language/VmServiceActorTest.terl",
        "task_result_monitor_and_cancel_execute_from_terlan",
    ),
    (
        "tests/language/VmSupervisorPolicyTest.terl",
        "restart_strategies_select_ordered_children",
    ),
];

const COMPILER_INTRINSIC_FILES: &[&str] = &[
    "crates/terlan/src/compiler/typeck/core_ir/intrinsics.rs",
    "crates/terlan/src/compiler/typeck/core_intrinsic_lowering/registry.rs",
];

const FRAMEWORK_INTRINSIC_MARKERS: &[&str] = &[
    "VmAgentStart",
    "VmAgentGet",
    "VmAgentGetAndUpdate",
    "VmAgentUpdate",
    "VmAgentCast",
    "VmAgentStop",
    "VmGenServerStart",
    "VmGenServerCall",
    "VmGenServerCast",
    "VmGenServerStop",
    "VmSupervisorStartRoot",
    "VmSupervisorChildSpec",
    "VmSupervisorStart",
    "VmSupervisorStop",
    "VmTaskStart",
    "VmTaskResult",
    "VmTaskCancel",
];

const RUNTIME_SCAN_DIRS: &[&str] = &["crates/terlan/src/runtime", "crates/terlan/src/vm"];

const RUNTIME_MAGIC_KEYS: &[&str] = &["vm.agent.", "vm.gen_server.", "vm.supervisor.", "vm.task."];

const POLICY_DOC_TERMS: &[&str] = &[
    "Runtime Mechanics Versus Runtime Policy",
    "The VM owns hard runtime primitives",
    "High-level service semantics belong in Terlan stdlib",
    "Magic lowering is reserved for thin primitive wrappers",
];

const POLICY_DOCS: &[&str] = &[
    "docs/runtime/TERLAN_VM_RUNTIME_CONCEPTS.md",
    "std/vm/README.md",
];

/// Summary produced by the OTP-abstractions-as-Terlan-stdlib gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmOtpAbstractionsTerlanStdlibSummary {
    pub behavior_module_count: usize,
    pub pending_framework_intrinsic_count: usize,
    pub runtime_magic_count: usize,
    pub policy_doc_count: usize,
    pub executable_evidence_count: usize,
}

/// Runs the gate that keeps OTP-style abstractions out of direct VM runtime magic.
///
/// Inputs:
/// - Repository root.
///
/// Output:
/// - Success summary when framework abstractions have Terlan stdlib modules,
///   direct VM runtime code does not implement framework intrinsic keys, and
///   compiler-level framework intrinsics have been removed.
/// - Stable diagnostics for missing stdlib modules or new runtime magic.
/// - Stable diagnostics when the VM-mechanics versus Terlan-policy boundary
///   is no longer documented for runtime and stdlib authors.
///
/// Transformation:
/// - Encodes the policy that Agent, GenServer, Supervisor, and Task should
///   become Terlan stdlib code over low-level VM primitives rather than direct
///   per-framework VM opcodes.
pub fn run_vm_otp_abstractions_terlan_stdlib(
    root: &Path,
) -> QualityResult<VmOtpAbstractionsTerlanStdlibSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_behavior_modules(root)?);
    diagnostics.extend(validate_executable_evidence(root)?);
    diagnostics.extend(validate_policy_docs(root)?);
    let pending_framework_intrinsic_count = count_pending_framework_intrinsics(root)?;
    if pending_framework_intrinsic_count != 0 {
        diagnostics.push(format!(
            "compiler contains {pending_framework_intrinsic_count} high-level framework intrinsic marker(s); Agent, GenServer, Supervisor, and Task must lower through Terlan stdlib over hard VM primitives"
        ));
    }
    let runtime_magic = find_runtime_framework_magic(root)?;
    diagnostics.extend(runtime_magic.iter().map(|finding| {
        format!(
            "{}: direct framework VM runtime key `{}` must move behind Terlan stdlib lowering",
            finding.path.display(),
            finding.key
        )
    }));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(VmOtpAbstractionsTerlanStdlibSummary {
        behavior_module_count: BEHAVIOR_MODULES.len(),
        pending_framework_intrinsic_count,
        runtime_magic_count: runtime_magic.len(),
        policy_doc_count: POLICY_DOCS.len(),
        executable_evidence_count: EXECUTABLE_EVIDENCE.len(),
    })
}

fn validate_behavior_modules(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for (relative, module_name) in BEHAVIOR_MODULES {
        let path = root.join(relative);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                diagnostics.push(format!(
                    "{relative}: missing VM behavior stdlib module: {err}"
                ));
                continue;
            }
        };
        let module_decl = format!("module {module_name}.");
        if !text.contains(&module_decl) {
            diagnostics.push(format!(
                "{relative}: expected Terlan module declaration `{module_decl}`"
            ));
        }
        if !text.contains("@target.vm") {
            diagnostics.push(format!(
                "{relative}: VM behavior stdlib module must declare VM target metadata"
            ));
        }
        if SOURCE_IMPLEMENTED_MODULES.contains(relative)
            && text.lines().any(|line| line.trim() == "native.")
        {
            diagnostics.push(format!(
                "{relative}: high-level service abstraction must execute as Terlan source, not a native placeholder"
            ));
        }
    }
    Ok(diagnostics)
}

fn validate_executable_evidence(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for (relative, selector) in EXECUTABLE_EVIDENCE {
        let text = match fs::read_to_string(root.join(relative)) {
            Ok(text) => text,
            Err(err) => {
                diagnostics.push(format!(
                    "{relative}: missing executable Terlan service evidence: {err}"
                ));
                continue;
            }
        };
        if !text.contains(&format!("pub {selector}(")) {
            diagnostics.push(format!(
                "{relative}: missing executable Terlan service case `{selector}`"
            ));
        }
    }
    Ok(diagnostics)
}

fn validate_policy_docs(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for relative in POLICY_DOCS {
        let text = match fs::read_to_string(root.join(relative)) {
            Ok(text) => text,
            Err(err) => {
                diagnostics.push(format!(
                    "{relative}: missing VM policy-boundary documentation: {err}"
                ));
                continue;
            }
        };
        for term in POLICY_DOC_TERMS {
            if !text.contains(term) {
                diagnostics.push(format!(
                    "{relative}: missing VM policy-boundary anchor `{term}`"
                ));
            }
        }
    }
    Ok(diagnostics)
}

fn count_pending_framework_intrinsics(root: &Path) -> QualityResult<usize> {
    let mut count = 0;
    for relative in COMPILER_INTRINSIC_FILES {
        let text = fs::read_to_string(root.join(relative))
            .map_err(|err| format!("{relative}: failed to read compiler intrinsic file: {err}"))?;
        for marker in FRAMEWORK_INTRINSIC_MARKERS {
            if text.contains(marker) {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn find_runtime_framework_magic(root: &Path) -> QualityResult<Vec<RuntimeMagicFinding>> {
    let mut findings = Vec::new();
    for relative in RUNTIME_SCAN_DIRS {
        collect_runtime_framework_magic(root, &root.join(relative), &mut findings)?;
    }
    Ok(findings)
}

fn collect_runtime_framework_magic(
    root: &Path,
    dir: &Path,
    findings: &mut Vec<RuntimeMagicFinding>,
) -> QualityResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("{}: failed to read runtime dir: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read runtime entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_runtime_framework_magic(root, &path, findings)?;
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("{}: failed to read runtime source: {err}", path.display()))?;
        for key in RUNTIME_MAGIC_KEYS {
            if text.contains(key) {
                findings.push(RuntimeMagicFinding {
                    path: path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                    key: (*key).to_string(),
                });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeMagicFinding {
    path: PathBuf,
    key: String,
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-otp-abstractions-terlan-stdlib] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_otp_abstractions_terlan_stdlib_test.rs"]
#[cfg(test)]
mod vm_otp_abstractions_terlan_stdlib_test;
