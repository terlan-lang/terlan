use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the VM release/install validation gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmReleaseInstallValidationSummary {
    pub checked_file_count: usize,
    pub required_rule_count: usize,
}

const REQUIRED_INSTALL_SH_TERMS: &[&str] = &[
    "TERLAN_VERSION",
    "terlc-${TERLAN_OS}-${TERLAN_ARCH}.tar.gz",
    "terlan-vm",
    "\"$INSTALL_DIR/terlc\" --version",
    "\"$INSTALL_DIR/terlan-vm\" --version",
    "\"$INSTALL_DIR/terlan-vm\" validate-package \"$SHARE_DIR\"",
    "runtime/release-self-test.tvm",
];

const REQUIRED_INSTALL_PS1_TERMS: &[&str] = &[
    "TERLAN_VERSION",
    "terlc-windows-$terlanArch.zip",
    "terlan-vm.exe",
    "terlc.exe",
    "--version",
    "validate-package $shareDestination",
    "runtime\\release-self-test.tvm",
];

const REQUIRED_PACKAGE_HELPER_TERMS: &[&str] = &[
    "compiler_binary_name",
    "vm_binary_name",
    "artifact did not contain",
    "terlan-vm",
    "subprocess.check_output([str(vm_binary), \"--version\"]",
    "glob(\"*.tvm\")",
    "vm_release.Main.main",
    "installer did not install",
    "actual_vm = subprocess.check_output([str(vm), \"--version\"]",
    "package-image-metadata",
    "validate-package",
    "native_self_test",
    "release-self-test.tvm",
];

const REQUIRED_CLI_MK_TERMS: &[&str] = &[
    "cli-build:",
    "cli-test-full:",
    "cli-release-artifact-current:",
    "--bin terlc --bin terlan-vm",
];

const REQUIRED_MAKEFILE_TERMS: &[&str] = &[
    ".SHELLFLAGS := -eo pipefail -c",
    "publish-preflight:",
    "$(MAKE) check",
    "test-release:",
    "$(MAKE) release-artifact-current",
    "vm-release-install-validation-check",
    "tvm-aot-package-install-consumer-check",
];

const REQUIRED_LOCAL_UPGRADE_TERMS: &[&str] = &[
    "upgrade-local:",
    "LOCAL_TERLAN_VM",
    "--bin terlc --bin terlan-vm",
    "install -m 0755 target/release/terlc",
    "install -m 0755 target/release/terlan-vm",
    "\"$$vm_install_path\" --version",
];

/// Runs the VM release/install validation gate.
///
/// Inputs:
/// - `root`: repository root containing installers, editor package metadata,
///   release packaging helper, and Makefile.
///
/// Output:
/// - Success summary when release/install metadata is synchronized for the
///   VM-first release line.
/// - Stable diagnostics when installers, release artifacts, or editor package
///   artifacts drift.
///
/// Transformation:
/// - Reads release-facing metadata without building artifacts, then enforces
///   that `terlc` and `terlan-vm` ship together and stale editor `.vsix`
///   packages cannot be committed.
pub fn run_vm_release_install_validation(
    root: &Path,
) -> QualityResult<VmReleaseInstallValidationSummary> {
    let workspace_version = read_workspace_version(root)?;
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "install.sh",
        REQUIRED_INSTALL_SH_TERMS,
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "install.ps1",
        REQUIRED_INSTALL_PS1_TERMS,
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "tools/package_release_artifact.py",
        REQUIRED_PACKAGE_HELPER_TERMS,
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/cli.mk",
        REQUIRED_CLI_MK_TERMS,
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "Makefile",
        REQUIRED_MAKEFILE_TERMS,
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "Makefile",
        REQUIRED_LOCAL_UPGRADE_TERMS,
    )?);
    diagnostics.extend(validate_installer_versions(root, &workspace_version)?);
    diagnostics.extend(validate_editor_package_versions(root, &workspace_version)?);
    diagnostics.extend(validate_vsix_artifacts(root, &workspace_version)?);
    diagnostics.extend(validate_release_smoke_does_not_use_erlang(root)?);

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(VmReleaseInstallValidationSummary {
        checked_file_count: 7,
        required_rule_count: REQUIRED_INSTALL_SH_TERMS.len()
            + REQUIRED_INSTALL_PS1_TERMS.len()
            + REQUIRED_PACKAGE_HELPER_TERMS.len()
            + REQUIRED_CLI_MK_TERMS.len()
            + REQUIRED_MAKEFILE_TERMS.len()
            + REQUIRED_LOCAL_UPGRADE_TERMS.len()
            + 7,
    })
}

/// Reads the workspace package version from the root `Cargo.toml`.
fn read_workspace_version(root: &Path) -> QualityResult<String> {
    let text = read_repo_text(root, "Cargo.toml")?;
    for line in text.lines() {
        let stripped = line.trim();
        if let Some(version) = stripped.strip_prefix("version = \"") {
            if let Some((version, _)) = version.split_once('"') {
                return Ok(version.to_string());
            }
        }
    }
    Err("Cargo.toml: missing workspace package version".to_string())
}

/// Validates one repository file contains required text fragments.
fn validate_required_terms(
    root: &Path,
    relative: &str,
    required_terms: &[&str],
) -> QualityResult<Vec<String>> {
    let text = read_repo_text(root, relative)?;
    Ok(validate_required_terms_text(
        relative,
        &text,
        required_terms,
    ))
}

/// Validates required text fragments in already-read file text.
fn validate_required_terms_text(
    relative: &str,
    text: &str,
    required_terms: &[&str],
) -> Vec<String> {
    required_terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing required release/install term `{term}`"))
        .collect()
}

/// Validates installer default versions match the workspace version.
fn validate_installer_versions(root: &Path, workspace_version: &str) -> QualityResult<Vec<String>> {
    let install_sh = read_repo_text(root, "install.sh")?;
    let install_ps1 = read_repo_text(root, "install.ps1")?;
    let mut diagnostics = Vec::new();
    let expected_sh = format!("VERSION=\"${{TERLAN_VERSION:-v{workspace_version}}}\"");
    if !install_sh.contains(&expected_sh) {
        diagnostics.push(format!(
            "install.sh: default version must be v{workspace_version}"
        ));
    }
    let expected_ps1 = format!("$Version = \"v{workspace_version}\"");
    if !install_ps1.contains(&expected_ps1) {
        diagnostics.push(format!(
            "install.ps1: default version must be v{workspace_version}"
        ));
    }
    Ok(diagnostics)
}

/// Validates checked-in VS Code package metadata matches the workspace version.
fn validate_editor_package_versions(
    root: &Path,
    workspace_version: &str,
) -> QualityResult<Vec<String>> {
    let paths = [
        "editors/vscode/package.json",
        "editors/vscode/package-lock.json",
        "editors/vscode/node_modules/.package-lock.json",
    ];
    let mut diagnostics = Vec::new();
    for path in paths {
        let text = read_repo_text(root, path)?;
        diagnostics.extend(validate_editor_package_version_text(
            path,
            &text,
            workspace_version,
        ));
    }
    Ok(diagnostics)
}

/// Validates one package metadata text uses the workspace version.
fn validate_editor_package_version_text(
    relative: &str,
    text: &str,
    workspace_version: &str,
) -> Vec<String> {
    let expected = format!("\"version\": \"{workspace_version}\"");
    if text.contains(&expected) {
        Vec::new()
    } else {
        vec![format!(
            "{relative}: VS Code package metadata must use workspace version {workspace_version}"
        )]
    }
}

/// Validates checked-in VS Code package artifacts are fresh or absent.
fn validate_vsix_artifacts(root: &Path, workspace_version: &str) -> QualityResult<Vec<String>> {
    let vscode_dir = root.join("editors/vscode");
    let mut diagnostics = Vec::new();
    for entry in fs::read_dir(&vscode_dir)
        .map_err(|err| format!("{}: failed to read directory: {err}", vscode_dir.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "{}: failed to read directory entry: {err}",
                vscode_dir.display()
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("vsix") {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let expected = format!("terlan-vscode-{workspace_version}.vsix");
        if file_name != expected {
            diagnostics.push(format!(
                "editors/vscode/{file_name}: stale VS Code package artifact; remove it or regenerate {expected}"
            ));
        }
    }
    Ok(diagnostics)
}

/// Validates release artifact smoke uses VM/browser paths rather than Erlang.
fn validate_release_smoke_does_not_use_erlang(root: &Path) -> QualityResult<Vec<String>> {
    let text = read_repo_text(root, "tools/package_release_artifact.py")?;
    let mut diagnostics = Vec::new();
    if text.contains("\"erlang\"") || text.contains("'erlang'") {
        diagnostics.push(
            "tools/package_release_artifact.py: release smoke must not build Erlang artifacts"
                .to_string(),
        );
    }
    if !text.contains("--target") || !text.contains("terlan-vm") {
        diagnostics.push(
            "tools/package_release_artifact.py: release smoke must exercise VM artifacts"
                .to_string(),
        );
    }
    let transitional_artifact_glob = ["*.tvm", ".json"].concat();
    if text.contains(&transitional_artifact_glob) {
        diagnostics.push(
            "tools/package_release_artifact.py: release smoke must not accept transitional JSON artifacts"
                .to_string(),
        );
    }
    if !text.contains("glob(\"*.tvm\")")
        || !text.contains("\"--entry\"")
        || !text.contains("\"--test-eval\"")
    {
        diagnostics.push(
            "tools/package_release_artifact.py: release smoke must execute the emitted native TVM image"
                .to_string(),
        );
    }
    Ok(diagnostics)
}

/// Reads one repository text file.
fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|err| format!("{relative}: failed to read file: {err}"))
}

/// Renders VM release/install validation diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-release-install-validation] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_release_install_validation_test.rs"]
mod vm_release_install_validation_test;
