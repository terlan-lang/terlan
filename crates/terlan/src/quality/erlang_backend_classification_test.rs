use std::path::Path;

use super::*;

/// The public gate rejects a reintroduced backend source file.
#[test]
fn classification_for_path_rejects_deleted_backend_paths() {
    let root = tempfile::tempdir().unwrap();
    let relative = "crates/terlan/src/backends/erlang/emit/core.rs";
    let path = root.path().join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "").unwrap();
    let error = run_erlang_backend_classification(root.path()).unwrap_err();
    assert!(error.contains(relative), "{error}");
}

/// Verifies the gate does not classify itself as backend code.
///
/// Inputs:
/// - The classification gate source path.
///
/// Output:
/// - Test passes when the source path is not treated as an Erlang/BEAM backend
///   migration candidate.
///
/// Transformation:
/// - Prevents the quality gate's own filename from forcing a meaningless
///   self-classification.
#[test]
fn scanner_ignores_classification_gate_itself() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/erlang_backend_classification.rs"
    )));
}

/// Verifies the scanner ignores the OTP reference inventory gate.
///
/// Inputs:
/// - The OTP reference inventory quality gate source path.
///
/// Output:
/// - Test passes when the path is not treated as Erlang/BEAM backend code.
///
/// Transformation:
/// - Keeps reference-inventory policy files separate from backend migration
///   implementation paths even though their names contain `otp`.
#[test]
fn scanner_ignores_otp_reference_inventory_gate() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/otp_reference_inventory.rs"
    )));
}

/// The removed OTP runtime-exit gate cannot reintroduce a backend path.
#[test]
fn scanner_rejects_deleted_otp_runtime_exit_gate() {
    assert!(is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/otp_runtime_exit.rs"
    )));
}

/// Verifies the scanner ignores the OTP test/pipeline inventory quality gate.
///
/// Inputs:
/// - The OTP test/pipeline inventory quality gate source path.
///
/// Output:
/// - Test passes when the path is not treated as Erlang/BEAM backend code.
///
/// Transformation:
/// - Keeps test and CI policy inventory separate from backend migration
///   implementation paths even though its name contains `otp`.
#[test]
fn scanner_ignores_otp_test_pipeline_inventory_gate() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/otp_test_pipeline_inventory.rs"
    )));
}

/// Reports every forbidden path and accepts the tree after their removal.
#[test]
fn scanner_reports_all_forbidden_paths() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("crates/terlan/src");
    fs::create_dir_all(&source).unwrap();
    for name in ["beam.rs", "otp.rs"] {
        fs::write(source.join(name), "").unwrap();
    }
    let error = run_erlang_backend_classification(root.path()).unwrap_err();
    for name in ["beam.rs", "otp.rs"] {
        assert!(error.contains(name), "{error}");
        fs::remove_file(source.join(name)).unwrap();
    }
    run_erlang_backend_classification(root.path()).unwrap();
}
