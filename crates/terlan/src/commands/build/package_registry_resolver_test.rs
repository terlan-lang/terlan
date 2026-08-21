use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

use super::*;
use crate::package_registry::model::*;

const ORIGIN: &str = "https://registry.example.test";
const KEY_ID: &str = "root-1";

#[test]
fn trusted_offline_resolution_is_deterministic_and_checksum_closed() {
    let first = temp("first");
    let second = temp("second");
    prepare_remote(&first, false);
    prepare_remote(&second, false);
    resolve_registry_package(&resolve_args(&first), &first).unwrap();
    resolve_registry_package(&resolve_args(&second), &second).unwrap();
    assert_eq!(
        fs::read(first.join("terlan.lock")).unwrap(),
        fs::read(second.join("terlan.lock")).unwrap()
    );

    let archive = first
        .join(".terlan/packages/registry")
        .read_dir()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("archive.tar.zst");
    fs::write(&archive, "mutated").unwrap();
    let error = resolve_locked_dependency(&first, "demo", ORIGIN, "1.0.0").unwrap_err();
    assert!(error.contains("registry_checksum_mismatch"));
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

#[test]
fn existing_lock_fetches_yanked_archive_but_new_selection_fails() {
    let locked = temp("locked");
    prepare_remote(&locked, false);
    resolve_registry_package(&resolve_args(&locked), &locked).unwrap();
    prepare_remote(&locked, true);
    resolve_registry_package(&resolve_args(&locked), &locked)
        .expect("existing lock remains fetchable");

    let fresh = temp("fresh-yank");
    prepare_remote(&fresh, true);
    let error = resolve_registry_package(&resolve_args(&fresh), &fresh).unwrap_err();
    assert!(error.contains("registry_dependency_conflict"));
    assert!(error.contains("1.0.0 (yanked)"));
    let mut allowed = resolve_args(&fresh);
    allowed.allow_yanked = true;
    resolve_registry_package(&allowed, &fresh).unwrap();
    fs::remove_dir_all(locked).unwrap();
    fs::remove_dir_all(fresh).unwrap();
}

#[test]
fn project_resolution_update_and_tree_use_lock_v3() {
    let project = temp("project-graph");
    prepare_remote(&project, false);
    fs::write(
        project.join("terlan.toml"),
        format!(
            "[package]\nname = \"consumer\"\nversion = \"1.0.0\"\n\n[dependencies]\ndemo = {{ registry = \"{ORIGIN}\", version = \"=1.0.0\" }}\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n"
        ),
    )
    .unwrap();
    fs::create_dir_all(project.join("src/consumer")).unwrap();
    fs::write(
        project.join("src/consumer/Main.terl"),
        "module consumer.Main.\n",
    )
    .unwrap();
    let mut args = resolve_args(&project);
    args.package = None;
    args.version = None;
    let resolution = resolve_registry_package(&args, &project).unwrap();
    assert_eq!(resolution.package_count, 1);
    let lock = fs::read_to_string(project.join("terlan.lock")).unwrap();
    assert!(lock.contains("version = 3"));
    assert!(lock.contains("resolver = \"terlan-registry-resolver-v2\""));
    resolve_locked_dependency(&project, "demo", ORIGIN, "=1.0.0").unwrap();
    assert_eq!(
        run_tree(&["tree".into()], &project),
        std::process::ExitCode::SUCCESS
    );
    fs::remove_dir_all(project).unwrap();
}

#[test]
fn add_and_remove_drive_trusted_project_resolution_transactionally() {
    let project = temp("project-commands");
    prepare_remote(&project, false);
    fs::write(
        project.join("terlan.toml"),
        "[package]\nname = \"consumer\"\nversion = \"1.0.0\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .unwrap();
    fs::create_dir_all(project.join("src/consumer")).unwrap();
    fs::write(
        project.join("src/consumer/Main.terl"),
        "module consumer.Main.\n",
    )
    .unwrap();
    let common = vec![
        "--registry".to_string(),
        ORIGIN.to_string(),
        "--trust-root".to_string(),
        project.join("trust-pin.json").display().to_string(),
        "--offline".to_string(),
    ];
    let mut add = vec!["add".into(), "demo".into(), "=1.0.0".into()];
    add.extend(common.clone());
    assert_eq!(
        super::super::package_registry_commands::run_add(&add, &project),
        std::process::ExitCode::SUCCESS
    );
    assert!(fs::read_to_string(project.join("terlan.toml"))
        .unwrap()
        .contains("demo = { registry ="));
    assert!(fs::read_to_string(project.join("terlan.lock"))
        .unwrap()
        .contains("name = \"demo\""));

    let mut remove = vec!["remove".into(), "demo".into()];
    remove.extend(common);
    assert_eq!(
        super::super::package_registry_commands::run_remove(&remove, &project),
        std::process::ExitCode::SUCCESS
    );
    assert!(!fs::read_to_string(project.join("terlan.toml"))
        .unwrap()
        .contains("demo = { registry ="));
    let lock = read_lockfile(&project.join("terlan.lock")).unwrap();
    assert!(lock.registry.is_empty());
    fs::remove_dir_all(project).unwrap();
}

fn resolve_args(root: &Path) -> ResolveArgs {
    ResolveArgs {
        registry: ORIGIN.into(),
        trust_root: root.join("trust-pin.json"),
        package: Some("demo".into()),
        version: Some("1.0.0".into()),
        updates: BTreeSet::new(),
        update_all: false,
        allow_yanked: false,
        offline: true,
    }
}

fn prepare_remote(root: &Path, yanked: bool) {
    fs::create_dir_all(root).unwrap();
    let seed = STANDARD.encode([29_u8; 32]);
    let public_key = crate::runtime::native::ed25519::sign(&seed, "probe")
        .unwrap()
        .public_key_base64;
    let pin = TrustPin {
        schema: "terlan-registry-trust-pin-v1".into(),
        origin: ORIGIN.into(),
        key_id: KEY_ID.into(),
        algorithm: "ed25519".into(),
        public_key_base64: public_key.clone(),
    };
    fs::write(
        root.join("trust-pin.json"),
        serde_json::to_vec(&pin).unwrap(),
    )
    .unwrap();

    let source = temp("source");
    let sealed_root = temp("sealed");
    fs::create_dir_all(source.join("src/demo")).unwrap();
    fs::write(
        source.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"1.0.0\"\ndescription = \"Trusted resolver fixture.\"\nlicense = \"Apache-2.0\"\nrepository = \"https://github.com/terlan-lang/terlan\"\nlinks = [\"https://terlan.org/packages/demo\"]\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .unwrap();
    fs::write(source.join("src/demo/Main.terl"), "module demo.Main.\n").unwrap();
    let sealed =
        super::super::package_publish::seal_publish_dry_run(&source, &sealed_root).unwrap();
    let metadata_bytes = fs::read(&sealed.request).unwrap();
    let request: PublishRequest = serde_json::from_slice(&metadata_bytes).unwrap();
    let archive_bytes = fs::read(&sealed.archive).unwrap();

    let root_record = RootRecord {
        schema: "terlan-registry-root-v1".into(),
        version: 1,
        previous_version: None,
        threshold: 1,
        keys: vec![TrustKey {
            key_id: KEY_ID.into(),
            algorithm: "ed25519".into(),
            public_key_base64: public_key,
            roles: vec!["root".into(), "snapshot".into(), "package-index".into()],
        }],
        signed_digest: digest(b"placeholder"),
    };
    let root_bytes = envelope("/repo/v1/root.json", &root_record, &seed);
    let index = PackageIndexRecord {
        schema: "terlan-registry-package-index-v1".into(),
        name: "demo".into(),
        repository_url: request.package_version.repository_url.clone(),
        versions: vec![PackageIndexVersion {
            version: "1.0.0".into(),
            archive: digest(&archive_bytes),
            metadata: digest(&metadata_bytes),
            documentation: None,
            built_with: request.package_version.built_with.clone(),
            requires_terlan: request.package_version.requires_terlan.clone(),
            published_sequence: 1,
            published_at: "2026-08-20T00:00:00.000000Z".into(),
            yanked,
            yank: yanked.then_some(PackageIndexYank {
                reason: YankReason::Deprecated,
                message: "superseded".into(),
                replacement_package: None,
            }),
        }],
        latest_stable: (!yanked).then_some("1.0.0".into()),
        signed_digest: digest(b"placeholder"),
    };
    let index_route = "/repo/v1/packages/demo.json";
    let index_bytes = envelope(index_route, &index, &seed);
    let snapshot = SnapshotRecord {
        schema: "terlan-registry-snapshot-v1".into(),
        sequence: if yanked { 2 } else { 1 },
        root_version: 1,
        packages: vec![SnapshotPackage {
            name: "demo".into(),
            index: digest(&index_bytes),
        }],
        signed_digest: digest(b"placeholder"),
    };
    let snapshot_bytes = envelope("/repo/v1/snapshot.json", &snapshot, &seed);

    let cache_root = root
        .join(".terlan/registry/remotes")
        .join(sha256_hex(ORIGIN.as_bytes()))
        .join("cache");
    let client = RepositoryClient::new(ORIGIN.into(), cache_root, false).unwrap();
    for (route, bytes) in [
        ("/repo/v1/root.json".to_string(), root_bytes),
        ("/repo/v1/snapshot.json".to_string(), snapshot_bytes),
        (index_route.to_string(), index_bytes),
        (
            "/repo/v1/packages/demo/1.0.0/metadata.json".to_string(),
            metadata_bytes,
        ),
        (
            "/repo/v1/packages/demo/1.0.0/archive.tar.zst".to_string(),
            archive_bytes,
        ),
    ] {
        let download = Download {
            etag: format!("\"{}\"", sha256_hex(&bytes)),
            bytes,
        };
        client.commit_verified(&route, &download).unwrap();
    }
    fs::remove_dir_all(source).unwrap();
    fs::remove_dir_all(sealed_root).unwrap();
}

fn envelope<T: Serialize>(route: &str, value: &T, seed: &str) -> Vec<u8> {
    let mut object = match serde_json::to_value(value).unwrap() {
        serde_json::Value::Object(object) => object,
        _ => unreachable!(),
    };
    object.remove("signed_digest");
    let unsigned = serde_json::to_vec(&object).unwrap();
    object.insert(
        "signed_digest".into(),
        serde_json::to_value(digest(&unsigned)).unwrap(),
    );
    let payload = serde_json::to_vec(&object).unwrap();
    let payload_sha = sha256_hex(&payload);
    let payload_base64 = STANDARD.encode(&payload);
    let input = format!(
        "terlan-registry-signed-resource-v1\n{ORIGIN}\n{route}\n{payload_sha}\n{payload_base64}"
    );
    let signature = crate::runtime::native::ed25519::sign(seed, &input).unwrap();
    serde_json::to_vec(&SignedResourceRecord {
        schema: "terlan-registry-signed-resource-v1".into(),
        origin: ORIGIN.into(),
        resource: route.into(),
        payload_base64,
        payload: Digest {
            algorithm: "sha256".into(),
            value: payload_sha,
        },
        signatures: vec![ResourceSignature {
            key_id: KEY_ID.into(),
            algorithm: "ed25519".into(),
            signature_base64: signature.signature_base64,
        }],
    })
    .unwrap()
}

fn digest(bytes: &[u8]) -> Digest {
    Digest {
        algorithm: "sha256".into(),
        value: sha256_hex(bytes),
    }
}

fn temp(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-registry-resolve-{label}-{}-{nonce}",
        std::process::id()
    ))
}
