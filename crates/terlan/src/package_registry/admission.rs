//! Registry admission policy shared by the package producer and serving runtime.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use sha2::{Digest as _, Sha256};
use url::{Host, Url};

use super::model::{
    ArtifactKind, DependencySource, PackageIndexRecord, PublishRequest, SourceIdentityKind,
    SourceIdentityVerification, MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_ARCHIVE_PATH_BYTES,
    MAX_UNPACKED_BYTES,
};
use super::{canonical_version, parse_requirement, requirement_matches};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Stable admission-policy failure returned before registry state is mutated.
pub struct AdmissionError(String);

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AdmissionError {}

impl From<String> for AdmissionError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for AdmissionError {
    fn from(message: &str) -> Self {
        Self(message.to_string())
    }
}

/// Validates every signed metadata field whose correctness does not depend on
/// mutable Registry state or the uploaded archive bytes.
pub fn validate_publish_request(request: &PublishRequest) -> Result<(), AdmissionError> {
    if request.schema != "terlan-registry-publish-request-v1" {
        return Err("error[registry_publish_schema]: publish request schema is unsupported".into());
    }
    canonical_version(&request.package_version.package.version)
        .map_err(|error| error.to_string())?;
    validate_spdx_expression(&request.package_version.license)?;
    let mut link_names = BTreeSet::new();
    if request.package_version.description.trim().is_empty()
        || request.package_version.description.trim() != request.package_version.description
        || request.package_version.description.len() > 500
        || request.package_version.license.trim() != request.package_version.license
        || request.package_version.license.len() > 200
        || request.package_version.links.is_empty()
        || request.package_version.links.iter().any(|link| {
            link.name.trim().is_empty()
                || link.name.trim() != link.name
                || !link_names.insert(link.name.as_str())
                || !valid_public_https_url(&link.url)
        })
    {
        return Err("error[registry_publish_metadata]: publication metadata is invalid".into());
    }
    if request.package_version.schema != "terlan-registry-package-version-v1"
        || !canonical_package_name(&request.package_version.package.name)
        || !valid_public_https_url(&request.package_version.repository_url)
        || request.package_version.targets.is_empty()
        || request
            .package_version
            .targets
            .iter()
            .any(|target| target.trim().is_empty())
    {
        return Err("error[registry_publish_target]: target metadata is missing or invalid".into());
    }
    if !safe_identity_segment(&request.publisher_key_id)
        || !safe_identity_segment(&request.request_id)
        || !safe_artifact_path(&request.archive_upload)
        || request
            .documentation_upload
            .as_deref()
            .is_some_and(|path| !safe_artifact_path(path))
    {
        return Err(
            "error[registry_publish_identity]: request, signer, or upload identity is invalid"
                .into(),
        );
    }
    let mut dependency_identities = BTreeSet::new();
    for dependency in &request.package_version.dependencies {
        let identity = format!(
            "{:?}\0{}\0{}",
            dependency.source,
            dependency.name,
            dependency.target.as_deref().unwrap_or("")
        );
        if dependency.schema != "terlan-registry-dependency-v1"
            || !canonical_package_name(&dependency.name)
            || !dependency_identities.insert(identity)
            || dependency
                .capabilities
                .iter()
                .any(|capability| capability.trim().is_empty())
        {
            return Err(
                "error[registry_dependency_metadata]: dependency metadata is invalid".into(),
            );
        }
        match dependency.source {
            DependencySource::TerlanRegistry => {
                parse_requirement(&dependency.requirement).map_err(|error| error.to_string())?;
                if !valid_registry_origin(&dependency.registry)
                    || dependency.source_identity.is_some()
                    || dependency.integrity.is_some()
                    || !dependency.options.is_empty()
                {
                    return Err(AdmissionError(format!(
                        "error[registry_dependency_registry]: dependency `{}` requires a public HTTPS Registry URL",
                        dependency.name
                    )));
                }
            }
            DependencySource::Path => {
                return Err(AdmissionError(format!(
                    "error[registry_dependency_path]: published package dependency `{}` must use a Registry or immutable Git source",
                    dependency.name
                )));
            }
            DependencySource::Git => {
                if !valid_public_https_url(&dependency.registry)
                    || !is_immutable_git_revision(&dependency.requirement)
                    || dependency.source_identity.as_deref()
                        != Some(dependency.requirement.as_str())
                    || dependency.integrity.is_some()
                    || !dependency.options.is_empty()
                {
                    return Err(AdmissionError(format!(
                        "error[registry_dependency_git]: dependency `{}` requires a public HTTPS URL and a full immutable commit",
                        dependency.name
                    )));
                }
            }
            DependencySource::Npm => {
                validate_exact_ecosystem_dependency("npm", dependency)?;
            }
            DependencySource::Cargo => {
                validate_exact_ecosystem_dependency("cargo", dependency)?;
            }
        }
    }
    let limits = &request.limits;
    if limits.max_archive_bytes != MAX_ARCHIVE_BYTES
        || limits.max_unpacked_bytes != MAX_UNPACKED_BYTES
        || limits.max_files != MAX_ARCHIVE_FILES
        || limits.max_path_bytes != MAX_ARCHIVE_PATH_BYTES
        || request.package_version.archive.format != "tar.zst"
        || request.package_version.archive.digest.algorithm != "sha256"
        || !is_sha256(&request.package_version.archive.digest.value)
        || request.package_version.archive.compressed_bytes == 0
        || request.package_version.archive.compressed_bytes > MAX_ARCHIVE_BYTES
        || request.package_version.archive.unpacked_bytes == 0
        || request.package_version.archive.unpacked_bytes > MAX_UNPACKED_BYTES
        || request.package_version.archive.file_count == 0
        || request.package_version.archive.file_count > MAX_ARCHIVE_FILES
    {
        return Err(
            "error[registry_publish_limits]: archive limits differ from protocol v1".into(),
        );
    }
    if request.package_version.built_with.trim().is_empty()
        || request.package_version.built_with.len() > 100
        || parse_requirement(&request.package_version.requires_terlan).is_err()
    {
        return Err(
            "error[registry_publish_compiler]: producing compiler or Terlan compatibility requirement is invalid"
                .into(),
        );
    }
    if let Some(documentation) = &request.package_version.documentation {
        if documentation.format != "tar.zst"
            || documentation.digest.algorithm != "sha256"
            || !is_sha256(&documentation.digest.value)
            || documentation.compressed_bytes == 0
            || documentation.compressed_bytes > MAX_ARCHIVE_BYTES
            || documentation.unpacked_bytes > MAX_UNPACKED_BYTES
            || documentation.file_count == 0
            || documentation.file_count > MAX_ARCHIVE_FILES
            || request.documentation_upload.is_none()
        {
            return Err(
                "error[registry_publish_documentation]: documentation object identity is invalid"
                    .into(),
            );
        }
    } else if request.documentation_upload.is_some() {
        return Err(
            "error[registry_publish_documentation]: documentation upload has no signed object identity"
                .into(),
        );
    }
    let mut paths = BTreeSet::new();
    for artifact in &request.package_version.artifacts {
        if artifact.schema != "terlan-registry-artifact-v1"
            || !safe_artifact_path(&artifact.path)
            || !paths.insert(artifact.path.as_str())
            || artifact.digest.algorithm != "sha256"
            || !is_sha256(&artifact.digest.value)
        {
            return Err(
                "error[registry_publish_artifact]: artifact inventory is unsafe or invalid".into(),
            );
        }
    }
    if paths.len() != request.package_version.archive.file_count as usize {
        return Err(
            "error[registry_publish_artifact]: archive file count differs from artifact inventory"
                .into(),
        );
    }
    if request
        .package_version
        .capabilities
        .iter()
        .any(|capability| capability == "native.rust")
        && !request
            .package_version
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == ArtifactKind::Native)
    {
        return Err("error[registry_publish_native_artifact]: native capability lacks declared native artifacts".into());
    }
    let provenance = artifact_identity(
        "terlan-package-provenance-v1",
        request.package_version.artifacts.iter(),
    );
    let source_identity_valid = match request.package_version.source_identity.kind {
        SourceIdentityKind::ArtifactSet => {
            request.package_version.source_identity.value == provenance
                && request.package_version.source_identity.verification
                    == SourceIdentityVerification::RegistryDerived
        }
        SourceIdentityKind::RepositoryCommit => {
            is_immutable_git_revision(&request.package_version.source_identity.value)
                && request.package_version.source_identity.verification
                    == SourceIdentityVerification::MaintainerClaimed
        }
    };
    if request.package_version.provenance.algorithm != "sha256"
        || request.package_version.provenance.value != provenance
        || !source_identity_valid
    {
        return Err(
            "error[registry_publish_provenance]: provenance does not match artifact inventory"
                .into(),
        );
    }
    Ok(())
}

/// Validates Registry dependencies against the exact local snapshot used for
/// one publication decision.
pub fn validate_registry_dependencies(
    request: &PublishRequest,
    mirror: &Path,
) -> Result<(), AdmissionError> {
    for dependency in &request.package_version.dependencies {
        match dependency.source {
            DependencySource::TerlanRegistry => {
                let index_path = mirror
                    .join("packages")
                    .join(format!("{}.json", dependency.name));
                let index: PackageIndexRecord = read_json(&index_path).map_err(|_| {
                    format!(
                        "error[registry_dependency_missing]: dependency `{}` is absent from the selected Registry snapshot",
                        dependency.name
                    )
                })?;
                if index.schema != "terlan-registry-package-index-v1"
                    || index.name != dependency.name
                    || !index.versions.iter().any(|candidate| {
                        !candidate.yanked
                            && requirement_matches(&dependency.requirement, &candidate.version)
                                .unwrap_or(false)
                    })
                {
                    return Err(AdmissionError(format!(
                        "error[registry_dependency_requirement]: dependency `{}` has no visible version matching `{}`",
                        dependency.name, dependency.requirement
                    )));
                }
            }
            DependencySource::Path => {
                return Err(AdmissionError(format!(
                    "error[registry_dependency_path]: published package dependency `{}` must use a Registry or immutable Git source",
                    dependency.name
                )));
            }
            DependencySource::Git | DependencySource::Npm | DependencySource::Cargo => {}
        }
    }
    Ok(())
}

/// Extracts a bounded archive and checks every declared file against the signed
/// byte count and digest inventory.
pub fn validate_archive_inventory(
    archive: &Path,
    request: &PublishRequest,
    staging: &Path,
) -> Result<(), AdmissionError> {
    let metadata = fs::metadata(archive)
        .map_err(|error| format!("cannot inspect {}: {error}", archive.display()))?;
    if !metadata.is_file()
        || metadata.len() != request.package_version.archive.compressed_bytes
        || metadata.len() > MAX_ARCHIVE_BYTES
    {
        return Err(
            "error[registry_publish_archive_size]: uploaded archive size differs from request"
                .into(),
        );
    }
    if hash_file(archive)? != request.package_version.archive.digest.value {
        return Err(
            "error[registry_publish_archive_checksum]: uploaded archive differs from request"
                .into(),
        );
    }
    let inventory = staging.join("inventory");
    if inventory.exists() {
        return Err(
            "error[registry_publish_staging]: archive inventory path already exists".into(),
        );
    }
    terlan_archive::extract_tar_zstd(archive, &inventory).map_err(|error| error.to_string())?;
    let result = request
        .package_version
        .artifacts
        .iter()
        .try_fold(0_u64, |total, artifact| {
            let path = inventory.join(&artifact.path);
            let metadata = fs::metadata(&path).map_err(|_| {
                format!(
                    "error[registry_publish_artifact]: declared artifact is absent: {}",
                    artifact.path
                )
            })?;
            if !metadata.is_file()
                || metadata.len() != artifact.bytes
                || hash_file(&path)? != artifact.digest.value
            {
                return Err(AdmissionError(format!(
                    "error[registry_publish_artifact]: artifact bytes differ: {}",
                    artifact.path
                )));
            }
            total
                .checked_add(metadata.len())
                .ok_or_else(|| "error[registry_publish_limits]: byte count overflow".into())
        });
    let _ = fs::remove_dir_all(&inventory);
    let total = result?;
    if total != request.package_version.archive.unpacked_bytes {
        return Err(
            "error[registry_publish_artifact]: unpacked size differs from inventory".into(),
        );
    }
    Ok(())
}

pub(crate) fn validate_spdx_expression(source: &str) -> Result<(), AdmissionError> {
    let expression = spdx::Expression::parse(source).map_err(|_| {
        AdmissionError(format!(
            "error[registry_license]: `{source}` is not a valid SPDX expression"
        ))
    })?;
    let deprecated = expression.requirements().any(|requirement| {
        let license_deprecated = match requirement.req.license {
            spdx::LicenseItem::Spdx { id, .. } => id.is_deprecated(),
            spdx::LicenseItem::Other { .. } => false,
        };
        let addition_deprecated = match requirement.req.addition {
            Some(spdx::AdditionItem::Spdx(id)) => id.is_deprecated(),
            Some(spdx::AdditionItem::Other { .. }) | None => false,
        };
        license_deprecated || addition_deprecated
    });
    if deprecated {
        return Err(AdmissionError(format!(
            "error[registry_license_deprecated]: `{source}` uses a deprecated SPDX identifier"
        )));
    }
    Ok(())
}

pub(crate) fn safe_identity_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Returns whether a package name has the one canonical Registry spelling.
///
/// Names are deliberately narrower than generic request/key identifiers:
/// lowercase ASCII, an alphanumeric first and last byte, and only `-` or `_`
/// punctuation internally. Keeping `.` out avoids path/namespace aliases.
pub(crate) fn canonical_package_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    let bytes = value.as_bytes();
    let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    bytes[0].is_ascii_lowercase()
        && alphanumeric(bytes[bytes.len() - 1])
        && bytes
            .iter()
            .all(|byte| alphanumeric(*byte) || matches!(*byte, b'_' | b'-'))
}

fn safe_artifact_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with("../")
        && !path.contains("/../")
        && !path.ends_with("/..")
        && !path.contains('\\')
        && !path.contains(':')
        && path.len() <= MAX_ARCHIVE_PATH_BYTES as usize
}

pub(crate) fn valid_public_https_url(value: &str) -> bool {
    value.trim() == value
        && Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none_or(|port| port == 443)
                && url.host().is_some_and(public_host)
        })
}

fn valid_registry_origin(value: &str) -> bool {
    if valid_public_https_url(value) {
        return true;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    value.trim() == value
        && url.scheme() == "http"
        && matches!(url.host(), Some(Host::Ipv4(address)) if address.is_loopback())
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn public_host(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            !domain.is_empty()
                && domain.contains('.')
                && domain != "localhost"
                && !domain.ends_with(".localhost")
                && !domain.ends_with(".local")
                && !domain.ends_with(".internal")
                && !domain.ends_with(".invalid")
        }
        Host::Ipv4(address) => {
            let octets = address.octets();
            !address.is_private()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_unspecified()
                && !address.is_broadcast()
                && !address.is_multicast()
                && octets[0] != 0
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                && !(octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                && !(octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        }
        Host::Ipv6(address) => {
            let segments = address.segments();
            !(address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || address.is_unicast_link_local()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] == 0x2001 && segments[1] == 0x0db8))
        }
    }
}

fn is_immutable_git_revision(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_exact_ecosystem_dependency(
    ecosystem: &str,
    dependency: &super::model::DependencyRecord,
) -> Result<(), AdmissionError> {
    let Some((package, version)) = dependency.requirement.rsplit_once('@') else {
        return Err(AdmissionError(format!(
            "error[registry_dependency_{ecosystem}]: dependency `{}` requires an exact package version",
            dependency.name
        )));
    };
    let integrity_valid = dependency
        .integrity
        .as_ref()
        .is_some_and(|integrity| integrity.algorithm == "sha256" && is_sha256(&integrity.value));
    if dependency.registry != ecosystem
        || package.is_empty()
        || version.is_empty()
        || canonical_version(version).is_err()
        || dependency.source_identity.as_deref() != Some(dependency.requirement.as_str())
        || !integrity_valid
        || dependency
            .options
            .iter()
            .any(|option| option.trim().is_empty())
        || (ecosystem == "npm" && !dependency.options.is_empty())
    {
        return Err(AdmissionError(format!(
            "error[registry_dependency_{ecosystem}]: dependency `{}` requires an exact package version",
            dependency.name
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "admission_test.rs"]
mod tests;

pub(super) fn artifact_identity<'a>(
    domain: &str,
    artifacts: impl Iterator<Item = &'a super::model::ArtifactRecord>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for artifact in artifacts {
        hasher.update([0]);
        hasher.update(artifact.path.as_bytes());
        hasher.update([0]);
        hasher.update(artifact.digest.value.as_bytes());
    }
    hex(hasher.finalize().as_slice())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn hash_file(path: &Path) -> Result<String, AdmissionError> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(hex(Sha256::digest(bytes).as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AdmissionError> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read Registry resource {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AdmissionError(format!(
            "invalid Registry resource {}: {error}",
            path.display()
        ))
    })
}
