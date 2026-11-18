use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
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

/// Verifies release rules may live in recursively included Make fragments.
#[test]
fn make_graph_reads_recursive_literal_includes_once() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-vm-release-install-make-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("mk")).expect("fixture directory should be created");
    fs::write(
        root.join("Makefile"),
        "include mk/release.mk\ninclude mk/release.mk\n-include mk/missing.mk\n",
    )
    .expect("root Makefile should be written");
    fs::write(
        root.join("mk/release.mk"),
        "include mk/install.mk\npublish-preflight:\n",
    )
    .expect("release fragment should be written");
    fs::write(root.join("mk/install.mk"), "upgrade-local:\n")
        .expect("install fragment should be written");

    let graph = read_make_graph_text(&root).expect("literal include graph should load");

    assert!(graph.contains("publish-preflight:"));
    assert!(graph.contains("upgrade-local:"));
    assert_eq!(graph.matches("# source: mk/release.mk").count(), 1);
    fs::remove_dir_all(root).expect("fixture directory should be removed");
}

/// Verifies dynamic or escaping includes cannot make validation incomplete.
#[test]
fn make_include_paths_rejects_unbounded_paths() {
    for source in [
        "include $(FRAGMENT)\n",
        "include ../outside.mk\n",
        "include /tmp/file.mk\n",
    ] {
        let error = make_include_paths("Makefile", source)
            .expect_err("unbounded include should fail closed");
        assert!(error.contains("literal repository-relative path"));
    }
}

/// Verifies every supported optional include spelling is discovered.
#[test]
fn make_include_paths_accepts_optional_literal_paths() {
    let paths = make_include_paths(
        "Makefile",
        "-include mk/optional.mk\nsinclude mk/secondary.mk\n",
    )
    .expect("literal optional includes should be accepted");

    assert_eq!(
        paths,
        [
            ("mk/optional.mk".to_string(), true),
            ("mk/secondary.mk".to_string(), true)
        ]
    );
}

/// Verifies release installer smoke accepts the public platform-field syntax.
///
/// Inputs:
/// - A compact installer smoke fixture using `platform.lsp`.
///
/// Output:
/// - Test passes when every release installer term is recognized.
///
/// Transformation:
/// - Prevents release validation from retaining retired getter-call spellings
///   after public struct fields replace accessor functions.
#[test]
fn release_installer_terms_accept_platform_field_access() {
    let source = r#"
install.sh
install.ps1
validate-package
terlan-native-worker
let lsp = ReleasePayload.child(install_bin, platform.lsp);
ReleaseSmoke.lsp_help_matches(root, lsp)
"#;

    assert!(validate_required_terms_text(
        RELEASE_INSTALLER_SMOKE_SOURCE,
        source,
        REQUIRED_RELEASE_INSTALLER_SMOKE_TERMS,
    )
    .is_empty());
}

/// Verifies release installer smoke cannot omit the LSP platform field.
#[test]
fn release_installer_terms_reject_missing_platform_lsp() {
    let source = r#"
install.sh
install.ps1
validate-package
terlan-native-worker
ReleaseSmoke.lsp_help_matches(root, lsp)
"#;
    let diagnostics = validate_required_terms_text(
        RELEASE_INSTALLER_SMOKE_SOURCE,
        source,
        REQUIRED_RELEASE_INSTALLER_SMOKE_TERMS,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("platform.lsp")),
        "diagnostics should report missing LSP platform field: {diagnostics:?}"
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
