//! Linux proof-tool identity shared by the independent replica owners.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    environment::ProofEnvironment, run_proof_command, workspace::ProofWorkspace, ToolchainContract,
};
use crate::terlan_quality::QualityResult;

#[path = "lean_proof_tool_files.rs"]
mod files;
use files::ToolFiles;

/// A content-bound handoff from this producer's fresh tool admission command.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Admission {
    lake: PathBuf,
    files: files::Reference,
    identity: String,
}

/// Resolved native Lake and verified tool contents, never an unchecked version key.
pub(super) struct ProofTools {
    lake: PathBuf,
    files: ToolFiles,
    identity: String,
}

impl ProofTools {
    /// Produces a stable reference; unchanged admissions have identical bytes.
    pub(super) fn admission(&self) -> Admission {
        Admission {
            lake: self.lake.clone(),
            files: self.files.reference(),
            identity: self.identity.clone(),
        }
    }

    /// Recheck actual bytes and the effective environment before using admission.
    pub(super) fn resume(
        admission: Admission,
        environment: &ProofEnvironment,
        contract: &ToolchainContract,
    ) -> QualityResult<Self> {
        let files = ToolFiles::resume(admission.files)?;
        let identity = tool_identity(&files, environment, contract)?;
        if identity != admission.identity || !files.contains_tool(&admission.lake) {
            return Err("proof tool admission identity mismatch".into());
        }
        Ok(Self {
            lake: admission.lake,
            files,
            identity,
        })
    }

    /// Resolves the explicit Elan pin and includes native-loader dependencies.
    pub(super) fn capture(
        root: &Path,
        environment: &ProofEnvironment,
        contract: &ToolchainContract,
    ) -> QualityResult<Self> {
        let elan = environment.program("elan")?;
        let mut resolved = Vec::new();
        for name in ["lean", "lake"] {
            let output = probe(root, environment, contract, &elan, &["which", name], false)?;
            let path = PathBuf::from(output.trim());
            if !path.is_absolute() || !path.is_file() {
                return Err(format!("Elan did not resolve an absolute installed {name}"));
            }
            resolved.push(fs::canonicalize(path).map_err(|error| error.to_string())?);
        }
        let lean = &resolved[0];
        let lake = &resolved[1];
        let bin = lean.parent().ok_or("Lean tool has no directory")?;
        if lake.parent() != Some(bin)
            || bin.file_name().and_then(|value| value.to_str()) != Some("bin")
        {
            return Err("Lean and Lake must belong to one installed toolchain".into());
        }
        let tree = bin.parent().ok_or("Lean installation has no root")?;
        let mut extras = BTreeSet::from([
            elan,
            std::env::current_exe().map_err(|error| error.to_string())?,
            PathBuf::from("/etc/ld.so.cache"),
            PathBuf::from("/etc/ld.so.preload"),
        ]);
        for program in &resolved {
            let listing = probe(root, environment, contract, program, &[], true)?;
            extras.extend(loader_dependencies(&listing)?);
        }
        let files = ToolFiles::capture(tree, &extras.into_iter().collect::<Vec<_>>())?;
        let version = probe(root, environment, contract, lean, &["--version"], false)?;
        if !super::environment::matches_version(&version, &contract.lean_version) {
            return Err("resolved native Lean does not match the pinned version".into());
        }
        files.verify_unchanged()?;
        let identity = tool_identity(&files, environment, contract)?;
        Ok(Self {
            lake: lake.clone(),
            files,
            identity,
        })
    }

    /// Native Lake is fixed before execution; Elan cannot select another tool.
    pub(super) fn lake(&self) -> &Path {
        &self.lake
    }

    /// Complete captured file and environment identity for a replica key.
    pub(super) fn identity(&self) -> &str {
        &self.identity
    }

    /// Fail closed on mutation before consuming or publishing a checkpoint.
    pub(super) fn verify_unchanged(&self) -> QualityResult<()> {
        self.files.verify_unchanged()
    }
}

fn tool_identity(
    files: &ToolFiles,
    environment: &ProofEnvironment,
    contract: &ToolchainContract,
) -> QualityResult<String> {
    Ok(super::format_digest(&Sha256::digest(
        format!(
            "terlan.proof-tools.v1\n{}\n{}\n{}\n{}\n{}",
            files.digest(),
            environment.identity()?,
            contract.elan_channel,
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
        .as_bytes(),
    )))
}

fn probe(
    root: &Path,
    environment: &ProofEnvironment,
    contract: &ToolchainContract,
    program: &Path,
    arguments: &[&str],
    loader: bool,
) -> QualityResult<String> {
    let workspace = ProofWorkspace::create(root, &[])?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(workspace.root().join("proofs/lean"));
    environment.configure(&mut command, workspace.root(), &contract.elan_channel)?;
    if loader {
        // The GNU loader reports its resolved dependency closure without running main.
        command.env("LD_TRACE_LOADED_OBJECTS", "1");
    }
    let result = run_proof_command(&mut command, Duration::from_secs(30));
    let cleanup = workspace.close();
    let output = result?;
    cleanup?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "proof tool identity probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn loader_dependencies(output: &str) -> QualityResult<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("linux-vdso.so.") || line.starts_with("linux-gate.so.") {
            continue;
        }
        let resolved = line.split_once(" => ").map_or(line, |(_, path)| path);
        let (path, address) = resolved
            .rsplit_once(" (")
            .ok_or("invalid proof loader dependency")?;
        if !Path::new(path).is_absolute() || !address.starts_with("0x") || !address.ends_with(')') {
            return Err("unresolved or malformed proof loader dependency".into());
        }
        paths.insert(PathBuf::from(path));
    }
    if paths.is_empty() {
        return Err("proof checkpoints require an inspectable GNU native loader".into());
    }
    Ok(paths)
}

#[cfg(test)]
#[path = "lean_proof_tools_test.rs"]
mod tests;
