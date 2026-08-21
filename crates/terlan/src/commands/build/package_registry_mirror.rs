//! Local Registry mirror publication used by CLI integration and self-hosting.

use std::fs;
use std::path::Path;

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::package_registry::admission::{
    canonical_package_name, safe_identity_segment, validate_archive_inventory,
    validate_publish_request, validate_registry_dependencies,
};
use crate::package_registry::model::{
    Digest, PackageIndexRecord, PackageIndexVersion, PackageIndexYank, PublishRequest, RootRecord,
    SnapshotPackage, SnapshotRecord, TrustKey, YankReason, YankRecord, YankState,
};
use crate::package_registry::{canonical_version, latest_stable};

use super::package_publish::PackageSealSummary;

#[derive(Debug)]
pub(super) struct MirrorMutationSummary {
    pub(super) package: String,
    pub(super) version: String,
    pub(super) sequence: u64,
    pub(super) snapshot_sha256: String,
}

pub(super) fn publish_to_mirror(
    sealed: &PackageSealSummary,
    mirror: &Path,
) -> Result<MirrorMutationSummary, String> {
    let request_bytes = fs::read(&sealed.request)
        .map_err(|error| format!("cannot read {}: {error}", sealed.request.display()))?;
    let request: PublishRequest = serde_json::from_slice(&request_bytes)
        .map_err(|error| format!("invalid sealed publish request: {error}"))?;
    if request.schema != "terlan-registry-publish-request-v1" {
        return Err("error[registry_publish_schema]: sealed request schema is unsupported".into());
    }
    validate_publish_request(&request).map_err(|error| error.to_string())?;
    validate_registry_dependencies(&request, mirror).map_err(|error| error.to_string())?;
    let package = &request.package_version.package;
    let package_directory = mirror.join("packages").join(&package.name);
    let version_directory = package_directory.join(&package.version);
    let index_path = mirror
        .join("packages")
        .join(format!("{}.json", package.name));
    let existing_index = if index_path.is_file() {
        let index: PackageIndexRecord = read_json(&index_path)?;
        if index.name != package.name
            || index.schema != "terlan-registry-package-index-v1"
            || index
                .versions
                .iter()
                .any(|version| version.version == package.version)
        {
            return Err(format!(
                "error[registry_version_immutable]: `{}@{}` already exists or has a conflicting index",
                package.name, package.version
            ));
        }
        Some(index)
    } else {
        None
    };
    if version_directory.exists() {
        return Err(format!(
            "error[registry_version_immutable]: object directory already exists for `{}@{}`",
            package.name, package.version
        ));
    }

    let sequence = next_sequence(mirror)?;
    let temporary =
        package_directory.join(format!(".{}.tmp-{}", package.version, std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary).map_err(|error| {
            format!(
                "cannot remove stale publication staging {}: {error}",
                temporary.display()
            )
        })?;
    }
    fs::create_dir_all(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    let publish_result = stage_version(&request, sealed, &temporary).and_then(|metadata_sha256| {
        fs::rename(&temporary, &version_directory)
            .map_err(|error| format!("cannot publish immutable version directory: {error}"))?;
        let mut index = existing_index.unwrap_or_else(|| PackageIndexRecord {
            schema: "terlan-registry-package-index-v1".into(),
            name: package.name.clone(),
            repository_url: request.package_version.repository_url.clone(),
            versions: Vec::new(),
            latest_stable: None,
            signed_digest: digest_bytes(b"empty-package-index"),
        });
        index.repository_url = request.package_version.repository_url.clone();
        index.versions.push(PackageIndexVersion {
            version: package.version.clone(),
            archive: request.package_version.archive.digest.clone(),
            metadata: Digest {
                algorithm: "sha256".into(),
                value: metadata_sha256,
            },
            documentation: request
                .package_version
                .documentation
                .as_ref()
                .map(|documentation| documentation.digest.clone()),
            built_with: request.package_version.built_with.clone(),
            requires_terlan: request.package_version.requires_terlan.clone(),
            published_sequence: sequence,
            published_at: "1970-01-01T00:00:00.000000Z".into(),
            yanked: false,
            yank: None,
        });
        index.versions.sort_by(|left, right| {
            canonical_version(&left.version)
                .expect("published versions are canonical")
                .cmp(&canonical_version(&right.version).expect("published versions are canonical"))
        });
        index.latest_stable = latest_stable(
            index
                .versions
                .iter()
                .filter(|candidate| !candidate.yanked)
                .map(|candidate| candidate.version.as_str()),
        );
        index.signed_digest = digest_bytes(index_identity(&index).as_bytes());
        write_json_atomic(&index_path, &index)?;
        write_root_if_missing(mirror)?;
        let snapshot = build_snapshot(mirror, sequence)?;
        write_json_atomic(&mirror.join("snapshot.json"), &snapshot)?;
        let snapshot_sha256 = hash_file(&mirror.join("snapshot.json"))?;
        Ok(MirrorMutationSummary {
            package: package.name.clone(),
            version: package.version.clone(),
            sequence,
            snapshot_sha256,
        })
    });
    if publish_result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    publish_result
}

pub(super) fn yank_in_mirror(
    mirror: &Path,
    package: &str,
    version: &str,
    reason: YankReason,
    message: &str,
    replacement_package: Option<&str>,
) -> Result<MirrorMutationSummary, String> {
    if !canonical_package_name(package)
        || !safe_identity_segment(version)
        || message.trim().is_empty()
        || message.trim() != message
        || message.len() > 500
    {
        return Err(
            "error[registry_yank_input]: package, version, and a trimmed message of at most 500 bytes are required"
                .into(),
        );
    }
    if let Some(replacement) = replacement_package {
        if !canonical_package_name(replacement) || replacement == package {
            return Err(
                "error[registry_yank_replacement]: replacement must name a different valid package"
                    .into(),
            );
        }
        if !mirror
            .join("packages")
            .join(format!("{replacement}.json"))
            .is_file()
        {
            return Err(format!(
                "error[registry_yank_replacement]: replacement package `{replacement}` is absent"
            ));
        }
    }
    let index_path = mirror.join("packages").join(format!("{package}.json"));
    let mut index: PackageIndexRecord = read_json(&index_path)?;
    if index.schema != "terlan-registry-package-index-v1" || index.name != package {
        return Err("error[registry_yank_index]: package index identity is invalid".into());
    }
    let selected = index
        .versions
        .iter_mut()
        .find(|candidate| candidate.version == version)
        .ok_or_else(|| format!("error[registry_yank_missing]: `{package}@{version}` is absent"))?;
    if selected.yanked {
        return Err(format!(
            "error[registry_yank_state]: `{package}@{version}` is already yanked"
        ));
    }
    let sequence = next_sequence(mirror)?;
    selected.yanked = true;
    selected.yank = Some(PackageIndexYank {
        reason,
        message: message.into(),
        replacement_package: replacement_package.map(str::to_string),
    });
    index.latest_stable = latest_stable(
        index
            .versions
            .iter()
            .filter(|candidate| !candidate.yanked)
            .map(|candidate| candidate.version.as_str()),
    );
    index.signed_digest = digest_bytes(index_identity(&index).as_bytes());
    let yank = YankRecord {
        schema: "terlan-registry-yank-v1".into(),
        package: crate::package_registry::model::PackageIdentity {
            name: package.into(),
            version: version.into(),
        },
        state: YankState::Yanked,
        reason,
        message: message.into(),
        replacement_package: replacement_package.map(str::to_string),
        publisher_key_id: "registry-operator-local-v1".into(),
        sequence,
    };

    write_json_atomic(&index_path, &index)?;
    write_json_atomic(
        &mirror
            .join("packages")
            .join(package)
            .join(version)
            .join("yank.json"),
        &yank,
    )?;
    let snapshot = build_snapshot(mirror, sequence)?;
    write_json_atomic(&mirror.join("snapshot.json"), &snapshot)?;
    Ok(MirrorMutationSummary {
        package: package.into(),
        version: version.into(),
        sequence,
        snapshot_sha256: hash_file(&mirror.join("snapshot.json"))?,
    })
}

fn stage_version(
    request: &PublishRequest,
    sealed: &PackageSealSummary,
    temporary: &Path,
) -> Result<String, String> {
    let archive = temporary.join("archive.tar.zst");
    fs::copy(&sealed.archive, &archive)
        .map_err(|error| format!("cannot stage sealed archive: {error}"))?;
    let actual = hash_file(&archive)?;
    if request.package_version.archive.digest.algorithm != "sha256"
        || actual != request.package_version.archive.digest.value
    {
        return Err(
            "error[registry_publish_archive_checksum]: staged archive differs from request".into(),
        );
    }
    validate_archive_inventory(&archive, request, temporary).map_err(|error| error.to_string())?;
    match (
        &request.package_version.documentation,
        &sealed.documentation,
    ) {
        (Some(expected), Some(source)) => {
            let documentation = temporary.join("documentation.tar.zst");
            fs::copy(source, &documentation)
                .map_err(|error| format!("cannot stage sealed documentation: {error}"))?;
            let bytes = fs::metadata(&documentation)
                .map_err(|error| format!("cannot inspect staged documentation: {error}"))?
                .len();
            if expected.digest.algorithm != "sha256"
                || hash_file(&documentation)? != expected.digest.value
                || bytes != expected.compressed_bytes
            {
                return Err(
                    "error[registry_publish_documentation]: staged documentation differs from request"
                        .into(),
                );
            }
        }
        (None, None) => {}
        _ => {
            return Err(
                "error[registry_publish_documentation]: documentation object and upload disagree"
                    .into(),
            );
        }
    }
    let metadata = json_bytes(&request.package_version)?;
    let metadata_sha256 = hex(Sha256::digest(&metadata).as_slice());
    fs::write(temporary.join("metadata.json"), metadata)
        .map_err(|error| format!("cannot stage package metadata: {error}"))?;
    Ok(metadata_sha256)
}

fn next_sequence(mirror: &Path) -> Result<u64, String> {
    let path = mirror.join("snapshot.json");
    if !path.is_file() {
        return Ok(1);
    }
    let snapshot: SnapshotRecord = read_json(&path)?;
    snapshot
        .sequence
        .checked_add(1)
        .ok_or_else(|| "error[registry_snapshot_sequence]: sequence overflow".into())
}

fn write_root_if_missing(mirror: &Path) -> Result<(), String> {
    let path = mirror.join("root.json");
    if path.is_file() {
        let root: RootRecord = read_json(&path)?;
        if root.schema != "terlan-registry-root-v1" || root.version != 1 {
            return Err("error[registry_root_conflict]: mirror root is unsupported".into());
        }
        return Ok(());
    }
    let root = RootRecord {
        schema: "terlan-registry-root-v1".into(),
        version: 1,
        previous_version: None,
        threshold: 1,
        keys: vec![TrustKey {
            key_id: "registry-operator-local-v1".into(),
            algorithm: "ed25519".into(),
            public_key_base64: "local-trust-adapter".into(),
            roles: vec!["root".into(), "snapshot".into()],
        }],
        signed_digest: digest_bytes(b"terlan-registry-local-root-v1"),
    };
    write_json_atomic(&path, &root)
}

fn build_snapshot(mirror: &Path, sequence: u64) -> Result<SnapshotRecord, String> {
    let packages_directory = mirror.join("packages");
    let mut packages = Vec::new();
    for entry in fs::read_dir(&packages_directory)
        .map_err(|error| format!("cannot read {}: {error}", packages_directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Registry index name is not UTF-8".to_string())?;
        packages.push(SnapshotPackage {
            name: name.into(),
            index: Digest {
                algorithm: "sha256".into(),
                value: hash_file(&path)?,
            },
        });
    }
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    let identity = packages
        .iter()
        .map(|package| format!("{}:{}", package.name, package.index.value))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(SnapshotRecord {
        schema: "terlan-registry-snapshot-v1".into(),
        sequence,
        root_version: 1,
        packages,
        signed_digest: digest_bytes(identity.as_bytes()),
    })
}

fn index_identity(index: &PackageIndexRecord) -> String {
    let versions = index
        .versions
        .iter()
        .map(|version| {
            format!(
                "{}:{}:{}:{}",
                version.version, version.archive.value, version.metadata.value, version.yanked
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n{}", index.repository_url, versions)
}

fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid Registry resource {}: {error}", path.display()))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Registry resource has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&temporary, json_bytes(value)?)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("cannot publish {}: {error}", path.display())
    })
}

fn json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(hex(Sha256::digest(bytes).as_slice()))
}

fn digest_bytes(bytes: &[u8]) -> Digest {
    Digest {
        algorithm: "sha256".into(),
        value: hex(Sha256::digest(bytes).as_slice()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "package_registry_mirror_test.rs"]
mod tests;
