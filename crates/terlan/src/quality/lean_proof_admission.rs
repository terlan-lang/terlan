//! Fresh tool admission for a preparation owner; execution rechecks content, not probes.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    environment::ProofEnvironment, format_digest, read_metadata, tools, validate_metadata,
    validate_toolchain, ArtifactRow, QualityResult, ToolchainContract,
};

const SCHEMA: &str = "terlan.proof-tool-admission.v1";
const MAX_DOCUMENT: u64 = 16 * 1024 * 1024;
const VARIABLES: [&str; 2] = [
    "TERLAN_PROOF_TOOL_ADMISSION",
    "TERLAN_PROOF_TOOL_ADMISSION_SHA256",
];

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    root: PathBuf,
    producer: PathBuf,
    utc_date: String,
    environment: String,
    tools: Vec<Entry>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    contract: ToolchainContract,
    tool: tools::Admission,
}

fn utc_date() -> String {
    super::super::lean_proof_gap::current_utc_date().to_string()
}

fn private_path(root: &Path, path: &Path) -> QualityResult<()> {
    let parent = path.parent().ok_or("proof admission has no parent")?;
    let namespace = fs::canonicalize(root)
        .map_err(|error| error.to_string())?
        .join("target/quality/preparation");
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || !parent.starts_with(&namespace)
        || parent == namespace
        || !parent
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".work"))
        || fs::canonicalize(parent).map_err(|error| error.to_string())? != parent
    {
        return Err(
            "proof tool admission requires a canonical private preparation workspace".into(),
        );
    }
    Ok(())
}

/// Admit each selected toolchain once and write a stable, content-bound handoff.
pub(in super::super) fn capture(
    root: &Path,
    path: &Path,
    artifacts: &[ArtifactRow],
) -> QualityResult<String> {
    private_path(root, path)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
        Ok(_) => return Err("proof tool admission destination already exists".into()),
    }
    let mut contracts = BTreeSet::new();
    for artifact in artifacts
        .iter()
        .filter(|artifact| artifact.status == "current")
    {
        let metadata = read_metadata(root, &artifact.replay_metadata)?;
        validate_metadata(root, artifact, &metadata)?;
        contracts.insert(metadata.toolchain);
    }
    if contracts.is_empty() || contracts.len() > 64 {
        return Err("proof admission requires between one and 64 toolchain contracts".into());
    }
    let environment = ProofEnvironment::capture()?;
    let date = utc_date();
    let mut captured = Vec::new();
    for contract in contracts {
        validate_toolchain(root, &contract, &environment)?;
        let tool = tools::ProofTools::capture(root, &environment, &contract)?;
        captured.push((contract, tool));
    }
    for (_, tool) in &captured {
        tool.verify_unchanged()?;
    }
    if date != utc_date() {
        return Err("UTC date changed during proof tool admission".into());
    }
    let document = Document {
        schema: SCHEMA.into(),
        root: fs::canonicalize(root).map_err(|error| error.to_string())?,
        producer: std::env::current_exe().map_err(|error| error.to_string())?,
        utc_date: date,
        environment: environment.identity()?,
        tools: captured
            .into_iter()
            .map(|(contract, tool)| Entry {
                contract,
                tool: tool.admission(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&document).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT {
        return Err("proof admission document exceeds its byte budget".into());
    }
    let mut file = File::create_new(path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(format_digest(&Sha256::digest(&bytes)))
}

/// Optional handoffs are all-or-nothing and never fall back after rejection.
pub(super) fn from_environment(
    root: &Path,
    environment: &ProofEnvironment,
    contracts: &BTreeSet<ToolchainContract>,
) -> QualityResult<Option<BTreeMap<ToolchainContract, tools::ProofTools>>> {
    let values = VARIABLES.map(std::env::var_os);
    let Some((path, digest)) = assignment(values)? else {
        return Ok(None);
    };
    load(root, &path, &digest, environment, contracts).map(Some)
}

fn assignment(values: [Option<OsString>; 2]) -> QualityResult<Option<(PathBuf, String)>> {
    match values {
        [None, None] => Ok(None),
        [Some(path), Some(digest)] if !path.is_empty() => {
            let digest = digest
                .into_string()
                .map_err(|_| "invalid admission digest encoding")?;
            if digest.strip_prefix("sha256:").is_none_or(|hex| {
                hex.len() != 64
                    || !hex
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            }) {
                return Err("invalid proof admission digest".into());
            }
            Ok(Some((path.into(), digest)))
        }
        _ => Err("proof tool admission requires both path and digest".into()),
    }
}

fn load(
    root: &Path,
    path: &Path,
    expected: &str,
    environment: &ProofEnvironment,
    contracts: &BTreeSet<ToolchainContract>,
) -> QualityResult<BTreeMap<ToolchainContract, tools::ProofTools>> {
    private_path(root, path)?;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_DOCUMENT {
        return Err("proof admission must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_DOCUMENT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT || format_digest(&Sha256::digest(&bytes)) != expected {
        return Err("proof tool admission bytes do not match the declared digest".into());
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if document.schema != SCHEMA
        || document.root != fs::canonicalize(root).map_err(|error| error.to_string())?
        || document.producer != std::env::current_exe().map_err(|error| error.to_string())?
        || document.utc_date != utc_date()
        || document.environment != environment.identity()?
        || document.tools.len() > 64
        || document.tools.len() != contracts.len()
        || document
            .tools
            .iter()
            .map(|entry| entry.contract.clone())
            .collect::<BTreeSet<_>>()
            != *contracts
    {
        return Err("proof tool admission context or selected contracts changed".into());
    }
    let mut tools = BTreeMap::new();
    for entry in document.tools {
        super::validate_toolchain_inputs(root, &entry.contract)?;
        let tool = tools::ProofTools::resume(entry.tool, environment, &entry.contract)?;
        tools.insert(entry.contract, tool);
    }
    Ok(tools)
}

#[cfg(test)]
#[path = "lean_proof_admission_test.rs"]
mod tests;
