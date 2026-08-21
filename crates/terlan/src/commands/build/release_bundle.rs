//! Deterministic Terlan Cloud release-bundle emission.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::commands::build::project_manifest::ProjectManifest;
use crate::commands::deploy::semantic_deploy_plan_value;
use crate::CliState;

const RELEASE_BUNDLE_SCHEMA: &str = "terlan-cloud-release-bundle-v1";
const RELEASE_SECTION_SCHEMA: &str = "terlan-cloud-release-section-v1";
const RELEASE_CHECKSUM_SCHEMA: &str = "terlan-cloud-release-checksums-v1";

#[derive(Debug, Clone, Serialize)]
pub(in crate::commands::build) struct FileIdentity {
    path: String,
    sha256: String,
    size: u64,
}

#[derive(Serialize)]
struct ArtifactIdentity {
    identity: String,
    files: Vec<FileIdentity>,
}

#[derive(Serialize)]
struct ReleaseGenerator {
    tool: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct ReleaseToolchain {
    compiler_version: &'static str,
    stdlib_version: &'static str,
}

#[derive(Serialize)]
struct ReleaseFiles {
    artifact: &'static str,
    deploy_plan: &'static str,
    health: &'static str,
    runtime: &'static str,
    routes: &'static str,
    capabilities: &'static str,
    sources: &'static str,
    checksums: &'static str,
}

#[derive(Serialize)]
struct ReleaseManifest {
    schema: &'static str,
    generated_by: ReleaseGenerator,
    toolchain: ReleaseToolchain,
    release: Value,
    target: Value,
    source_revision: Option<String>,
    artifact: ArtifactIdentity,
    routes: Value,
    sources: Value,
    migrations: Value,
    health_checks: Value,
    runtime_requirements: Value,
    rollback: Value,
    files: ReleaseFiles,
}

#[derive(Serialize)]
struct ReleaseChecksums {
    schema: &'static str,
    algorithm: &'static str,
    files: Vec<FileIdentity>,
}

/// Writes a complete release bundle below the selected build output root.
///
/// The bundle is staged and then published at `.terlan/release`, contains only
/// portable relative paths, and derives every deployment section from the
/// compiler's semantic deploy plan.
pub(in crate::commands::build) fn write_release_bundle(
    project_dir: &Path,
    manifest: &ProjectManifest,
    state: &CliState,
) -> Result<PathBuf, String> {
    let deploy_plan = semantic_deploy_plan_value(project_dir, manifest)?;
    let release_root = state.out_dir.join(".terlan").join("release");
    let staging_root = state.out_dir.join(".terlan").join("release.staging");
    remove_existing(&staging_root)?;
    fs::create_dir_all(staging_root.join("artifact")).map_err(|error| {
        format!(
            "cannot create release bundle staging directory {}: {error}",
            staging_root.display()
        )
    })?;

    let artifact_files = copy_executable_artifacts(&state.out_dir, &staging_root)?;
    let artifact_identity = artifact_set_identity(&artifact_files);

    write_json_file(&staging_root.join("deploy-plan.json"), &deploy_plan)?;
    write_json_file(
        &staging_root.join("health.json"),
        &json!({
            "schema": RELEASE_SECTION_SCHEMA,
            "services": required_field(&deploy_plan, "services")?,
        }),
    )?;
    write_json_file(
        &staging_root.join("runtime.json"),
        &json!({
            "schema": RELEASE_SECTION_SCHEMA,
            "target": required_field(&deploy_plan, "target")?,
            "services": required_field(&deploy_plan, "services")?,
            "resources": required_field(&deploy_plan, "resources")?,
            "outbound_network": required_field(&deploy_plan, "outbound_network")?,
            "native_packages": required_field(&deploy_plan, "native_packages")?,
        }),
    )?;
    write_json_file(
        &staging_root.join("routes.json"),
        &json!({
            "schema": RELEASE_SECTION_SCHEMA,
            "routes": required_field(&deploy_plan, "routes")?,
        }),
    )?;
    write_json_file(
        &staging_root.join("capabilities.json"),
        &json!({
            "schema": RELEASE_SECTION_SCHEMA,
            "capabilities": required_field(&deploy_plan, "capabilities")?,
            "configuration": required_field(&deploy_plan, "configuration")?,
        }),
    )?;
    write_json_file(
        &staging_root.join("sources.json"),
        &json!({
            "schema": RELEASE_SECTION_SCHEMA,
            "sources": required_field(&deploy_plan, "sources")?,
            "migrations": required_field(&deploy_plan, "migrations")?,
        }),
    )?;

    let manifest = ReleaseManifest {
        schema: RELEASE_BUNDLE_SCHEMA,
        generated_by: ReleaseGenerator {
            tool: "terlc",
            version: env!("CARGO_PKG_VERSION"),
        },
        toolchain: ReleaseToolchain {
            compiler_version: env!("CARGO_PKG_VERSION"),
            stdlib_version: env!("CARGO_PKG_VERSION"),
        },
        release: required_field(&deploy_plan, "release")?,
        target: required_field(&deploy_plan, "target")?,
        source_revision: None,
        artifact: ArtifactIdentity {
            identity: artifact_identity,
            files: artifact_files,
        },
        routes: required_field(&deploy_plan, "routes")?,
        sources: required_field(&deploy_plan, "sources")?,
        migrations: required_field(&deploy_plan, "migrations")?,
        health_checks: required_field(&deploy_plan, "services")?,
        runtime_requirements: json!({
            "services": required_field(&deploy_plan, "services")?,
            "resources": required_field(&deploy_plan, "resources")?,
            "outbound_network": required_field(&deploy_plan, "outbound_network")?,
            "capabilities": required_field(&deploy_plan, "capabilities")?,
            "native_packages": required_field(&deploy_plan, "native_packages")?,
        }),
        rollback: required_field(&deploy_plan, "rollback")?,
        files: ReleaseFiles {
            artifact: "artifact/",
            deploy_plan: "deploy-plan.json",
            health: "health.json",
            runtime: "runtime.json",
            routes: "routes.json",
            capabilities: "capabilities.json",
            sources: "sources.json",
            checksums: "checksums.json",
        },
    };
    write_json_file(&staging_root.join("manifest.json"), &manifest)?;

    let checksum_files = collect_file_identities(&staging_root, Some("checksums.json"))?;
    write_json_file(
        &staging_root.join("checksums.json"),
        &ReleaseChecksums {
            schema: RELEASE_CHECKSUM_SCHEMA,
            algorithm: "sha256",
            files: checksum_files,
        },
    )?;

    remove_existing(&release_root)?;
    fs::rename(&staging_root, &release_root).map_err(|error| {
        format!(
            "cannot publish release bundle {}: {error}",
            release_root.display()
        )
    })?;
    Ok(release_root)
}

fn copy_executable_artifacts(
    output_root: &Path,
    staging_root: &Path,
) -> Result<Vec<FileIdentity>, String> {
    let metadata_path = output_root.join(super::BUILD_PACKAGE_METADATA_FILE);
    let metadata_text = fs::read_to_string(&metadata_path).map_err(|error| {
        format!(
            "cannot read release package metadata {}: {error}",
            metadata_path.display()
        )
    })?;
    let metadata: Value = serde_json::from_str(&metadata_text).map_err(|error| {
        format!(
            "cannot parse release package metadata {}: {error}",
            metadata_path.display()
        )
    })?;
    let executable = metadata
        .get("executable")
        .and_then(Value::as_object)
        .ok_or_else(|| "release package metadata has no executable section".to_string())?;
    let mut paths = BTreeSet::new();
    for field in ["path", "image", "runtime", "native_worker"] {
        let value = executable
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("release executable metadata is missing `{field}`"))?;
        paths.insert(validate_portable_relative_path(value)?);
    }
    if let Some(value) = executable.get("service_runtime").and_then(Value::as_str) {
        paths.insert(validate_portable_relative_path(value)?);
    }

    let mut identities = Vec::new();
    for relative in paths {
        identities.push(copy_artifact_file(output_root, staging_root, &relative)?);
    }
    if let Some(value) = executable.get("web_root").and_then(Value::as_str) {
        let relative_root = validate_portable_relative_path(value)?;
        let source_root = output_root.join(&relative_root);
        if !source_root.is_dir() {
            return Err(format!(
                "release service web root is missing: {}",
                source_root.display()
            ));
        }
        let mut files = Vec::new();
        collect_files(&source_root, &source_root, None, &mut files)?;
        files.sort();
        for source in files {
            let nested = source
                .strip_prefix(&source_root)
                .expect("service file remains below web root");
            let relative = relative_root.join(nested);
            identities.push(copy_artifact_file(output_root, staging_root, &relative)?);
        }
    }
    identities.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(identities)
}

fn copy_artifact_file(
    output_root: &Path,
    staging_root: &Path,
    relative: &Path,
) -> Result<FileIdentity, String> {
    let source = output_root.join(relative);
    if !source.is_file() {
        return Err(format!("release artifact is missing: {}", source.display()));
    }
    let destination = staging_root.join("artifact").join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create release artifact directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::copy(&source, &destination).map_err(|error| {
        format!(
            "cannot copy release artifact {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    file_identity(
        &destination,
        &format!("artifact/{}", portable_path(relative)),
    )
}

fn artifact_set_identity(files: &[FileIdentity]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update([0]);
        hasher.update(file.size.to_le_bytes());
    }
    hex_digest(hasher.finalize().as_slice())
}

pub(in crate::commands::build) fn collect_file_identities(
    root: &Path,
    excluded_root_file: Option<&str>,
) -> Result<Vec<FileIdentity>, String> {
    let mut paths = Vec::new();
    collect_files(root, root, excluded_root_file, &mut paths)?;
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .expect("collected release file remains below release root");
            file_identity(&path, &portable_path(relative))
        })
        .collect()
}

fn collect_files(
    root: &Path,
    directory: &Path,
    excluded_root_file: Option<&str>,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| {
        format!(
            "cannot inspect release bundle {}: {error}",
            directory.display()
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "cannot inspect release bundle entry in {}: {error}",
                directory.display()
            )
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "release bundle cannot contain symlink {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            collect_files(root, &entry.path(), excluded_root_file, output)?;
        } else if file_type.is_file() {
            let entry_path = entry.path();
            let relative = entry_path.strip_prefix(root).expect("release entry root");
            if relative.components().count() == 1 && relative.to_str() == excluded_root_file {
                continue;
            }
            output.push(entry_path);
        }
    }
    Ok(())
}

fn file_identity(path: &Path, relative: &str) -> Result<FileIdentity, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read release file {}: {error}", path.display()))?;
    Ok(FileIdentity {
        path: relative.to_string(),
        sha256: hex_digest(Sha256::digest(&bytes).as_slice()),
        size: bytes.len() as u64,
    })
}

fn validate_portable_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains('\\') {
        return Err(format!("release artifact path is not portable: `{value}`"));
    }
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "release artifact path must be package-relative: `{value}`"
        ));
    }
    Ok(path)
}

fn required_field(value: &Value, field: &str) -> Result<Value, String> {
    value
        .get(field)
        .cloned()
        .ok_or_else(|| format!("semantic deploy plan is missing `{field}`"))
}

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("cannot serialize release file {}: {error}", path.display()))?;
    fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("cannot write release file {}: {error}", path.display()))
}

fn remove_existing(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot replace {}: {error}", path.display())),
    }
}

fn portable_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
