use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verifies diagnostics contract text accepts all required terms.
///
/// Inputs:
/// - A compact text containing every required diagnostics term.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Exercises the document validator without reading repository files.
#[test]
fn diagnostics_contract_text_accepts_required_terms() {
    let text = REQUIRED_CONTRACT_TERMS.join("\n");

    assert!(validate_required_terms_text(
        "docs/runtime/VM_DIAGNOSTICS_QUALITY.md",
        &text,
        REQUIRED_CONTRACT_TERMS
    )
    .is_empty());
}

/// Verifies diagnostics contract text rejects missing JSON diagnostics terms.
///
/// Inputs:
/// - Contract text that mentions text diagnostics only.
///
/// Output:
/// - Test passes when JSON diagnostics are reported missing.
///
/// Transformation:
/// - Keeps the VM diagnostics contract from becoming text-only.
#[test]
fn diagnostics_contract_text_rejects_missing_json_diagnostics() {
    let diagnostics = validate_required_terms_text(
        "docs/runtime/VM_DIAGNOSTICS_QUALITY.md",
        "stable diagnostic code text diagnostics",
        REQUIRED_CONTRACT_TERMS,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("json diagnostics")),
        "diagnostics should report missing JSON diagnostics: {diagnostics:?}"
    );
}

/// Verifies Makefile selector validation rejects missing exact tests.
///
/// Inputs:
/// - Temporary Makefile with the target name but no exact selectors.
///
/// Output:
/// - Test passes when the first missing selector is reported.
///
/// Transformation:
/// - Prevents the diagnostics quality gate from existing without adversarial
///   execution coverage.
#[test]
fn makefile_selector_validation_rejects_missing_selectors() {
    let root = make_temp_root("vm_diagnostics_quality_missing_selectors");
    fs::write(
        root.join("Makefile"),
        "vm-diagnostics-quality-check:\n\ttrue\n",
    )
    .expect("write Makefile");

    let diagnostics = validate_makefile_selectors(&root).expect("validate selectors");
    fs::remove_dir_all(&root).expect("remove fixture");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains(REQUIRED_MAKE_SELECTORS[0])),
        "diagnostics should report missing exact selectors: {diagnostics:?}"
    );
}

/// Verifies Makefile selector validation accepts the target and selectors.
///
/// Inputs:
/// - Temporary Makefile containing the diagnostics target and every selector.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Locks the exact-selector list to the checker.
#[test]
fn makefile_selector_validation_accepts_required_selectors() {
    let root = make_temp_root("vm_diagnostics_quality_required_selectors");
    let mut makefile = String::from("vm-diagnostics-quality-check:\n");
    for selector in REQUIRED_MAKE_SELECTORS {
        makefile.push_str("\tbash scripts/run_exact_cargo_test.sh -p terlan ");
        makefile.push_str(selector);
        makefile.push_str(" -- --exact\n");
    }
    fs::write(root.join("Makefile"), makefile).expect("write Makefile");

    let diagnostics = validate_makefile_selectors(&root).expect("validate selectors");
    fs::remove_dir_all(&root).expect("remove fixture");

    assert!(
        diagnostics.is_empty(),
        "expected no selector diagnostics: {diagnostics:?}"
    );
}

/// Creates a unique temporary fixture root.
fn make_temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "terlan-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create fixture root");
    root
}
