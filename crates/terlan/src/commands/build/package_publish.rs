//! Deterministic package sealing used by publish dry-run and live publication.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::LazyLock;

use regex::RegexSet;
use serde_json::to_vec_pretty;
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::package_registry::admission::{valid_public_https_url, validate_spdx_expression};
use crate::package_registry::model::{
    ArchiveIdentity, ArchiveLimits, ArtifactKind, ArtifactRecord, DependencyRecord,
    DependencySource, Digest, PackageIdentity, PackageLink, PackageVersionRecord, PublishRequest,
    SourceIdentity, SourceIdentityKind, SourceIdentityVerification, SymlinkPolicy,
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_ARCHIVE_PATH_BYTES, MAX_UNPACKED_BYTES,
    SHA256_ALGORITHM,
};

use super::project_manifest::{
    read_project_manifest, ProjectDependencyScope, ProjectDependencySource, ProjectManifest,
    ProjectTarget,
};
use super::TERLAN_PROJECT_MANIFEST_FILE;

pub(super) fn run(args: &[String], output_root: &Path) -> ExitCode {
    let command = match parse_args(args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let (project_root, mirror) = match command {
        PublishCommand::DryRun { project_root } => (project_root, None),
        PublishCommand::Mirror {
            project_root,
            mirror,
        } => (project_root, Some(mirror)),
        PublishCommand::Live {
            project_root,
            registry,
            publisher_key_id,
            signing_seed_file,
        } => {
            return match seal_publish_dry_run(&project_root, output_root).and_then(|summary| {
                Ok(super::package_publish_live::publish(
                    summary,
                    &registry,
                    &publisher_key_id,
                    &signing_seed_file,
                )?)
            }) {
                Ok(summary) => {
                    println!(
                        "published {}@{} to {} as {} at snapshot sequence {}",
                        summary.package,
                        summary.version,
                        registry,
                        summary.publish_id,
                        summary.snapshot_sequence
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            };
        }
    };
    match seal_publish_dry_run(&project_root, output_root) {
        Ok(summary) => {
            println!(
                "sealed {} files as sha256:{} at {}",
                summary.file_count,
                summary.archive_sha256,
                summary.archive.display()
            );
            println!("publish dry-run metadata: {}", summary.request.display());
            if let Some(mirror) = mirror {
                match super::package_registry_mirror::publish_to_mirror(&summary, &mirror) {
                    Ok(published) => println!(
                        "published {}@{} to {} at sequence {} (snapshot sha256:{})",
                        published.package,
                        published.version,
                        mirror.display(),
                        published.sequence,
                        published.snapshot_sha256
                    ),
                    Err(message) => {
                        eprintln!("{message}");
                        return ExitCode::from(1);
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PackageSealSummary {
    pub(super) archive: PathBuf,
    pub(super) documentation: Option<PathBuf>,
    pub(super) request: PathBuf,
    pub(super) archive_sha256: String,
    pub(super) file_count: u32,
}

pub(super) fn seal_publish_dry_run(
    project_root: &Path,
    output_root: &Path,
) -> Result<PackageSealSummary, String> {
    let manifest_path = project_root.join(TERLAN_PROJECT_MANIFEST_FILE);
    let manifest = read_project_manifest(&manifest_path)?;
    let files = package_files(project_root, &manifest)?;
    scan_package_secrets(project_root, &files)?;
    let package_directory = output_root.join("package");
    fs::create_dir_all(&package_directory)
        .map_err(|error| format!("cannot create {}: {error}", package_directory.display()))?;
    let stem = format!("{}-{}", manifest.package.name, manifest.package.version);
    let archive_path = package_directory.join(format!("{stem}.tar.zst"));
    let documentation_path = package_directory.join(format!("{stem}.docs.tar.zst"));
    let request_path = package_directory.join(format!("{stem}.publish-request.json"));
    if archive_path.exists() || documentation_path.exists() || request_path.exists() {
        return Err(format!(
            "publish dry-run output already exists; choose a clean --out-dir: {}",
            package_directory.display()
        ));
    }

    let archive_summary =
        terlan_archive::create_tar_zstd_files(project_root, &files, &archive_path)
            .map_err(|error| error.to_string())?;
    let compressed_bytes = fs::metadata(&archive_path)
        .map_err(|error| format!("cannot inspect {}: {error}", archive_path.display()))?
        .len();
    if compressed_bytes > MAX_ARCHIVE_BYTES {
        let _ = fs::remove_file(&archive_path);
        return Err(format!(
            "sealed archive exceeds {MAX_ARCHIVE_BYTES} compressed bytes"
        ));
    }
    let archive_sha256 = hash_file(&archive_path)?;
    let artifacts = artifact_records(project_root, &files, &manifest)?;
    let documentation_files = artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Documentation)
        .map(|artifact| PathBuf::from(&artifact.path))
        .collect::<Vec<_>>();
    let documentation = if documentation_files.is_empty() {
        None
    } else {
        let summary = terlan_archive::create_tar_zstd_files(
            project_root,
            &documentation_files,
            &documentation_path,
        )
        .map_err(|error| error.to_string())?;
        let compressed_bytes = fs::metadata(&documentation_path)
            .map_err(|error| format!("cannot inspect {}: {error}", documentation_path.display()))?
            .len();
        Some(ArchiveIdentity {
            format: "tar.zst".into(),
            digest: Digest {
                algorithm: SHA256_ALGORITHM.into(),
                value: hash_file(&documentation_path)?,
            },
            compressed_bytes,
            unpacked_bytes: summary.unpacked_bytes,
            file_count: summary.file_count,
        })
    };
    let provenance = digest_artifacts("terlan-package-provenance-v1", &artifacts);
    let public_api = digest_selected("terlan-package-public-api-v1", &artifacts, |artifact| {
        artifact.path.ends_with(".terli") || artifact.kind == ArtifactKind::PublicApi
    });
    let capabilities = package_capabilities(&manifest);
    let repository_url = public_repository_url(&manifest).map_err(|error| error.to_string())?;
    let (description, license, links) = publication_metadata(&manifest)?;
    let package = PackageIdentity {
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
    };
    let request = PublishRequest {
        schema: "terlan-registry-publish-request-v1".into(),
        package_version: PackageVersionRecord {
            schema: "terlan-registry-package-version-v1".into(),
            package,
            repository_url,
            description,
            license,
            links,
            archive: ArchiveIdentity {
                format: "tar.zst".into(),
                digest: Digest {
                    algorithm: SHA256_ALGORITHM.into(),
                    value: archive_sha256.clone(),
                },
                compressed_bytes,
                unpacked_bytes: archive_summary.unpacked_bytes,
                file_count: archive_summary.file_count,
            },
            dependencies: dependency_records(&manifest)?,
            artifacts,
            targets: vec![manifest.artifact.as_str().into()],
            capabilities,
            built_with: format!("terlan-{}", env!("CARGO_PKG_VERSION")),
            requires_terlan: manifest
                .package
                .compiler
                .clone()
                .unwrap_or_else(|| format!(">={}, <0.1.0", env!("CARGO_PKG_VERSION"))),
            source_identity: SourceIdentity {
                kind: SourceIdentityKind::ArtifactSet,
                value: provenance.value.clone(),
                verification: SourceIdentityVerification::RegistryDerived,
            },
            provenance: provenance.clone(),
            public_api,
            documentation,
        },
        publisher_key_id: "dry-run".into(),
        request_id: format!("dry-run-{}", &provenance.value[..16]),
        archive_upload: archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "archive output name is not UTF-8".to_string())?
            .into(),
        documentation_upload: documentation_path
            .is_file()
            .then(|| {
                documentation_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                    .ok_or_else(|| "documentation output name is not UTF-8".to_string())
            })
            .transpose()?,
        limits: ArchiveLimits {
            max_archive_bytes: MAX_ARCHIVE_BYTES,
            max_unpacked_bytes: MAX_UNPACKED_BYTES,
            max_files: MAX_ARCHIVE_FILES,
            max_path_bytes: MAX_ARCHIVE_PATH_BYTES,
            symlinks: SymlinkPolicy::Reject,
        },
    };
    let mut request_bytes = to_vec_pretty(&request).map_err(|error| error.to_string())?;
    request_bytes.push(b'\n');
    if let Err(error) = fs::write(&request_path, request_bytes) {
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_file(&documentation_path);
        return Err(format!("cannot write {}: {error}", request_path.display()));
    }
    Ok(PackageSealSummary {
        archive: archive_path,
        documentation: documentation_path.is_file().then_some(documentation_path),
        request: request_path,
        archive_sha256,
        file_count: archive_summary.file_count,
    })
}

const SECRET_FINDING_CLASSES: [&str; 6] = [
    "private-key",
    "aws-access-key",
    "github-token",
    "slack-token",
    "jwt",
    "assigned-credential",
];

static SECRET_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----",
        r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
        r"\bgh[pousr]_[A-Za-z0-9]{36,255}\b",
        r"\bxox[baprs]-[A-Za-z0-9-]{10,255}\b",
        r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b",
        r#"(?i)\b(?:api[_-]?key|secret[_-]?(?:key|token)?|access[_-]?token|password)\s*[:=]\s*["']?[A-Za-z0-9_./+=-]{20,}"#,
    ])
    .expect("Registry secret patterns must compile")
});

fn scan_package_secrets(root: &Path, files: &[PathBuf]) -> Result<(), String> {
    for relative in files {
        let bytes = fs::read(root.join(relative)).map_err(|error| {
            format!("cannot scan package input {}: {error}", relative.display())
        })?;
        let text = String::from_utf8_lossy(&bytes);
        let matches = SECRET_PATTERNS.matches(&text);
        if let Some(index) = matches.iter().next() {
            let pattern = &SECRET_PATTERNS.patterns()[index];
            let line = regex::Regex::new(pattern)
                .expect("Registry secret pattern must compile independently")
                .find(&text)
                .map(|finding| {
                    text[..finding.start()]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count()
                        + 1
                })
                .unwrap_or(1);
            return Err(format!(
                "error[registry_secret_scan]: possible {} credential in {}:{line}; remove it before publication",
                SECRET_FINDING_CLASSES[index],
                relative.display()
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct RepositoryUrlError(String);

impl std::fmt::Display for RepositoryUrlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RepositoryUrlError {}

fn public_repository_url(manifest: &ProjectManifest) -> Result<String, RepositoryUrlError> {
    let value = manifest.package.repository.as_deref().ok_or_else(|| {
        RepositoryUrlError(
            "error[registry_repository]: [package].repository is required for publication"
                .to_string(),
        )
    })?;
    if !valid_public_https_url(value) {
        return Err(RepositoryUrlError(
            "error[registry_repository]: [package].repository must be a valid public HTTPS URL"
                .into(),
        ));
    }
    Ok(value.into())
}

fn publication_metadata(
    manifest: &ProjectManifest,
) -> Result<(String, String, Vec<PackageLink>), String> {
    let description = manifest.package.description.as_deref().ok_or_else(|| {
        "error[registry_description]: [package].description is required for publication".to_string()
    })?;
    if description.trim() != description || description.len() > 500 {
        return Err(
            "error[registry_description]: [package].description must be trimmed and at most 500 bytes"
                .into(),
        );
    }
    let license = manifest.package.license.as_deref().ok_or_else(|| {
        "error[registry_license]: [package].license is required for publication".to_string()
    })?;
    validate_spdx_expression(license).map_err(|error| error.to_string())?;
    let mut names = BTreeSet::new();
    let links = manifest
        .package
        .links
        .iter()
        .map(|value| {
            let parsed = Url::parse(value).map_err(|_| {
                format!("error[registry_link]: package link `{value}` is not a valid HTTPS URL")
            })?;
            let name = parsed.host_str().ok_or_else(|| {
                format!("error[registry_link]: package link `{value}` has no public host")
            })?;
            if !valid_public_https_url(value) || !names.insert(name.to_string())
            {
                return Err(format!(
                    "error[registry_link]: package links require unique named HTTPS hosts; rejected `{value}`"
                ));
            }
            Ok(PackageLink {
                name: name.into(),
                url: value.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if links.is_empty() {
        return Err("error[registry_link]: [package].links requires at least one HTTPS URL".into());
    }
    Ok((description.into(), license.into(), links))
}

enum PublishCommand {
    DryRun {
        project_root: PathBuf,
    },
    Mirror {
        project_root: PathBuf,
        mirror: PathBuf,
    },
    Live {
        project_root: PathBuf,
        registry: String,
        publisher_key_id: String,
        signing_seed_file: PathBuf,
    },
}

fn parse_args(args: &[String]) -> Result<PublishCommand, String> {
    match args {
        [command, flag] if command == "publish" && flag == "--dry-run" => Ok(PublishCommand::DryRun {
            project_root: PathBuf::from("."),
        }),
        [command, flag, project] if command == "publish" && flag == "--dry-run" => {
            Ok(PublishCommand::DryRun { project_root: PathBuf::from(project) })
        }
        [command, mirror_flag, mirror] if command == "publish" && mirror_flag == "--mirror" => {
            Ok(PublishCommand::Mirror { project_root: PathBuf::from("."), mirror: PathBuf::from(mirror) })
        }
        [command, mirror_flag, mirror, project] if command == "publish" && mirror_flag == "--mirror" => {
            Ok(PublishCommand::Mirror { project_root: PathBuf::from(project), mirror: PathBuf::from(mirror) })
        }
        [command, registry_flag, registry, publisher_flag, publisher_key_id, seed_flag, signing_seed_file]
            if command == "publish"
                && registry_flag == "--registry"
                && publisher_flag == "--publisher-key-id"
                && seed_flag == "--signing-seed-file" =>
        {
            Ok(PublishCommand::Live {
                project_root: PathBuf::from("."),
                registry: registry.clone(),
                publisher_key_id: publisher_key_id.clone(),
                signing_seed_file: PathBuf::from(signing_seed_file),
            })
        }
        [command, registry_flag, registry, publisher_flag, publisher_key_id, seed_flag, signing_seed_file, project]
            if command == "publish"
                && registry_flag == "--registry"
                && publisher_flag == "--publisher-key-id"
                && seed_flag == "--signing-seed-file" =>
        {
            Ok(PublishCommand::Live {
                project_root: PathBuf::from(project),
                registry: registry.clone(),
                publisher_key_id: publisher_key_id.clone(),
                signing_seed_file: PathBuf::from(signing_seed_file),
            })
        }
        _ => Err("usage: terlc package publish (--dry-run | --mirror <dir> | --registry <url> --publisher-key-id <id> --signing-seed-file <path>) [project-dir] --out-dir <dir>".into()),
    }
}

fn package_files(project_root: &Path, manifest: &ProjectManifest) -> Result<Vec<PathBuf>, String> {
    let mut roots = BTreeSet::new();
    roots.insert(PathBuf::from(TERLAN_PROJECT_MANIFEST_FILE));
    for source_root in &manifest.source_roots {
        roots.insert(PathBuf::from(source_root));
    }
    for script in &manifest.scripts {
        roots.insert(PathBuf::from(&script.path));
    }
    if let Some(web_assets) = &manifest.web_assets {
        roots.insert(PathBuf::from(&web_assets.directory));
    }
    if let Some(native) = &manifest.native_rust {
        roots.insert(PathBuf::from(&native.path));
    }
    if let Some(deployment) = &manifest.deployment {
        for migration in &deployment.migrations {
            roots.insert(PathBuf::from(migration));
        }
    }
    for optional in ["README.md", "LICENSE", "LICENSE.md", "docs"] {
        if project_root.join(optional).exists() {
            roots.insert(PathBuf::from(optional));
        }
    }

    let mut files = BTreeSet::new();
    for relative in roots {
        validate_relative_path(&relative)?;
        collect_package_path(project_root, &relative, &mut files)?;
    }
    if files.len() > MAX_ARCHIVE_FILES as usize {
        return Err(format!("package exceeds {MAX_ARCHIVE_FILES} files"));
    }
    Ok(files.into_iter().collect())
}

fn collect_package_path(
    project_root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let absolute = project_root.join(relative);
    let metadata = fs::symlink_metadata(&absolute).map_err(|error| {
        format!(
            "cannot inspect package input {}: {error}",
            absolute.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "package input symlink is forbidden: {}",
            relative.display()
        ));
    }
    if metadata.is_file() {
        validate_relative_path(relative)?;
        files.insert(relative.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(format!(
            "package input is not a regular file or directory: {}",
            relative.display()
        ));
    }
    let mut children = fs::read_dir(&absolute)
        .map_err(|error| format!("cannot read package input {}: {error}", absolute.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot read package input {}: {error}", absolute.display()))?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        let name = child.file_name();
        if matches!(
            name.to_str(),
            Some(".git" | ".terlan" | "_build" | "target" | "node_modules")
        ) {
            continue;
        }
        collect_package_path(project_root, &relative.join(name), files)?;
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "package path must be relative and traversal-free: {}",
            path.display()
        ));
    }
    let text = path
        .to_str()
        .ok_or_else(|| format!("package path must be UTF-8: {}", path.display()))?;
    if text.len() > MAX_ARCHIVE_PATH_BYTES as usize || text.contains('\\') {
        return Err(format!(
            "package path is non-portable or exceeds {MAX_ARCHIVE_PATH_BYTES} bytes: {text}"
        ));
    }
    Ok(())
}

fn artifact_records(
    root: &Path,
    files: &[PathBuf],
    manifest: &ProjectManifest,
) -> Result<Vec<ArtifactRecord>, String> {
    files
        .iter()
        .map(|relative| {
            let path = relative.to_string_lossy().replace('\\', "/");
            let metadata = fs::metadata(root.join(relative))
                .map_err(|error| format!("cannot inspect {path}: {error}"))?;
            Ok(ArtifactRecord {
                schema: "terlan-registry-artifact-v1".into(),
                kind: artifact_kind(&path, manifest),
                path,
                digest: Digest {
                    algorithm: SHA256_ALGORITHM.into(),
                    value: hash_file(&root.join(relative))?,
                },
                bytes: metadata.len(),
                target: None,
                executable: false,
            })
        })
        .collect()
}

fn artifact_kind(path: &str, manifest: &ProjectManifest) -> ArtifactKind {
    if path.ends_with(".terli") {
        ArtifactKind::PublicApi
    } else if path.ends_with(".md") || path.starts_with("docs/") {
        ArtifactKind::Documentation
    } else if path.contains("generated") || path.ends_with(".d.ts") {
        ArtifactKind::GeneratedBinding
    } else if manifest.native_rust.as_ref().is_some_and(|native| {
        path == native.path || path.starts_with(&format!("{}/", native.path.trim_end_matches('/')))
    }) {
        ArtifactKind::Native
    } else {
        ArtifactKind::Source
    }
}

fn dependency_records(manifest: &ProjectManifest) -> Result<Vec<DependencyRecord>, String> {
    manifest
        .dependencies
        .iter()
        .map(|dependency| {
            let (source, registry, requirement, source_identity, integrity, options) =
                match &dependency.source {
                    ProjectDependencySource::Path { path } => (
                        DependencySource::Path,
                        "local".into(),
                        path.clone(),
                        None,
                        None,
                        Vec::new(),
                    ),
                    ProjectDependencySource::Git { url, rev } => (
                        DependencySource::Git,
                        url.clone(),
                        rev.clone(),
                        Some(rev.clone()),
                        None,
                        Vec::new(),
                    ),
                    ProjectDependencySource::Registry { registry, version } => (
                        DependencySource::TerlanRegistry,
                        registry.clone(),
                        version.clone(),
                        None,
                        None,
                        Vec::new(),
                    ),
                    ProjectDependencySource::Npm {
                        package,
                        version,
                        integrity,
                    } => (
                        DependencySource::Npm,
                        "npm".into(),
                        format!("{package}@{version}"),
                        Some(format!("{package}@{version}")),
                        parse_external_integrity(integrity.as_deref())?,
                        Vec::new(),
                    ),
                    ProjectDependencySource::Cargo {
                        package,
                        version,
                        integrity,
                        features,
                    } => (
                        DependencySource::Cargo,
                        "cargo".into(),
                        format!("{package}@{version}"),
                        Some(format!("{package}@{version}")),
                        parse_external_integrity(integrity.as_deref())?,
                        features.clone(),
                    ),
                };
            let target = match dependency.scope {
                ProjectDependencyScope::Local => None,
                ProjectDependencyScope::Target(ProjectTarget::Js) => Some("js".into()),
                ProjectDependencyScope::Target(ProjectTarget::Rust) => Some("rust".into()),
            };
            Ok(DependencyRecord {
                schema: "terlan-registry-dependency-v1".into(),
                name: dependency.alias.clone(),
                source,
                requirement,
                registry,
                optional: false,
                target,
                capabilities: Vec::new(),
                source_identity,
                integrity,
                options,
            })
        })
        .collect()
}

fn parse_external_integrity(value: Option<&str>) -> Result<Option<Digest>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let digest = value.strip_prefix("sha256:").ok_or_else(|| {
        "external dependency integrity must use sha256:<lowercase-hex>".to_string()
    })?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("external dependency integrity must use sha256:<lowercase-hex>".into());
    }
    Ok(Some(Digest {
        algorithm: SHA256_ALGORITHM.into(),
        value: digest.into(),
    }))
}

fn package_capabilities(manifest: &ProjectManifest) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    capabilities.insert(format!("artifact.{}", manifest.artifact.as_str()));
    if manifest.web_assets.is_some() {
        capabilities.insert("web.assets".into());
    }
    if manifest.native_rust.is_some() {
        capabilities.insert("native.rust".into());
    }
    for dependency in &manifest.dependencies {
        match dependency.scope {
            ProjectDependencyScope::Local => {
                capabilities.insert("dependency.local".into());
            }
            ProjectDependencyScope::Target(ProjectTarget::Js) => {
                capabilities.insert("dependency.target.js".into());
            }
            ProjectDependencyScope::Target(ProjectTarget::Rust) => {
                capabilities.insert("dependency.target.rust".into());
            }
        }
    }
    capabilities.into_iter().collect()
}

fn digest_artifacts(domain: &str, artifacts: &[ArtifactRecord]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for artifact in artifacts {
        hasher.update([0]);
        hasher.update(artifact.path.as_bytes());
        hasher.update([0]);
        hasher.update(artifact.digest.value.as_bytes());
    }
    Digest {
        algorithm: SHA256_ALGORITHM.into(),
        value: hex(hasher.finalize().as_slice()),
    }
}

fn digest_selected<F>(domain: &str, artifacts: &[ArtifactRecord], include: F) -> Digest
where
    F: Fn(&ArtifactRecord) -> bool,
{
    let selected = artifacts
        .iter()
        .filter(|artifact| include(artifact))
        .cloned()
        .collect::<Vec<_>>();
    digest_artifacts(domain, &selected)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(hex(Sha256::digest(bytes).as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "package_publish_test.rs"]
mod tests;
