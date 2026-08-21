//! Strict versioned records exchanged by Terlan package tooling and Registry.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "terlan-registry-protocol-v1";
pub const SHA256_ALGORITHM: &str = "sha256";

pub const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ARCHIVE_FILES: u32 = 4096;
pub const MAX_ARCHIVE_PATH_BYTES: u16 = 240;

/// SHA-256 or future algorithm-tagged content identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Digest {
    pub algorithm: String,
    pub value: String,
}

/// Canonical package name and semantic version pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub name: String,
    pub version: String,
}

/// Resource and path limits enforced while accepting an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveLimits {
    pub max_archive_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub max_files: u32,
    pub max_path_bytes: u16,
    pub symlinks: SymlinkPolicy,
}

/// Policy applied to symbolic links found inside package archives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymlinkPolicy {
    Reject,
}

/// Measured identity and expanded shape of one accepted archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveIdentity {
    pub format: String,
    pub digest: Digest,
    pub compressed_bytes: u64,
    pub unpacked_bytes: u64,
    pub file_count: u32,
}

/// One package dependency recorded in published metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencySource {
    TerlanRegistry,
    Git,
    Path,
    Npm,
    Cargo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Versioned dependency declaration embedded in published package metadata.
pub struct DependencyRecord {
    pub schema: String,
    pub name: String,
    pub source: DependencySource,
    pub requirement: String,
    pub registry: String,
    pub optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<Digest>,
    pub options: Vec<String>,
}

/// Meaning of the immutable source identity carried by a package release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceIdentityKind {
    RepositoryCommit,
    ArtifactSet,
}

/// Whether the Registry derived a source identity or retained a publisher claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceIdentityVerification {
    MaintainerClaimed,
    RegistryDerived,
}

/// Honest source identity for an immutable package version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub kind: SourceIdentityKind,
    pub value: String,
    pub verification: SourceIdentityVerification,
}

/// Semantic role of a file published with a package version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Source,
    Documentation,
    GeneratedBinding,
    Native,
    PublicApi,
}

/// Integrity and execution metadata for one published file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRecord {
    pub schema: String,
    pub kind: ArtifactKind,
    pub path: String,
    pub digest: Digest,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub executable: bool,
}

/// One named public HTTPS resource associated with a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageLink {
    pub name: String,
    pub url: String,
}

/// Complete immutable metadata for one published package version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageVersionRecord {
    pub schema: String,
    pub package: PackageIdentity,
    pub repository_url: String,
    pub description: String,
    pub license: String,
    pub links: Vec<PackageLink>,
    pub archive: ArchiveIdentity,
    pub dependencies: Vec<DependencyRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    pub targets: Vec<String>,
    pub capabilities: Vec<String>,
    pub built_with: String,
    pub requires_terlan: String,
    pub source_identity: SourceIdentity,
    pub provenance: Digest,
    pub public_api: Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<ArchiveIdentity>,
}

/// Signed publisher request to admit one package archive and metadata record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRequest {
    pub schema: String,
    pub package_version: PackageVersionRecord,
    pub publisher_key_id: String,
    pub request_id: String,
    pub archive_upload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_upload: Option<String>,
    pub limits: ArchiveLimits,
}

/// Registry decision for a publish request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublishStatus {
    Accepted,
    Rejected,
}

/// Stable result returned after evaluating a publish request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishResult {
    pub schema: String,
    pub publish_id: String,
    pub request_id: String,
    pub package: PackageIdentity,
    pub status: PublishStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_code: Option<String>,
    pub snapshot: Digest,
}

/// Requested visibility state for an existing package version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum YankState {
    Yanked,
    Restored,
}

/// Stable operator-visible reason class for withdrawing one release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum YankReason {
    Security,
    InvalidMetadata,
    Deprecated,
    Renamed,
    Other,
}

/// Auditable yank or restore operation for one package version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YankRecord {
    pub schema: String,
    pub package: PackageIdentity,
    pub state: YankState,
    pub reason: YankReason,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_package: Option<String>,
    pub publisher_key_id: String,
    pub sequence: u64,
}

/// Public verification key and its authorized Registry roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustKey {
    pub key_id: String,
    pub algorithm: String,
    pub public_key_base64: String,
    pub roles: Vec<String>,
}

/// One Registry signing-key assertion over exact envelope payload bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSignature {
    pub key_id: String,
    pub algorithm: String,
    pub signature_base64: String,
}

/// Origin- and route-bound signature envelope for every trusted resource.
///
/// `payload_base64` preserves the exact signed bytes. Consumers verify the
/// digest, origin, route, and signature threshold before decoding or parsing
/// those bytes as a root, snapshot, or package-index record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedResourceRecord {
    pub schema: String,
    pub origin: String,
    pub resource: String,
    pub payload_base64: String,
    pub payload: Digest,
    pub signatures: Vec<ResourceSignature>,
}

/// Versioned Registry trust root signed by the preceding root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootRecord {
    pub schema: String,
    pub version: u64,
    pub previous_version: Option<u64>,
    pub threshold: u16,
    pub keys: Vec<TrustKey>,
    pub signed_digest: Digest,
}

/// Package-index digest included in a Registry snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPackage {
    pub name: String,
    pub index: Digest,
}

/// Signed, monotonically sequenced view of all package indexes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRecord {
    pub schema: String,
    pub sequence: u64,
    pub root_version: u64,
    pub packages: Vec<SnapshotPackage>,
    pub signed_digest: Digest,
}

/// One version entry in a package's append-only index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIndexVersion {
    pub version: String,
    pub archive: Digest,
    pub metadata: Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<Digest>,
    pub built_with: String,
    pub requires_terlan: String,
    pub published_sequence: u64,
    pub published_at: String,
    pub yanked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yank: Option<PackageIndexYank>,
}

/// Current structured withdrawal state carried in trusted package metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIndexYank {
    pub reason: YankReason,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_package: Option<String>,
}

/// Signed index of all published versions for one package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIndexRecord {
    pub schema: String,
    pub name: String,
    pub repository_url: String,
    pub versions: Vec<PackageIndexVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_stable: Option<String>,
    pub signed_digest: Digest,
}
