use std::fs;
use std::time::UNIX_EPOCH;

use super::*;

/// Verifies the marker audit reports missing runtime contracts.
///
/// Inputs:
/// - One temporary source file.
/// - One rule requiring absent marker text.
///
/// Output:
/// - One stable diagnostic naming the missing marker.
///
/// Transformation:
/// - Exercises the rule engine without relying on the repository filesystem.
#[test]
fn marker_audit_reports_missing_runtime_contracts() {
    let root = make_quality_temp_dir("missing_runtime_marker");
    fs::create_dir_all(root.join("crates/terlan/src/main.rs").parent().unwrap())
        .expect("create source dir");
    fs::write(root.join("crates/terlan/src/main.rs"), "pub fn main() {}\n").expect("write source");
    let rules = [RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "--target terlan-vm",
        reason: "test marker",
    }];

    let diagnostics =
        missing_runtime_selection_markers(&root, &rules).expect("collect diagnostics");

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("--target terlan-vm"));
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies the marker audit accepts explicit runtime contracts.
///
/// Inputs:
/// - One temporary source file containing the expected marker.
/// - One rule requiring that marker.
///
/// Output:
/// - Empty diagnostic list.
///
/// Transformation:
/// - Locks the successful path used by the repository gate.
#[test]
fn marker_audit_accepts_present_runtime_contracts() {
    let root = make_quality_temp_dir("present_runtime_marker");
    fs::create_dir_all(root.join("crates/terlan/src/main.rs").parent().unwrap())
        .expect("create source dir");
    fs::write(
        root.join("crates/terlan/src/main.rs"),
        "terlc run [project-dir|file.terl] [--target terlan-vm]\n",
    )
    .expect("write source");
    let rules = [RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "terlc run [project-dir|file.terl] [--target terlan-vm]",
        reason: "test marker",
    }];

    let diagnostics =
        missing_runtime_selection_markers(&root, &rules).expect("collect diagnostics");

    assert!(diagnostics.is_empty());
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies forbidden runtime fragments produce stable diagnostics.
///
/// Inputs:
/// - One temporary public usage file containing a removed runtime flag.
/// - One forbidden-fragment rule requiring that flag to stay absent.
///
/// Output:
/// - One stable diagnostic naming the forbidden fragment.
///
/// Transformation:
/// - Exercises the negative public-surface audit independently from marker
///   presence so removed runtime spellings cannot reappear in help text.
#[test]
fn forbidden_fragment_audit_reports_removed_runtime_flags() {
    let root = make_quality_temp_dir("forbidden_runtime_fragment");
    fs::create_dir_all(root.join("crates/terlan/src/main.rs").parent().unwrap())
        .expect("create source dir");
    fs::write(
        root.join("crates/terlan/src/main.rs"),
        "terlc run [project-dir|file.terl] [--target erlang]\n",
    )
    .expect("write source");
    let fragments = [ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "--target erlang",
        reason: "test fragment",
    }];

    let diagnostics =
        forbidden_runtime_fragment_diagnostics(&root, &fragments).expect("collect diagnostics");

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("--target erlang"));
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies forbidden runtime fragment diagnostics include the audit reason.
///
/// Inputs:
/// - One temporary public usage file containing a removed runtime flag.
/// - One forbidden-fragment rule with an explicit reason.
///
/// Output:
/// - Diagnostic names both the forbidden fragment and reason.
///
/// Transformation:
/// - Keeps runtime-removal failures actionable instead of reporting only a raw
///   matched string.
#[test]
fn forbidden_fragment_audit_reports_reason_text() {
    let root = make_quality_temp_dir("forbidden_runtime_reason");
    fs::create_dir_all(root.join("crates/terlan/src/main.rs").parent().unwrap())
        .expect("create source dir");
    fs::write(root.join("crates/terlan/src/main.rs"), "--runtime beam\n").expect("write source");
    let fragments = [ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "--runtime beam",
        reason: "beam runtime was removed",
    }];

    let diagnostics =
        forbidden_runtime_fragment_diagnostics(&root, &fragments).expect("collect diagnostics");

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("--runtime beam"));
    assert!(diagnostics[0].contains("beam runtime was removed"));
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies absent forbidden runtime fragments pass the audit.
///
/// Inputs:
/// - One temporary public usage file containing only the VM target.
/// - One forbidden-fragment rule for the removed Erlang target.
///
/// Output:
/// - Empty diagnostic list.
///
/// Transformation:
/// - Locks the clean public-surface path used by the repository gate.
#[test]
fn forbidden_fragment_audit_accepts_vm_only_usage() {
    let root = make_quality_temp_dir("absent_forbidden_runtime_fragment");
    fs::create_dir_all(root.join("crates/terlan/src/main.rs").parent().unwrap())
        .expect("create source dir");
    fs::write(
        root.join("crates/terlan/src/main.rs"),
        "terlc run [project-dir|file.terl] [--target terlan-vm]\n",
    )
    .expect("write source");
    let fragments = [ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "--target erlang",
        reason: "test fragment",
    }];

    let diagnostics =
        forbidden_runtime_fragment_diagnostics(&root, &fragments).expect("collect diagnostics");

    assert!(diagnostics.is_empty());
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Creates a unique temporary directory for quality unit tests.
fn make_quality_temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "terlan_quality_{label}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create quality temp dir");
    path
}
