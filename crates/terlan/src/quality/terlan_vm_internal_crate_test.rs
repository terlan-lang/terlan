use super::*;

/// Verifies VM README ownership wording accepts required terms.
///
/// Inputs:
/// - A compact text containing every required term.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Exercises the README term gate without reading repository files.
#[test]
fn vm_readme_text_accepts_required_terms() {
    let text = REQUIRED_VM_README_TERMS.join("\n");
    assert!(validate_vm_readme_text(&text).is_empty());
}

/// Verifies VM README ownership wording reports missing terms.
///
/// Inputs:
/// - A text that omits required ownership wording.
///
/// Output:
/// - Test passes when at least one missing-term diagnostic is produced.
///
/// Transformation:
/// - Prevents the VM binary documentation from drifting back into separate
///   product/distribution language.
#[test]
fn vm_readme_text_rejects_missing_terms() {
    let diagnostics = validate_vm_readme_text("terlan-vm runtime");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("internal compiler/runtime")),
        "diagnostics should report missing ownership wording: {diagnostics:?}"
    );
}

/// Verifies VM README ownership wording rejects placeholders.
///
/// Inputs:
/// - Complete required terms plus placeholder planning text.
///
/// Output:
/// - Test passes when placeholder text is reported.
///
/// Transformation:
/// - Keeps the internal VM binary contract release-ready rather than
///   future-planned.
#[test]
fn vm_readme_text_rejects_placeholder_wording() {
    let text = format!(
        "{}\nTODO: split the VM crate later.",
        REQUIRED_VM_README_TERMS.join("\n")
    );
    let diagnostics = validate_vm_readme_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder VM README ownership text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

/// Verifies VM entrypoint text accepts compiler/runtime-only imports.
///
/// Inputs:
/// - A compact standalone VM entrypoint fixture without backend aliases.
///
/// Output:
/// - Test passes when no backend-import diagnostics are produced.
///
/// Transformation:
/// - Locks the standalone VM binary to VM/compiler modules instead of the
///   Erlang backend.
#[test]
fn vm_main_text_accepts_vm_owned_imports() {
    let text = r#"
#[path = "commands.rs"]
pub mod commands;
#[path = "../compiler/mod.rs"]
pub mod compiler;
pub(crate) use compiler::syntax as terlan_syntax;
"#;

    assert!(validate_vm_main_text(text).is_empty());
}

/// Verifies VM entrypoint text rejects Erlang backend imports.
///
/// Inputs:
/// - A compact standalone VM entrypoint fixture that imports the Erlang backend.
///
/// Output:
/// - Test passes when the gate reports the forbidden backend fragments.
///
/// Transformation:
/// - Prevents VM lowering modules from being compiled into `terlan-vm` as an
///   active dependency.
#[test]
fn vm_main_text_rejects_erlang_backend_imports() {
    let text = r#"
#[path = "../backends/mod.rs"]
pub mod backends;
pub(crate) use backends::erlang as terlan_erlang;
"#;

    let diagnostics = validate_vm_main_text(text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("../backends/mod.rs")),
        "diagnostics should report backend path import: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan_erlang")),
        "diagnostics should report Erlang alias: {diagnostics:?}"
    );
}
