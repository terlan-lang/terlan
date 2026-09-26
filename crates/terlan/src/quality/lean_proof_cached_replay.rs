//! Adapts independent replay to its input/tool-bound receipt owner.

use super::*;
use checkpoint::Checkpoints;
use tools::ProofTools;

/// Pins every current replica identity before cache retention runs.
pub(super) fn open(
    root: &Path,
    current: &[(&ArtifactRow, ReplayMetadata)],
    tools: &BTreeMap<ToolchainContract, ProofTools>,
) -> QualityResult<Checkpoints> {
    let mut protected = BTreeSet::new();
    for (artifact, metadata) in current {
        let workspace = snapshot(root, metadata)?;
        protected.insert(key(
            &workspace,
            metadata,
            artifact.expected_exit,
            &tools[&metadata.toolchain],
        )?);
        workspace.close()?;
    }
    Checkpoints::open(root, protected)
}

/// Reuses matching evidence; only an explicitly supplied environment may execute.
pub(super) fn execute(
    root: &Path,
    metadata: &ReplayMetadata,
    expected_exit: i32,
    environment: Option<&ProofEnvironment>,
    tools: &ProofTools,
    checkpoints: &mut Checkpoints,
    replica: u8,
) -> QualityResult<NormalizedExecution> {
    let workspace = snapshot(root, metadata)?;
    let input = key(&workspace, metadata, expected_exit, tools)?;
    tools.verify_unchanged()?;
    let result = checkpoints.run(&input, replica, || {
        let environment =
            environment.ok_or("completed proof receipt is missing; rerun its preparation owner")?;
        let mut command = Command::new(tools.lake());
        command
            .args(&metadata.execution_command[1..])
            .current_dir(workspace.root().join(&metadata.working_directory));
        environment.configure(
            &mut command,
            workspace.root(),
            &metadata.toolchain.elan_channel,
        )?;
        let output = run_proof_command(&mut command, Duration::from_secs(600))?;
        let output = normalize_execution(workspace.root(), output);
        validate_execution(metadata, expected_exit, &output)?;
        tools.verify_unchanged()?;
        Ok(output)
    });
    let unchanged = tools.verify_unchanged();
    let cleanup = workspace.close();
    let output = result?;
    unchanged?;
    cleanup?;
    validate_execution(metadata, expected_exit, &output)?;
    Ok(output)
}

fn snapshot(root: &Path, metadata: &ReplayMetadata) -> QualityResult<ProofWorkspace> {
    let mut dependencies = metadata.dependency_files.clone();
    dependencies.extend(metadata.manifest_fingerprints.keys().cloned());
    let workspace = ProofWorkspace::create(root, &dependencies)?;
    validate_snapshot(workspace.root(), metadata)?;
    Ok(workspace)
}

fn key(
    workspace: &ProofWorkspace,
    metadata: &ReplayMetadata,
    expected_exit: i32,
    tools: &ProofTools,
) -> QualityResult<String> {
    let identity = serde_json::to_vec(&json!({
        "schema": "terlan.proof-replica-input.v1",
        "inputs": workspace.input_identity(),
        "metadata": metadata,
        "expected_exit": expected_exit,
        "tools": tools.identity(),
    }))
    .map_err(|error| error.to_string())?;
    Ok(hex_digest(&Sha256::digest(identity)))
}
