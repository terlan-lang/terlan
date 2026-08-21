//! Canonical protocol examples generated from the public Rust models.

use serde::Serialize;

use super::{admission::artifact_identity, model::*, ProtocolDocument};

pub(super) fn fixture_documents() -> Result<Vec<ProtocolDocument>, serde_json::Error> {
    let dependency = DependencyRecord {
        schema: "terlan-registry-dependency-v1".into(),
        name: "terlan_json".into(),
        source: DependencySource::TerlanRegistry,
        requirement: ">=1.2.0 <2.0.0".into(),
        registry: "https://registry.terlan.dev".into(),
        optional: false,
        target: None,
        capabilities: vec!["json".into()],
        source_identity: None,
        integrity: None,
        options: Vec::new(),
    };
    let artifact = ArtifactRecord {
        schema: "terlan-registry-artifact-v1".into(),
        kind: ArtifactKind::Source,
        path: "src/registry_protocol/Protocol.terl".into(),
        digest: digest('a'),
        bytes: 1_284,
        target: None,
        executable: false,
    };
    let provenance = artifact_identity("terlan-package-provenance-v1", std::iter::once(&artifact));
    let package_version = PackageVersionRecord {
        schema: "terlan-registry-package-version-v1".into(),
        package: package(),
        repository_url: "https://github.com/terlan-lang/terlan".into(),
        description: "Terlan Registry protocol records.".into(),
        license: "Apache-2.0".into(),
        links: vec![PackageLink {
            name: "github.com".into(),
            url: "https://github.com/terlan-lang/terlan".into(),
        }],
        archive: ArchiveIdentity {
            format: "tar.zst".into(),
            digest: digest('b'),
            compressed_bytes: 24_576,
            unpacked_bytes: 98_304,
            file_count: 1,
        },
        dependencies: vec![dependency.clone()],
        artifacts: vec![artifact.clone()],
        targets: vec!["terlan-vm".into()],
        capabilities: vec!["http-client".into(), "json".into()],
        built_with: "terlan-0.0.8".into(),
        requires_terlan: ">=0.0.8, <0.1.0".into(),
        source_identity: SourceIdentity {
            kind: SourceIdentityKind::ArtifactSet,
            value: provenance.clone(),
            verification: SourceIdentityVerification::RegistryDerived,
        },
        provenance: Digest {
            algorithm: SHA256_ALGORITHM.into(),
            value: provenance,
        },
        public_api: digest('d'),
        documentation: Some(ArchiveIdentity {
            format: "tar.zst".into(),
            digest: digest('e'),
            compressed_bytes: 4_096,
            unpacked_bytes: 12_288,
            file_count: 2,
        }),
    };
    let publish_request = PublishRequest {
        schema: "terlan-registry-publish-request-v1".into(),
        package_version: package_version.clone(),
        publisher_key_id: "registry-bootstrap-2026".into(),
        request_id: "publish-00000001".into(),
        archive_upload: "uploads/publish-00000001.tar.zst".into(),
        documentation_upload: Some("uploads/publish-00000001.docs.tar.zst".into()),
        limits: ArchiveLimits {
            max_archive_bytes: MAX_ARCHIVE_BYTES,
            max_unpacked_bytes: MAX_UNPACKED_BYTES,
            max_files: MAX_ARCHIVE_FILES,
            max_path_bytes: MAX_ARCHIVE_PATH_BYTES,
            symlinks: SymlinkPolicy::Reject,
        },
    };
    let publish_result = PublishResult {
        schema: "terlan-registry-publish-result-v1".into(),
        publish_id: "release-00000001".into(),
        request_id: "publish-00000001".into(),
        package: package(),
        status: PublishStatus::Accepted,
        rejection_code: None,
        snapshot: digest('f'),
    };
    let yank = YankRecord {
        schema: "terlan-registry-yank-v1".into(),
        package: package(),
        state: YankState::Yanked,
        reason: YankReason::InvalidMetadata,
        message: "invalid release metadata".into(),
        replacement_package: None,
        publisher_key_id: "registry-bootstrap-2026".into(),
        sequence: 2,
    };
    let root = RootRecord {
        schema: "terlan-registry-root-v1".into(),
        version: 1,
        previous_version: None,
        threshold: 1,
        keys: vec![TrustKey {
            key_id: "registry-bootstrap-2026".into(),
            algorithm: "ed25519".into(),
            public_key_base64: "MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDA=".into(),
            roles: vec!["root".into(), "snapshot".into()],
        }],
        signed_digest: digest('1'),
    };
    let snapshot = SnapshotRecord {
        schema: "terlan-registry-snapshot-v1".into(),
        sequence: 1,
        root_version: 1,
        packages: vec![SnapshotPackage {
            name: "registry_protocol".into(),
            index: digest('2'),
        }],
        signed_digest: digest('3'),
    };
    let package_index = PackageIndexRecord {
        schema: "terlan-registry-package-index-v1".into(),
        name: "registry_protocol".into(),
        repository_url: "https://github.com/terlan-lang/terlan".into(),
        versions: vec![PackageIndexVersion {
            version: "1.0.0".into(),
            archive: digest('b'),
            metadata: digest('4'),
            documentation: Some(digest('e')),
            built_with: "terlan-0.0.8".into(),
            requires_terlan: ">=0.0.8, <0.1.0".into(),
            published_sequence: 1,
            published_at: "2026-08-20T12:00:00.000000Z".into(),
            yanked: false,
            yank: None,
        }],
        latest_stable: Some("1.0.0".into()),
        signed_digest: digest('5'),
    };
    let signed_resource = SignedResourceRecord {
        schema: "terlan-registry-signed-resource-v1".into(),
        origin: "https://registry.terlan.dev".into(),
        resource: "/repo/v1/snapshot.json".into(),
        payload_base64: "eyJzY2hlbWEiOiJ0ZXJsYW4tcmVnaXN0cnktc25hcHNob3QtdjEifQ==".into(),
        payload: digest('6'),
        signatures: vec![ResourceSignature {
            key_id: "registry-bootstrap-2026".into(),
            algorithm: "ed25519".into(),
            signature_base64: "MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA==".into(),
        }],
    };

    Ok(vec![
        fixture("publish-request.json", &publish_request)?,
        fixture("publish-result.json", &publish_result)?,
        fixture("package-version.json", &package_version)?,
        fixture("dependency.json", &dependency)?,
        fixture("artifact.json", &artifact)?,
        fixture("yank.json", &yank)?,
        fixture("root.json", &root)?,
        fixture("snapshot.json", &snapshot)?,
        fixture("package-index.json", &package_index)?,
        fixture("signed-resource.json", &signed_resource)?,
    ])
}

fn fixture<T: Serialize>(
    file_name: &'static str,
    value: &T,
) -> Result<ProtocolDocument, serde_json::Error> {
    Ok(ProtocolDocument {
        file_name,
        value: serde_json::to_value(value)?,
    })
}

fn package() -> PackageIdentity {
    PackageIdentity {
        name: "registry_protocol".into(),
        version: "1.0.0".into(),
    }
}

fn digest(value: char) -> Digest {
    Digest {
        algorithm: SHA256_ALGORITHM.into(),
        value: value.to_string().repeat(64),
    }
}
