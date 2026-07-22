use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    find_runtime_framework_magic, run_vm_otp_abstractions_terlan_stdlib, validate_behavior_modules,
    validate_policy_docs,
};

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-vm-otp-abstractions-{name}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

fn write_file(root: &PathBuf, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write fixture");
}

fn write_behavior_modules(root: &PathBuf) {
    for module in ["Agent", "GenServer", "Supervisor", "Task"] {
        write_file(
            root,
            &format!("std/vm/{module}.terl"),
            &format!(
                "module std.vm.{module}.\n\n@target.vm {{process_mailbox : true }}\npub marker(): Unit ->\n    Unit.\n"
            ),
        );
    }
}

fn write_compiler_intrinsic_files(root: &PathBuf) {
    write_file(
        root,
        "crates/terlan/src/compiler/typeck/core_ir/intrinsics.rs",
        "VmAgentStart VmGenServerStart VmSupervisorStart VmTaskStart",
    );
    write_file(
        root,
        "crates/terlan/src/compiler/typeck/core_intrinsic_lowering/registry.rs",
        "VmAgentStart VmGenServerStart VmSupervisorStart VmTaskStart",
    );
}

fn write_policy_docs(root: &PathBuf) {
    for relative in [
        "docs/runtime/TERLAN_VM_RUNTIME_CONCEPTS.md",
        "std/vm/README.md",
    ] {
        write_file(
            root,
            relative,
            r#"
# Runtime Mechanics Versus Runtime Policy

The VM owns hard runtime primitives.
High-level service semantics belong in Terlan stdlib.
Magic lowering is reserved for thin primitive wrappers.
"#,
        );
    }
}

#[test]
fn vm_otp_abstractions_gate_accepts_current_inventory_shape() {
    let root = temp_repo("complete");
    write_behavior_modules(&root);
    write_compiler_intrinsic_files(&root);
    write_policy_docs(&root);
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/process.rs",
        "pub struct VmProcess;",
    );

    let summary = run_vm_otp_abstractions_terlan_stdlib(&root).expect("gate should pass");

    assert_eq!(summary.behavior_module_count, 4);
    assert_eq!(summary.pending_framework_intrinsic_count, 8);
    assert_eq!(summary.runtime_magic_count, 0);
    assert_eq!(summary.policy_doc_count, 2);
    fs::remove_dir_all(root).expect("remove temp repo");
}

#[test]
fn vm_otp_abstractions_gate_rejects_missing_behavior_module() {
    let root = temp_repo("missing-module");
    write_behavior_modules(&root);
    fs::remove_file(root.join("std/vm/GenServer.terl")).expect("remove GenServer");

    let diagnostics = validate_behavior_modules(&root).expect("validate behavior modules");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("std/vm/GenServer.terl")),
        "expected missing GenServer diagnostic, got {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove temp repo");
}

#[test]
fn vm_otp_abstractions_gate_rejects_missing_policy_boundary_doc_anchor() {
    let root = temp_repo("missing-policy-doc-anchor");
    write_policy_docs(&root);
    write_file(
        &root,
        "std/vm/README.md",
        "# Runtime Mechanics Versus Runtime Policy\n\nThe VM owns hard runtime primitives.\n",
    );

    let diagnostics = validate_policy_docs(&root).expect("validate policy docs");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("High-level service semantics")),
        "expected policy boundary diagnostic, got {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove temp repo");
}

#[test]
fn vm_otp_abstractions_gate_rejects_direct_runtime_framework_magic() {
    let root = temp_repo("runtime-magic");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/intrinsics.rs",
        r#""vm.gen_server.start""#,
    );

    let findings = find_runtime_framework_magic(&root).expect("scan runtime magic");

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].key, "vm.gen_server.");
    fs::remove_dir_all(root).expect("remove temp repo");
}
