use super::*;
/// Verifies required release/install terms are accepted.
///
/// Inputs:
/// - A compact text containing required terms.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Exercises the shared term validator without reading repository files.
#[test]
fn required_terms_text_accepts_all_terms() {
    let text = REQUIRED_INSTALL_SH_TERMS.join("\n");

    assert!(
        validate_required_terms_text("install.sh", &text, REQUIRED_INSTALL_SH_TERMS).is_empty()
    );
}

/// Verifies missing release/install terms are reported.
///
/// Inputs:
/// - Text missing the `terlan-vm` release artifact requirement.
///
/// Output:
/// - Test passes when the missing VM term is reported.
///
/// Transformation:
/// - Prevents installer smoke checks from drifting back to compiler-only
///   artifacts.
#[test]
fn required_terms_text_rejects_missing_vm_term() {
    let diagnostics =
        validate_required_terms_text("install.sh", "terlc", REQUIRED_INSTALL_SH_TERMS);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm")),
        "diagnostics should report missing VM binary term: {diagnostics:?}"
    );
}

/// Verifies local upgrades install the VM beside the compiler.
///
/// Inputs:
/// - A compiler-only `upgrade-local` fixture.
///
/// Output:
/// - Test passes when the missing VM install contract is reported.
///
/// Transformation:
/// - Prevents local installs from drifting away from the release packaging
///   contract now that `terlan-vm` is the default runtime.
#[test]
fn local_upgrade_terms_reject_compiler_only_install() {
    let text = r#"
upgrade-local:
	$(CARGO) build --release -p terlan --bin terlc
	install -m 0755 target/release/terlc "$$install_path"
"#;
    let diagnostics = validate_required_terms_text("Makefile", text, REQUIRED_LOCAL_UPGRADE_TERMS);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm")),
        "diagnostics should report missing local VM install contract: {diagnostics:?}"
    );
}

/// Verifies editor package metadata accepts the workspace version.
///
/// Inputs:
/// - Minimal package metadata using the current workspace version.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Keeps VS Code package metadata tied to release metadata.
#[test]
fn editor_package_version_text_accepts_workspace_version() {
    let text = r#"{ "version": "0.0.7" }"#;

    assert!(validate_editor_package_version_text("package.json", text, "0.0.7").is_empty());
}

/// Verifies stale editor package metadata is rejected.
///
/// Inputs:
/// - Minimal package metadata using an older version.
///
/// Output:
/// - Test passes when the stale version is reported.
///
/// Transformation:
/// - Prevents editor packages from lagging behind compiler releases.
#[test]
fn editor_package_version_text_rejects_stale_version() {
    let diagnostics = validate_editor_package_version_text(
        "editors/vscode/package.json",
        r#"{ "version": "0.0.5" }"#,
        "0.0.7",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("0.0.7")),
        "diagnostics should report expected workspace version: {diagnostics:?}"
    );
}

/// Verifies release smoke diagnostics reject Erlang artifacts.
///
/// Inputs:
/// - A compact release helper fixture that still builds `--target erlang`.
///
/// Output:
/// - Test passes when the Erlang smoke path is rejected.
///
/// Transformation:
/// - Locks the release artifact smoke onto the VM-first release line.
#[test]
fn release_smoke_text_rejects_erlang_target_marker() {
    let source = r#"run(["build", "--target", "terlan-vm", "erlang"])
run(["--entry", "vm_release.Main.main", "--test-eval"])"#;
    let diagnostics = validate_release_smoke_text(RELEASE_SMOKE_SOURCE, source);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Erlang")),
        "diagnostics should reject Erlang release smoke: {diagnostics:?}"
    );
}

/// Verifies release smoke diagnostics reject transitional artifact discovery.
#[test]
fn release_smoke_text_requires_native_tvm_execution() {
    let source = r#"run(["build", "--target", "terlan-vm"])
let transitional = "*.tvm.json"."#;
    let diagnostics = validate_release_smoke_text(RELEASE_SMOKE_SOURCE, source);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.contains("transitional JSON")
            || diagnostic.contains("execute the emitted native TVM image")
    }));
}
