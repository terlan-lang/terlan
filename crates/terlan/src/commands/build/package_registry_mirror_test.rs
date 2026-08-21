use super::*;
use crate::commands::build::package_publish::seal_publish_dry_run;
use crate::package_registry::model::DependencySource;
use std::path::PathBuf;

#[test]
fn publish_request_rejects_traversal_provenance_target_and_native_mutations() {
    let (root, output, sealed) = sealed_fixture("mutations");
    let request: PublishRequest = read_json(&sealed.request).unwrap();
    validate_publish_request(&request).unwrap();

    let mut traversal = request.clone();
    traversal.package_version.artifacts[0].path = "../secret".into();
    assert!(validate_publish_request(&traversal)
        .unwrap_err()
        .to_string()
        .contains("registry_publish_artifact"));

    let mut provenance = request.clone();
    provenance.package_version.provenance.value = "f".repeat(64);
    assert!(validate_publish_request(&provenance)
        .unwrap_err()
        .to_string()
        .contains("registry_publish_provenance"));

    let mut target = request.clone();
    target.package_version.targets.clear();
    assert!(validate_publish_request(&target)
        .unwrap_err()
        .to_string()
        .contains("registry_publish_target"));

    let mut native = request;
    native
        .package_version
        .capabilities
        .push("native.rust".into());
    assert!(validate_publish_request(&native)
        .unwrap_err()
        .to_string()
        .contains("registry_publish_native_artifact"));
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn publish_request_rejects_unbound_identities_mutable_git_and_malformed_archive_identity() {
    let (root, output, sealed) = sealed_fixture("admission-boundaries");
    let request: PublishRequest = read_json(&sealed.request).unwrap();

    let mut invalid_request_id = request.clone();
    invalid_request_id.request_id = "../retry".into();
    assert!(validate_publish_request(&invalid_request_id)
        .unwrap_err()
        .to_string()
        .contains("registry_publish_identity"));

    let mut mutable_git = request.clone();
    mutable_git.package_version.dependencies.push(
        crate::package_registry::model::DependencyRecord {
            schema: "terlan-registry-dependency-v1".into(),
            name: "git_dependency".into(),
            source: DependencySource::Git,
            requirement: "main".into(),
            registry: "https://github.com/terlan-lang/dependency".into(),
            optional: false,
            target: None,
            capabilities: Vec::new(),
            source_identity: Some("main".into()),
            integrity: None,
            options: Vec::new(),
        },
    );
    assert!(validate_publish_request(&mutable_git)
        .unwrap_err()
        .to_string()
        .contains("registry_dependency_git"));

    let mut malformed_digest = request;
    malformed_digest.package_version.archive.digest.value = "z".repeat(64);
    assert!(validate_publish_request(&malformed_digest)
        .unwrap_err()
        .to_string()
        .contains("registry_publish_limits"));

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn malformed_prior_snapshot_cannot_publish_partial_version() {
    let (root, output, sealed) = sealed_fixture("partial-snapshot");
    let mirror = temp("partial-mirror");
    fs::create_dir_all(&mirror).unwrap();
    fs::write(mirror.join("snapshot.json"), "{not-json}").unwrap();
    let error = publish_to_mirror(&sealed, &mirror).unwrap_err();
    assert!(error.contains("invalid Registry resource"));
    assert!(!mirror.join("packages/demo/1.0.0").exists());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(output).unwrap();
    fs::remove_dir_all(mirror).unwrap();
}

#[test]
fn yank_updates_index_and_snapshot_without_removing_archive() {
    let (root, output, sealed) = sealed_fixture("yank");
    let mirror = temp("yank-mirror");
    publish_to_mirror(&sealed, &mirror).unwrap();
    let archive = mirror.join("packages/demo/1.0.0/archive.tar.zst");
    let mut replacement: PackageIndexRecord =
        read_json(&mirror.join("packages/demo.json")).unwrap();
    replacement.name = "demo_next".into();
    replacement.repository_url = "https://github.com/terlan-lang/demo-next".into();
    write_json_atomic(&mirror.join("packages/demo_next.json"), &replacement).unwrap();

    let summary = yank_in_mirror(
        &mirror,
        "demo",
        "1.0.0",
        YankReason::Renamed,
        "Package renamed",
        Some("demo_next"),
    )
    .unwrap();
    assert_eq!(summary.sequence, 2);
    assert!(archive.is_file());
    let index: PackageIndexRecord = read_json(&mirror.join("packages/demo.json")).unwrap();
    assert!(index.versions[0].yanked);
    assert_eq!(index.latest_stable, None);
    let yank: YankRecord = read_json(&mirror.join("packages/demo/1.0.0/yank.json")).unwrap();
    assert_eq!(yank.state, YankState::Yanked);
    assert_eq!(yank.reason, YankReason::Renamed);
    assert_eq!(yank.message, "Package renamed");
    assert_eq!(yank.replacement_package.as_deref(), Some("demo_next"));
    assert!(yank_in_mirror(&mirror, "demo", "1.0.0", YankReason::Other, "again", None,).is_err());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(output).unwrap();
    fs::remove_dir_all(mirror).unwrap();
}

#[test]
fn package_index_uses_semantic_order_and_excludes_prereleases_from_latest_stable() {
    let mirror = temp("semantic-mirror");
    let mut fixtures = Vec::new();
    for version in ["1.9.0", "2.0.0-rc.1", "1.10.0"] {
        let fixture = sealed_version_fixture("semantic", version);
        publish_to_mirror(&fixture.2, &mirror).unwrap();
        fixtures.push(fixture);
    }
    let index: PackageIndexRecord = read_json(&mirror.join("packages/demo.json")).unwrap();
    assert_eq!(
        index
            .versions
            .iter()
            .map(|version| version.version.as_str())
            .collect::<Vec<_>>(),
        vec!["1.9.0", "1.10.0", "2.0.0-rc.1"]
    );
    assert_eq!(index.latest_stable.as_deref(), Some("1.10.0"));
    for (root, output, _) in fixtures {
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(output).unwrap();
    }
    fs::remove_dir_all(mirror).unwrap();
}

#[test]
fn registry_dependencies_fail_before_visibility_and_accept_a_matching_release() {
    let mirror = temp("dependency-mirror");
    fs::create_dir_all(&mirror).unwrap();

    let (missing_root, missing_output, missing) = sealed_fixture("missing-dependency");
    add_registry_dependency(&missing, "missing_package", ">=1.0.0, <2.0.0");
    let error = publish_to_mirror(&missing, &mirror).unwrap_err();
    assert!(error.contains("registry_dependency_missing"));
    assert!(!mirror.join("packages/demo/1.0.0").exists());

    let (invalid_root, invalid_output, invalid) = sealed_fixture("invalid-requirement");
    add_registry_dependency(&invalid, "dependency", "not a version requirement");
    let error = publish_to_mirror(&invalid, &mirror).unwrap_err();
    assert!(error.contains("registry_requirement"));
    assert!(!mirror.join("packages/demo/1.0.0").exists());

    let dependency_index = PackageIndexRecord {
        schema: "terlan-registry-package-index-v1".into(),
        name: "dependency".into(),
        repository_url: "https://github.com/terlan-lang/dependency".into(),
        versions: vec![PackageIndexVersion {
            version: "1.10.0".into(),
            archive: Digest {
                algorithm: "sha256".into(),
                value: "a".repeat(64),
            },
            metadata: Digest {
                algorithm: "sha256".into(),
                value: "b".repeat(64),
            },
            documentation: None,
            built_with: "terlan-0.0.7".into(),
            requires_terlan: ">=0.0.7, <0.1.0".into(),
            published_sequence: 1,
            published_at: "1970-01-01T00:00:00.000000Z".into(),
            yanked: false,
            yank: None,
        }],
        latest_stable: Some("1.10.0".into()),
        signed_digest: Digest {
            algorithm: "sha256".into(),
            value: "c".repeat(64),
        },
    };
    write_json_atomic(&mirror.join("packages/dependency.json"), &dependency_index).unwrap();

    let (valid_root, valid_output, valid) = sealed_fixture("valid-dependency");
    add_registry_dependency(&valid, "dependency", ">=1.9.0, <2.0.0");
    publish_to_mirror(&valid, &mirror).unwrap();
    assert!(mirror.join("packages/demo/1.0.0").is_dir());

    for path in [
        missing_root,
        missing_output,
        invalid_root,
        invalid_output,
        valid_root,
        valid_output,
        mirror,
    ] {
        fs::remove_dir_all(path).unwrap();
    }
}

fn add_registry_dependency(sealed: &PackageSealSummary, name: &str, requirement: &str) {
    let mut request: PublishRequest = read_json(&sealed.request).unwrap();
    request
        .package_version
        .dependencies
        .push(crate::package_registry::model::DependencyRecord {
            schema: "terlan-registry-dependency-v1".into(),
            name: name.into(),
            source: DependencySource::TerlanRegistry,
            requirement: requirement.into(),
            registry: "https://registry.terlan.dev".into(),
            optional: false,
            target: None,
            capabilities: Vec::new(),
            source_identity: None,
            integrity: None,
            options: Vec::new(),
        });
    fs::write(&sealed.request, json_bytes(&request).unwrap()).unwrap();
}

fn sealed_fixture(label: &str) -> (PathBuf, PathBuf, PackageSealSummary) {
    sealed_version_fixture(label, "1.0.0")
}

fn sealed_version_fixture(label: &str, version: &str) -> (PathBuf, PathBuf, PackageSealSummary) {
    let root = temp(&format!("{label}-source"));
    let output = temp(&format!("{label}-output"));
    fs::create_dir_all(root.join("src/demo")).unwrap();
    fs::write(
        root.join("terlan.toml"),
        format!("[package]\nname = \"demo\"\nversion = \"{version}\"\ndescription = \"A Registry correctness fixture.\"\nlicense = \"Apache-2.0\"\nrepository = \"https://github.com/terlan-lang/terlan\"\nlinks = [\"https://terlan.org/packages/demo\"]\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n"),
    )
    .unwrap();
    fs::write(root.join("src/demo/Main.terl"), "module demo.Main.\n").unwrap();
    let sealed = seal_publish_dry_run(&root, &output).unwrap();
    (root, output, sealed)
}

fn temp(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-registry-mirror-{label}-{}-{nonce}",
        std::process::id()
    ))
}
