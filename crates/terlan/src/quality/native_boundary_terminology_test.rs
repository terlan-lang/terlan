use super::*;
use std::fs;
use std::time::UNIX_EPOCH;

/// Verifies glossary text accepts all required terms.
///
/// Inputs:
/// - A compact text containing every required glossary term.
///
/// Output:
/// - Test passes when the glossary validator produces no diagnostics.
///
/// Transformation:
/// - Keeps the compatibility glossary contract stable without filesystem
///   access.
#[test]
fn glossary_text_accepts_required_terms() {
    let text = REQUIRED_GLOSSARY_TERMS.join("\n");
    assert!(validate_glossary_text(&text).is_empty());
}

/// Verifies the glossary requires the native-boundary behavior contract.
///
/// Inputs:
/// - Glossary text with primary native-boundary names and compatibility
///   wording, but without the behavior list.
///
/// Output:
/// - Test passes when missing behavior diagnostics are reported.
///
/// Transformation:
/// - Prevents the terminology pivot from weakening typed manifests,
///   capabilities, resources, cleanup, scheduler, async, and failure behavior.
#[test]
fn glossary_text_rejects_missing_behavior_contract() {
    let text = [
        "NativeBoundary",
        "NativeModule",
        "NativeResource",
        "HostCapability",
        "old NIF-era name for the Terlan native boundary",
        "New 0.0.7 docs and APIs should use native boundary",
    ]
    .join("\n");

    let diagnostics = validate_glossary_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("typed manifests")),
        "diagnostics should report missing behavior contract terms: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("typed failure propagation")),
        "diagnostics should report missing behavior contract terms: {diagnostics:?}"
    );
}

/// Verifies glossary placeholder wording is rejected.
///
/// Inputs:
/// - Complete glossary terms plus placeholder planning text.
///
/// Output:
/// - Test passes when the placeholder text is reported.
///
/// Transformation:
/// - Keeps the NativeBoundary terminology contract release-ready instead of
///   future-planned.
#[test]
fn glossary_text_rejects_placeholder_wording() {
    let text = format!(
        "{}\nTODO: describe native resources later.",
        REQUIRED_GLOSSARY_TERMS.join("\n")
    );
    let diagnostics = validate_glossary_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder native-boundary glossary text")),
        "diagnostics should reject glossary placeholder text: {diagnostics:?}"
    );
}

/// Verifies NIF wording is allowed only in explicit compatibility contexts.
///
/// Inputs:
/// - Documentation lines that explain historical or out-of-contract behavior.
///
/// Output:
/// - Test passes when no terminology diagnostics are emitted.
///
/// Transformation:
/// - Protects the 0.0.7 language pivot while allowing necessary migration and
///   non-goal notes.
#[test]
fn nif_terms_accept_explicit_historical_and_non_goal_context() {
    let text = [
        "The native boundary is not a NIF ABI contract.",
        "The artifact is not a NIF ABI contract.",
        "Native boundary calls are not NIF calls.",
        "Out-of-contract behavior includes OTP NIF ABI compatibility.",
    ]
    .join("\n");

    assert!(validate_nif_terms_for_doc("docs/runtime/example.md", &text).is_empty());
}

/// Verifies casual NIF terminology is rejected in new docs.
///
/// Inputs:
/// - Documentation line using NIF as the ordinary implementation frame.
///
/// Output:
/// - Test passes when the diagnostic points at the offending line.
///
/// Transformation:
/// - Keeps new runtime/package docs centered on native-boundary terminology.
#[test]
fn nif_terms_reject_casual_runtime_framing() {
    let diagnostics =
        validate_nif_terms_for_doc("docs/runtime/example.md", "This uses a NIF for speed.");

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("docs/runtime/example.md`:1"));
    assert!(diagnostics[0].contains("must not use NIF terminology"));
}

/// Verifies selected web docs reject stale VM-handler framing.
///
/// Inputs:
/// - Temporary selected README files with one stale handler phrase.
///
/// Output:
/// - Diagnostic naming the stale phrase.
///
/// Transformation:
/// - Keeps web-facing docs centered on the explicit migration bridge wording
///   instead of letting VM handler execution become the source-facing model.
#[test]
fn web_handler_docs_reject_stale_beam_handler_framing() {
    let root = make_quality_temp_dir("web_handler_docs");
    for path in SELECTED_DOC_PATHS {
        write_source_file(&root, path, "temporary BEAM migration handler bridge\n");
    }
    write_source_file(
        &root,
        "crates/terlan/src/commands/serve/README.md",
        "VM-backed handler\n",
    );

    let diagnostics = validate_web_handler_docs(&root).expect("validate web docs");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("VM-backed handler")),
        "expected stale handler wording diagnostic, got {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Writes one source fixture, creating parent directories first.
fn write_source_file(root: &std::path::Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture has parent")).expect("create source dir");
    fs::write(path, text).expect("write source fixture");
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
