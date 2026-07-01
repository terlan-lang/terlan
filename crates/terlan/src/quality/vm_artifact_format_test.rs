use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_artifact_format, validate_vm_artifact_format_text};

/// Minimal complete VM artifact contract fixture.
const VALID_CONTRACT: &str = r#"
# Terlan VM Artifact Format

Status: 0.0.7 baseline contract.

The artifact is derived from CoreIR. It is not Erlang source, not BEAM bytecode,
and not a NIF ABI. The artifact is deterministic.

schema_version
artifact_kind
compiler_version
target_profile
module
exports
functions
types
constants
capabilities
native_boundary
source_map
debug
checksum

Validation rejects invalid artifacts.

## Non-Goals

The default path does not promise OTP compatibility.
"#;

/// Temporary repository fixture for VM artifact checks.
///
/// Inputs:
/// - Created with a unique path under the system temporary directory.
///
/// Output:
/// - Fixture root path and automatic cleanup on drop.
///
/// Transformation:
/// - Provides a tiny repo-shaped directory without external test dependencies.
struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    /// Creates an empty repository fixture.
    ///
    /// Inputs:
    /// - `name`: diagnostic name segment for the temporary directory.
    ///
    /// Output:
    /// - New fixture root.
    ///
    /// Transformation:
    /// - Combines process id and time into a unique temp path, then creates
    ///   the root directory.
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-quality-vm-artifact-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Returns the fixture root path.
    ///
    /// Inputs:
    /// - The fixture.
    ///
    /// Output:
    /// - Borrowed repository root path.
    ///
    /// Transformation:
    /// - Exposes the root as a `Path` for quality-check execution.
    fn root(&self) -> &Path {
        &self.root
    }

    /// Writes the VM artifact contract fixture file.
    ///
    /// Inputs:
    /// - `text`: contract content.
    ///
    /// Output:
    /// - `Ok(())` when the file is written.
    ///
    /// Transformation:
    /// - Creates the runtime docs directory and writes UTF-8 text.
    fn write_contract(&self, text: &str) -> io::Result<()> {
        let path = self.root.join("docs/runtime/TERLAN_VM_ARTIFACT_FORMAT.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }
}

impl Drop for TestRepo {
    /// Removes the temporary repository fixture.
    ///
    /// Inputs:
    /// - The fixture root path.
    ///
    /// Output:
    /// - Best-effort cleanup.
    ///
    /// Transformation:
    /// - Deletes the temporary directory and ignores cleanup failures.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Verifies the contract check accepts a complete minimal artifact spec.
///
/// Inputs:
/// - Repository fixture containing all required semantic groups.
///
/// Output:
/// - Successful summary with enforced requirement count.
///
/// Transformation:
/// - Runs the same public checker used by Make and CI.
#[test]
fn vm_artifact_format_accepts_complete_contract() {
    let repo = TestRepo::new("complete").expect("create fixture");
    repo.write_contract(VALID_CONTRACT)
        .expect("write contract fixture");

    let summary = run_vm_artifact_format(repo.root()).expect("contract should pass");

    assert!(summary.required_group_count > 10);
}

/// Verifies missing contract files fail with a stable diagnostic.
///
/// Inputs:
/// - Empty repository fixture.
///
/// Output:
/// - Error mentioning the missing VM artifact contract.
///
/// Transformation:
/// - Exercises the public file-loading path instead of only text validation.
#[test]
fn vm_artifact_format_rejects_missing_contract_file() {
    let repo = TestRepo::new("missing").expect("create fixture");

    let error = run_vm_artifact_format(repo.root()).expect_err("contract should be missing");

    assert!(error.contains("failed to read VM artifact contract"));
}

/// Verifies the checker reports missing required contract language.
///
/// Inputs:
/// - Contract text missing the native boundary field.
///
/// Output:
/// - Diagnostic for the missing native boundary requirement.
///
/// Transformation:
/// - Confirms individual semantic groups produce actionable messages.
#[test]
fn vm_artifact_format_rejects_missing_required_group() {
    let text = VALID_CONTRACT.replace("native_boundary\n", "");

    let diagnostics = validate_vm_artifact_format_text(&text);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("native boundary")));
}

/// Verifies the checker blocks BEAM as the default runtime artifact.
///
/// Inputs:
/// - Otherwise valid contract with a forbidden default-runtime claim.
///
/// Output:
/// - Diagnostic naming the forbidden claim.
///
/// Transformation:
/// - Prevents accidental drift back to BEAM-default wording.
#[test]
fn vm_artifact_format_rejects_beam_default_claims() {
    let text = format!("{VALID_CONTRACT}\nBEAM bytecode is the default.\n");

    let diagnostics = validate_vm_artifact_format_text(&text);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("forbidden default-runtime claim")));
}
