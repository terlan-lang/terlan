//! Immutable target package artifact verification and cache ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Suffix accepted for version-one Terlan package artifact schemas.
const ARTIFACT_SCHEMA_SUFFIX: &str = ".artifact.v1";

/// Manifest identity embedded in one package artifact.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ArtifactPackageIdentity {
    /// Published package name.
    name: String,
    /// Published package version.
    version: String,
}

/// Runtime locations and environment bindings embedded in an artifact.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ArtifactRuntimeManifest {
    /// Runtime guard relative to the artifact root.
    guard: String,
    /// Native worker relative to the artifact root.
    worker: String,
    /// Private dynamic-library directory relative to the artifact root.
    library_dir: String,
    /// Environment variables required to expose the packaged runtime.
    #[serde(default)]
    environment: BTreeMap<String, String>,
}

/// Generic manifest fields consumed by the Terlan package cache.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct PackageArtifactManifest {
    /// Package-defined versioned artifact schema.
    schema: String,
    /// Platform target triple.
    target: String,
    /// Package identity associated with the artifact.
    package: ArtifactPackageIdentity,
    /// Terlan source package relative to the artifact root.
    terlan_package: String,
    /// Runtime layout and environment contract.
    runtime: ArtifactRuntimeManifest,
}

/// One immutable artifact entry persisted in `terlan.lock`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct LockedPackageArtifact {
    /// Package name.
    pub(super) package: String,
    /// Package version.
    pub(super) version: String,
    /// Platform target triple.
    pub(super) target: String,
    /// Package-defined manifest schema.
    pub(super) schema: String,
    /// SHA-256 archive checksum with its algorithm prefix.
    pub(super) checksum: String,
    /// Content-addressed cache directory name.
    pub(super) cache_key: String,
    /// Relative source package path inside the artifact root.
    pub(super) terlan_package: String,
    /// Runtime environment bindings relative to the artifact root.
    #[serde(default)]
    pub(super) environment: Vec<LockedArtifactEnvironment>,
}

/// One runtime environment binding persisted in the lockfile.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LockedArtifactEnvironment {
    /// Environment variable name.
    pub(super) name: String,
    /// Runtime executable path relative to the artifact root.
    pub(super) path: String,
}

/// Verified cached artifact selected for dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CachedPackageArtifact {
    /// Extracted artifact root.
    pub(super) root: PathBuf,
    /// Terlan package source root.
    pub(super) package_dir: PathBuf,
    /// Absolute runtime environment bindings.
    pub(super) environment: Vec<(String, PathBuf)>,
}

/// Imports one local artifact archive into the immutable project cache.
pub(super) fn import_artifact(
    cache_root: &Path,
    archive_path: &Path,
    expected_target: &str,
) -> Result<LockedPackageArtifact, String> {
    let archive_path = archive_path.canonicalize().map_err(|error| {
        format!(
            "error[package_artifact_missing]: cannot resolve artifact {}: {error}",
            archive_path.display()
        )
    })?;
    let archive_checksum = hash_file(&archive_path)?;
    let temporary = artifact_temporary_dir(cache_root, &archive_checksum);
    remove_dir_if_present(&temporary)?;
    fs::create_dir_all(&temporary).map_err(|error| {
        format!(
            "error[package_artifact_cache_create_failed]: cannot create {}: {error}",
            temporary.display()
        )
    })?;

    let result = import_artifact_into(
        cache_root,
        &archive_path,
        expected_target,
        &archive_checksum,
        &temporary,
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

/// Extracts, validates, and atomically publishes one artifact cache entry.
fn import_artifact_into(
    cache_root: &Path,
    archive_path: &Path,
    expected_target: &str,
    archive_checksum: &str,
    temporary: &Path,
) -> Result<LockedPackageArtifact, String> {
    let extracted = temporary.join("extracted");
    extract_archive(archive_path, &extracted)?;
    let artifact_root = single_artifact_root(&extracted)?;
    let manifest = read_artifact_manifest(&artifact_root)?;
    validate_artifact_manifest(&manifest, expected_target, &artifact_root)?;
    verify_payload_checksums(&artifact_root)?;

    let cache_key = archive_checksum.to_string();
    let destination = artifact_cache_dir(
        cache_root,
        &manifest.package.name,
        &manifest.package.version,
        &manifest.target,
        &cache_key,
    );
    if destination.exists() {
        let entry = locked_entry(&manifest, archive_checksum, cache_key);
        validate_cached_artifact(cache_root, &entry)?;
        remove_dir_if_present(temporary)?;
        return Ok(entry);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "error[package_artifact_cache_create_failed]: cannot create {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::copy(archive_path, temporary.join("archive.tar.zst")).map_err(|error| {
        format!(
            "error[package_artifact_cache_write_failed]: cannot copy {}: {error}",
            archive_path.display()
        )
    })?;
    fs::rename(temporary, &destination).map_err(|error| {
        format!(
            "error[package_artifact_cache_publish_failed]: cannot publish {}: {error}",
            destination.display()
        )
    })?;
    let entry = locked_entry(&manifest, archive_checksum, cache_key);
    validate_cached_artifact(cache_root, &entry)?;
    Ok(entry)
}

/// Builds the lockfile representation of a validated artifact manifest.
fn locked_entry(
    manifest: &PackageArtifactManifest,
    archive_checksum: &str,
    cache_key: String,
) -> LockedPackageArtifact {
    LockedPackageArtifact {
        package: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        target: manifest.target.clone(),
        schema: manifest.schema.clone(),
        checksum: format!("sha256:{archive_checksum}"),
        cache_key,
        terlan_package: manifest.terlan_package.clone(),
        environment: manifest
            .runtime
            .environment
            .iter()
            .map(|(name, path)| LockedArtifactEnvironment {
                name: name.clone(),
                path: path.clone(),
            })
            .collect(),
    }
}

/// Resolves and revalidates one artifact lock entry from the cache.
pub(super) fn validate_cached_artifact(
    cache_root: &Path,
    entry: &LockedPackageArtifact,
) -> Result<CachedPackageArtifact, String> {
    validate_lock_entry(entry)?;
    let cache_dir = artifact_cache_dir(
        cache_root,
        &entry.package,
        &entry.version,
        &entry.target,
        &entry.cache_key,
    );
    let archive = cache_dir.join("archive.tar.zst");
    let expected_checksum = entry
        .checksum
        .strip_prefix("sha256:")
        .expect("lock entry validated above");
    let actual_checksum = hash_file(&archive)?;
    if actual_checksum != expected_checksum {
        return Err(format!(
            "error[package_artifact_checksum_mismatch]: cached artifact `{}` expected `{}`, found `sha256:{actual_checksum}`",
            entry.package, entry.checksum
        ));
    }
    let root = single_artifact_root(&cache_dir.join("extracted"))?;
    let manifest = read_artifact_manifest(&root)?;
    validate_artifact_manifest(&manifest, &entry.target, &root)?;
    if manifest.package.name != entry.package
        || manifest.package.version != entry.version
        || manifest.target != entry.target
        || manifest.schema != entry.schema
        || manifest.terlan_package != entry.terlan_package
    {
        return Err(format!(
            "error[package_artifact_identity_mismatch]: cached artifact `{}` does not match terlan.lock",
            entry.package
        ));
    }
    let manifest_environment = manifest
        .runtime
        .environment
        .iter()
        .map(|(name, path)| LockedArtifactEnvironment {
            name: name.clone(),
            path: path.clone(),
        })
        .collect::<Vec<_>>();
    let mut locked_environment = entry.environment.clone();
    locked_environment.sort();
    if manifest_environment != locked_environment {
        return Err(format!(
            "error[package_artifact_identity_mismatch]: cached artifact `{}` runtime bindings do not match terlan.lock",
            entry.package
        ));
    }
    verify_payload_checksums(&root)?;
    let package_dir = root.join(&entry.terlan_package);
    let environment = entry
        .environment
        .iter()
        .map(|binding| (binding.name.clone(), root.join(&binding.path)))
        .collect();
    Ok(CachedPackageArtifact {
        root,
        package_dir,
        environment,
    })
}

/// Returns the active host artifact target, allowing an explicit CLI override.
pub(super) fn active_artifact_target(explicit: Option<&str>) -> Result<String, String> {
    if let Some(target) = explicit {
        validate_target(target)?;
        return Ok(target.to_string());
    }
    let target = match (env::consts::ARCH, env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        (arch, os) => {
            return Err(format!(
                "error[package_artifact_target_unknown]: no default package artifact target for {arch}-{os}; pass --target <triple>"
            ));
        }
    };
    Ok(target.to_string())
}

/// Validates one persisted artifact lock entry before filesystem use.
pub(super) fn validate_lock_entry(entry: &LockedPackageArtifact) -> Result<(), String> {
    validate_target(&entry.target)?;
    if !entry.schema.starts_with("terlan.") || !entry.schema.ends_with(ARTIFACT_SCHEMA_SUFFIX) {
        return Err(format!(
            "error[package_artifact_schema_unsupported]: unsupported artifact schema `{}`",
            entry.schema
        ));
    }
    let Some(checksum) = entry.checksum.strip_prefix("sha256:") else {
        return Err(
            "error[package_artifact_lock_invalid]: artifact checksum must use sha256".into(),
        );
    };
    if !is_sha256(checksum) || entry.cache_key != checksum {
        return Err(
            "error[package_artifact_lock_invalid]: artifact cache key does not match checksum"
                .into(),
        );
    }
    validate_relative_path(Path::new(&entry.terlan_package))?;
    for binding in &entry.environment {
        validate_environment_name(&binding.name)?;
        validate_relative_path(Path::new(&binding.path))?;
    }
    Ok(())
}

/// Extracts one zstd-compressed tar archive after validating every entry path.
fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "error[package_artifact_extract_failed]: cannot create {}: {error}",
            destination.display()
        )
    })?;
    let file = fs::File::open(archive_path).map_err(|error| {
        format!(
            "error[package_artifact_read_failed]: cannot open {}: {error}",
            archive_path.display()
        )
    })?;
    let decoder = zstd::stream::read::Decoder::new(file).map_err(|error| {
        format!("error[package_artifact_decode_failed]: invalid zstd archive: {error}")
    })?;
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries().map_err(|error| {
        format!("error[package_artifact_decode_failed]: invalid tar archive: {error}")
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|error| {
            format!("error[package_artifact_decode_failed]: invalid tar entry: {error}")
        })?;
        let path = entry
            .path()
            .map_err(|error| {
                format!("error[package_artifact_path_invalid]: invalid archive path: {error}")
            })?
            .into_owned();
        validate_relative_path(&path)?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir() || entry_type.is_symlink()) {
            return Err(format!(
                "error[package_artifact_entry_unsupported]: unsupported archive entry `{}`",
                path.display()
            ));
        }
        if entry_type.is_symlink() {
            let target = entry.link_name().map_err(|error| {
                format!("error[package_artifact_link_invalid]: invalid link target: {error}")
            })?;
            let Some(target) = target else {
                return Err("error[package_artifact_link_invalid]: missing link target".into());
            };
            validate_relative_path(&target)?;
        }
        if !entry.unpack_in(destination).map_err(|error| {
            format!("error[package_artifact_extract_failed]: cannot extract entry: {error}")
        })? {
            return Err(format!(
                "error[package_artifact_path_invalid]: archive entry escapes cache: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Finds the sole top-level artifact directory after extraction.
fn single_artifact_root(extracted: &Path) -> Result<PathBuf, String> {
    let mut roots = fs::read_dir(extracted)
        .map_err(|error| {
            format!(
                "error[package_artifact_cache_missing]: cannot read {}: {error}",
                extracted.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("error[package_artifact_cache_missing]: {error}"))?;
    if roots.len() != 1 {
        return Err(
            "error[package_artifact_layout_invalid]: artifact must contain one root directory"
                .into(),
        );
    }
    let root = roots.remove(0).path();
    let metadata = fs::symlink_metadata(&root)
        .map_err(|error| format!("error[package_artifact_layout_invalid]: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(
            "error[package_artifact_layout_invalid]: artifact root must be a directory".into(),
        );
    }
    Ok(root)
}

/// Reads the generic artifact manifest from an extracted root.
fn read_artifact_manifest(root: &Path) -> Result<PackageArtifactManifest, String> {
    let path = root.join("artifact.json");
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "error[package_artifact_manifest_missing]: cannot read {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "error[package_artifact_manifest_invalid]: cannot parse {}: {error}",
            path.display()
        )
    })
}

/// Validates manifest identity, target, package, and runtime paths.
fn validate_artifact_manifest(
    manifest: &PackageArtifactManifest,
    expected_target: &str,
    root: &Path,
) -> Result<(), String> {
    if manifest.target != expected_target {
        return Err(format!(
            "error[package_artifact_target_mismatch]: artifact target `{}` does not match requested `{expected_target}`",
            manifest.target
        ));
    }
    if manifest.package.name.trim().is_empty() || manifest.package.version.trim().is_empty() {
        return Err(
            "error[package_artifact_identity_invalid]: package name and version are required"
                .into(),
        );
    }
    if !manifest.schema.starts_with("terlan.") || !manifest.schema.ends_with(ARTIFACT_SCHEMA_SUFFIX)
    {
        return Err(format!(
            "error[package_artifact_schema_unsupported]: unsupported artifact schema `{}`",
            manifest.schema
        ));
    }
    for path in [
        manifest.terlan_package.as_str(),
        manifest.runtime.guard.as_str(),
        manifest.runtime.worker.as_str(),
        manifest.runtime.library_dir.as_str(),
    ] {
        validate_relative_path(Path::new(path))?;
        if !root.join(path).exists() {
            return Err(format!(
                "error[package_artifact_payload_missing]: manifest path `{path}` is absent"
            ));
        }
    }
    if manifest.runtime.environment.is_empty() {
        return Err(
            "error[package_artifact_runtime_invalid]: runtime environment cannot be empty".into(),
        );
    }
    for (name, path) in &manifest.runtime.environment {
        validate_environment_name(name)?;
        validate_relative_path(Path::new(path))?;
        if !root.join(path).is_file() {
            return Err(format!(
                "error[package_artifact_payload_missing]: runtime binding `{name}` points to missing `{path}`"
            ));
        }
    }
    Ok(())
}

/// Verifies the complete internal checksum inventory of an extracted artifact.
fn verify_payload_checksums(root: &Path) -> Result<(), String> {
    let checksum_path = root.join("checksums.sha256");
    let text = fs::read_to_string(&checksum_path).map_err(|error| {
        format!(
            "error[package_artifact_checksums_missing]: cannot read {}: {error}",
            checksum_path.display()
        )
    })?;
    let mut expected = BTreeMap::new();
    for (line_index, line) in text.lines().enumerate() {
        let Some((digest, relative)) = line.split_once("  ") else {
            return Err(format!(
                "error[package_artifact_checksums_invalid]: malformed checksum line {}",
                line_index + 1
            ));
        };
        if !is_sha256(digest) {
            return Err(format!(
                "error[package_artifact_checksums_invalid]: invalid checksum line {}",
                line_index + 1
            ));
        }
        let relative = PathBuf::from(relative);
        validate_relative_path(&relative)?;
        if expected.insert(relative, digest.to_string()).is_some() {
            return Err(
                "error[package_artifact_checksums_invalid]: duplicate checksum path".into(),
            );
        }
    }
    let actual_paths = collect_payload_paths(root)?;
    let expected_paths = expected.keys().cloned().collect::<BTreeSet<_>>();
    if actual_paths != expected_paths {
        return Err(
            "error[package_artifact_checksums_mismatch]: checksum inventory does not match payload"
                .into(),
        );
    }
    for (relative, digest) in expected {
        let actual = hash_path(&root.join(&relative))?;
        if actual != digest {
            return Err(format!(
                "error[package_artifact_checksums_mismatch]: `{}` expected `{digest}`, found `{actual}`",
                relative.display()
            ));
        }
    }
    Ok(())
}

/// Collects every payload file and symbolic link except the checksum inventory.
fn collect_payload_paths(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?;
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() || metadata.file_type().is_symlink() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?
                    .to_path_buf();
                if relative != Path::new("checksums.sha256") {
                    paths.insert(relative);
                }
            } else {
                return Err(format!(
                    "error[package_artifact_entry_unsupported]: unsupported cached entry {}",
                    path.display()
                ));
            }
        }
    }
    Ok(paths)
}

/// Hashes one regular file or symbolic-link target with SHA-256.
fn hash_path(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?;
    let mut hasher = Sha256::new();
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)
            .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?;
        hasher.update(target.as_os_str().as_encoded_bytes());
    } else if metadata.is_file() {
        let mut file = fs::File::open(path)
            .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("error[package_artifact_cache_read_failed]: {error}"))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
    } else {
        return Err(format!(
            "error[package_artifact_entry_unsupported]: cannot hash {}",
            path.display()
        ));
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Hashes one archive file with SHA-256.
fn hash_file(path: &Path) -> Result<String, String> {
    hash_path(path).map_err(|error| {
        format!(
            "error[package_artifact_checksum_failed]: cannot hash {}: {error}",
            path.display()
        )
    })
}

/// Returns the content-addressed cache directory for one artifact.
fn artifact_cache_dir(
    cache_root: &Path,
    package: &str,
    version: &str,
    target: &str,
    cache_key: &str,
) -> PathBuf {
    cache_root
        .join("artifacts")
        .join(package)
        .join(version)
        .join(target)
        .join(cache_key)
}

/// Returns a collision-resistant unpublished cache directory.
fn artifact_temporary_dir(cache_root: &Path, checksum: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    cache_root
        .join("artifacts")
        .join(format!(".{checksum}.tmp-{}-{unique}", std::process::id()))
}

/// Rejects absolute, empty, parent-traversing, and prefixed artifact paths.
fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "error[package_artifact_path_invalid]: unsafe artifact path `{}`",
            path.display()
        ));
    }
    Ok(())
}

/// Validates an environment variable name admitted to runtime metadata.
fn validate_environment_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphanumeric() && (index > 0 || !byte.is_ascii_digit())
        })
    {
        return Err(format!(
            "error[package_artifact_runtime_invalid]: invalid environment name `{name}`"
        ));
    }
    Ok(())
}

/// Validates a non-empty target triple without accepting path syntax.
fn validate_target(target: &str) -> Result<(), String> {
    if target.is_empty()
        || target.starts_with('-')
        || !target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!(
            "error[package_artifact_target_invalid]: invalid target `{target}`"
        ));
    }
    Ok(())
}

/// Returns whether text is one lowercase or uppercase SHA-256 digest.
fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Removes an unpublished temporary directory when it exists.
fn remove_dir_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "error[package_artifact_cache_cleanup_failed]: cannot remove {}: {error}",
            path.display()
        )),
    }
}

#[cfg(test)]
#[path = "package_artifact_test.rs"]
pub(super) mod tests;
