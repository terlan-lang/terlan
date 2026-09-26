use super::*;
use crate::support::test_fs::TestDirectory;
use std::os::unix::fs::{symlink, PermissionsExt};

struct Fixture {
    root: TestDirectory,
    environment: ProofEnvironment,
    contract: ToolchainContract,
}

impl Fixture {
    fn new() -> Self {
        let root = TestDirectory::new("proof_admission", "handoff");
        for directory in [
            "proxies",
            "toolchain/bin",
            "proofs/lean",
            "target/quality/preparation/admission.work",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("proofs/lean/lean-toolchain"),
            "leanprover/lean4:v4.31.0\n",
        )
        .unwrap();
        fs::write(
            root.join("proofs/lean/lakefile.lean"),
            "import Lake\nopen Lake DSL\npackage probe\n",
        )
        .unwrap();
        fs::write(root.join("proofs/lean/lake-manifest.json"), "{}\n").unwrap();
        for (relative, source) in [
            ("proxies/elan", format!("#!/bin/sh\necho probe >> '{}/calls'\nprintf '%s\\n' '{}/toolchain/bin/'\"$2\"\n", root.display(), root.display())),
            ("toolchain/bin/lean", format!("#!/bin/sh\necho probe >> '{}/calls'\necho 'Lean (version 4.31.0, fixture)'\n", root.display())),
            ("toolchain/bin/lake", format!("#!/bin/sh\necho probe >> '{}/calls'\necho 'Lean (version 4.31.0, fixture)'\n", root.display())),
        ] {
            let path = root.join(relative);
            fs::write(&path, source).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let environment = ProofEnvironment::from_lookup(|key| match key {
            "PATH" => Some(root.join("proxies").into_os_string()),
            "ELAN_HOME" => Some(root.as_os_str().to_os_string()),
            _ => None,
        })
        .unwrap();
        Self {
            root,
            environment,
            contract: ToolchainContract {
                lean_version: "4.31.0".into(),
                elan_channel: "leanprover/lean4:v4.31.0".into(),
                lake_flags: vec!["env".into(), "lean".into()],
            },
        }
    }

    fn path(&self) -> PathBuf {
        self.root
            .join("target/quality/preparation/admission.work/tools.json")
    }

    fn document(&self) -> Document {
        let tools =
            tools::ProofTools::capture(&self.root, &self.environment, &self.contract).unwrap();
        Document {
            schema: SCHEMA.into(),
            root: fs::canonicalize(&self.root).unwrap(),
            producer: std::env::current_exe().unwrap(),
            utc_date: utc_date(),
            environment: self.environment.identity().unwrap(),
            tools: vec![Entry {
                contract: self.contract.clone(),
                tool: tools.admission(),
            }],
        }
    }

    fn write(&self, document: &Document) -> String {
        let bytes = serde_json::to_vec(document).unwrap();
        fs::write(self.path(), &bytes).unwrap();
        format_digest(&Sha256::digest(bytes))
    }

    fn load(&self, digest: &str) -> QualityResult<BTreeMap<ToolchainContract, tools::ProofTools>> {
        load(
            &self.root,
            &self.path(),
            digest,
            &self.environment,
            &BTreeSet::from([self.contract.clone()]),
        )
    }
}

#[test]
fn lean_proof_admission_rechecks_bytes_without_launching_tool_probes() {
    let fixture = Fixture::new();
    let document = fixture.document();
    let digest = fixture.write(&document);
    let before = fs::read(fixture.root.join("calls")).unwrap();
    let admitted = fixture.load(&digest).unwrap();
    assert_eq!(admitted.len(), 1);
    assert_eq!(fs::read(fixture.root.join("calls")).unwrap(), before);
    let pin = fixture.root.join("proofs/lean/lean-toolchain");
    let original_pin = fs::read(&pin).unwrap();
    fs::write(&pin, "leanprover/lean4:changed\n").unwrap();
    assert!(fixture.load(&digest).is_err());
    fs::write(&pin, original_pin).unwrap();
    let repeated = fixture.document();
    assert_eq!(
        serde_json::to_vec(&document).unwrap(),
        serde_json::to_vec(&repeated).unwrap()
    );
    fs::write(
        fixture.root.join("toolchain/bin/lean"),
        b"changed executable",
    )
    .unwrap();
    assert!(fixture.load(&digest).is_err());
}

#[test]
fn lean_proof_admission_rejects_incomplete_and_malformed_assignments() {
    assert!(assignment([None, None]).unwrap().is_none());
    let digest = format_digest(&[0; 32]);
    for values in [
        [Some("/tmp/tools".into()), None],
        [None, Some(digest.clone().into())],
        [Some("".into()), Some(digest.clone().into())],
        [Some("/tmp/tools".into()), Some("bad".into())],
        [
            Some("/tmp/tools".into()),
            Some(format!("sha256:{}", "A".repeat(64)).into()),
        ],
    ] {
        assert!(assignment(values).is_err());
    }
    assert!(assignment([Some("/tmp/tools".into()), Some(digest.into())])
        .unwrap()
        .is_some());
}

#[test]
fn lean_proof_admission_rejects_changed_context_contracts_and_digest() {
    let fixture = Fixture::new();
    let original = serde_json::to_vec(&fixture.document()).unwrap();
    for mutation in [
        "schema",
        "root",
        "producer",
        "date",
        "environment",
        "missing",
        "duplicate",
        "contract",
    ] {
        let mut document: Document = serde_json::from_slice(&original).unwrap();
        match mutation {
            "schema" => document.schema.push('x'),
            "root" => document.root.push("elsewhere"),
            "producer" => document.producer.push("elsewhere"),
            "date" => document.utc_date = "2000-01-01".into(),
            "environment" => document.environment.push('x'),
            "missing" => document.tools.clear(),
            "duplicate" => {
                let entry = serde_json::from_slice::<Document>(&original)
                    .unwrap()
                    .tools
                    .remove(0);
                document.tools.push(entry);
            }
            _ => document.tools[0].contract.lean_version = "other".into(),
        }
        assert!(
            fixture.load(&fixture.write(&document)).is_err(),
            "{mutation}"
        );
    }
    let original: Document = serde_json::from_slice(&original).unwrap();
    fixture.write(&original);
    assert!(fixture.load(&format_digest(&[0; 32])).is_err());
}

#[test]
fn lean_proof_admission_rejects_unsafe_paths_symlinks_and_oversized_documents() {
    let fixture = Fixture::new();
    for path in [
        fixture.root.join("tools.json"),
        fixture
            .root
            .join("target/quality/preparation/admission.work/../escape.json"),
    ] {
        assert!(private_path(&fixture.root, &path).is_err());
    }
    let alias = fixture.root.join("target/quality/preparation/alias.work");
    symlink(fixture.path().parent().unwrap(), &alias).unwrap();
    assert!(private_path(&fixture.root, &alias.join("tools.json")).is_err());
    let digest = fixture.write(&fixture.document());
    fs::remove_file(fixture.path()).unwrap();
    symlink(fixture.root.join("calls"), fixture.path()).unwrap();
    assert!(fixture.load(&digest).is_err());
    fs::remove_file(fixture.path()).unwrap();
    File::create(fixture.path())
        .unwrap()
        .set_len(MAX_DOCUMENT + 1)
        .unwrap();
    assert!(fixture.load(&digest).is_err());
}

#[test]
fn lean_proof_cached_consumer_never_executes_missing_or_changed_replicas() {
    use super::super::{cached, dependency_set_hash, sha256_file, OutputSignature, ReplayMetadata};

    let fixture = Fixture::new();
    let proof = "proofs/lean/A.lean";
    fs::write(fixture.root.join(proof), "theorem a : True := by trivial\n").unwrap();
    let metadata = ReplayMetadata {
        schema: "terlan.lean-proof-replay.v1".into(),
        family: "fixture".into(),
        theorem_names: vec!["a".into()],
        manifest_fingerprints: BTreeMap::new(),
        dependency_files: vec![proof.into()],
        proof_dependency_set_hash: dependency_set_hash(&fixture.root, &[proof.into()]).unwrap(),
        source_digest: sha256_file(&fixture.root.join(proof)).unwrap(),
        execution_command: vec!["lake".into(), "env".into(), "lean".into(), "A.lean".into()],
        working_directory: "proofs/lean".into(),
        deterministic_timestamp_strategy: "none-content-addressed".into(),
        output_signature: OutputSignature {
            stdout_class: "text".into(),
            stderr_class: "empty".into(),
            exit_class: "success".into(),
        },
        toolchain: fixture.contract.clone(),
    };
    let artifact = ArtifactRow {
        path: proof.into(),
        status: "current".into(),
        theorem_scope: "CoreIR".into(),
        targeted_manifests: vec![],
        expected_exit: 0,
        stderr_class: "none".into(),
        proof_digest: metadata.source_digest.clone(),
        replay_metadata: "unused.json".into(),
        remediation_plan: "none".into(),
    };
    let document = fixture.document();
    let tools = fixture.load(&fixture.write(&document)).unwrap();
    let tool = &tools[&fixture.contract];
    let mut cache = cached::open(&fixture.root, &[(&artifact, metadata.clone())], &tools).unwrap();
    let calls = fixture.root.join("calls");
    let before = fs::read(&calls).unwrap();
    assert!(cached::execute(&fixture.root, &metadata, 0, None, tool, &mut cache, 1).is_err());
    assert_eq!(fs::read(&calls).unwrap(), before);
    for replica in [1, 2] {
        cached::execute(
            &fixture.root,
            &metadata,
            0,
            Some(&fixture.environment),
            tool,
            &mut cache,
            replica,
        )
        .unwrap();
    }
    assert_eq!(cache.completed, 2);
    let completed = fs::read(&calls).unwrap();
    for replica in [1, 2] {
        cached::execute(&fixture.root, &metadata, 0, None, tool, &mut cache, replica).unwrap();
    }
    assert_eq!(cache.reused, 2);
    assert_eq!(fs::read(&calls).unwrap(), completed);
    drop(cache);

    fs::write(fixture.root.join(proof), "theorem b : True := by trivial\n").unwrap();
    let mut changed = metadata;
    changed.source_digest = sha256_file(&fixture.root.join(proof)).unwrap();
    changed.proof_dependency_set_hash =
        dependency_set_hash(&fixture.root, &[proof.into()]).unwrap();
    let mut cache = cached::open(&fixture.root, &[(&artifact, changed.clone())], &tools).unwrap();
    assert!(cached::execute(&fixture.root, &changed, 0, None, tool, &mut cache, 1).is_err());
    assert_eq!((cache.completed, cache.reused), (0, 0));
    assert_eq!(fs::read(&calls).unwrap(), completed);
}
