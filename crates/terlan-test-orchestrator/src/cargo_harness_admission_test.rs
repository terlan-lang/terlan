use super::*;
use crate::test_orchestrator_test::{temporary_fixture, CargoFixture};
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(5))
}

fn fixture(lib: &str) -> (CargoFixture, Value) {
    let root = temporary_fixture("harness-declaration");
    fs::create_dir(root.0.join("src")).unwrap();
    fs::write(root.0.join("src/lib.rs"), "// fixture\n").unwrap();
    fs::write(
        root.0.join("Cargo.toml"),
        format!("[package]\nname = \"terlan\"\nversion = \"0.0.0\"\n{lib}"),
    )
    .unwrap();
    fs::write(root.0.join("harness"), "fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.0.join("harness"), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let artifact = serde_json::json!({"reason": "compiler-artifact", "package_id": "fixture", "target": {"name": "terlan", "kind": ["lib"], "src_path": root.0.join("src/lib.rs")}, "profile": {"test": true}, "executable": root.0.join("harness"), "manifest_path": root.0.join("Cargo.toml")});
    (root, artifact)
}

fn admit(root: &Path, artifact: &Value) -> Result<DeclaredHarness, PhaseFailure> {
    super::admit(artifact, root, control())
}

#[test]
fn cargo_library_admits_default_and_explicit_libtest_contracts() {
    for lib in [
        "",
        "[lib]\nharness = true\nname = \"terlan\"\npath = \"src/lib.rs\"\n",
    ] {
        let (root, artifact) = fixture(lib);
        let harness = admit(&root.0, &artifact).unwrap();
        assert_eq!(harness.executable, root.0.join("harness"));
        assert_eq!(
            harness.json()["harness_setting"],
            if lib.is_empty() {
                "cargo-default"
            } else {
                "explicit"
            }
        );
        harness.verify(control()).unwrap();
    }
}

#[test]
fn saved_library_declaration_requires_unchanged_manifest_target_and_source() {
    let (root, artifact) = fixture("");
    let saved = admit(&root.0, &artifact).unwrap().json();
    assert_eq!(
        DeclaredHarness::restore_library(&saved, &root.0, control())
            .unwrap()
            .json(),
        saved
    );
    for (key, value) in [
        ("package", serde_json::json!("other")),
        ("kind", serde_json::json!("bin")),
        ("target", serde_json::json!("other")),
        ("manifest_identity_sha256", serde_json::json!("forged")),
        ("harness", serde_json::json!(false)),
        ("source", serde_json::json!(root.0.join("absent.rs"))),
        (
            "resolved_source",
            serde_json::json!(root.0.join("absent.rs")),
        ),
    ] {
        let mut changed = saved.clone();
        changed[key] = value;
        assert!(
            DeclaredHarness::restore_library(&changed, &root.0, control()).is_err(),
            "{key}"
        );
    }
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname=\"terlan\"\nversion=\"0.0.1\"\n",
    )
    .unwrap();
    assert!(DeclaredHarness::restore_library(&saved, &root.0, control()).is_err());
}

#[test]
fn cargo_profile_test_cannot_admit_a_custom_or_malformed_library_contract() {
    for lib in [
        "[lib]\nharness = false",
        "[lib]\nharness = \"true\"",
        "[lib]\nname = \"other\"",
        "[lib]\npath = false",
        "[lib]\npath = \"missing.rs\"",
    ] {
        let (root, artifact) = fixture(lib);
        assert!(admit(&root.0, &artifact).is_err(), "{lib}");
    }
}

#[test]
fn cargo_library_rejects_missing_ambiguous_or_inconsistent_artifact_metadata() {
    let (root, artifact) = fixture("");
    let output = format!("{artifact}\n{artifact}\n");
    assert_eq!(
        select(output.as_bytes(), &root.0, control())
            .unwrap_err()
            .outcome,
        "cargo-artifact-stream-failed"
    );
    assert_eq!(
        select(b"{}\n", &root.0, control()).unwrap_err().outcome,
        "cargo-artifact-stream-failed"
    );
    for field in ["manifest_path", "executable"] {
        let mut invalid = artifact.clone();
        invalid[field] = Value::Null;
        assert!(admit(&root.0, &invalid).is_err());
    }
    let mut missing = artifact.clone();
    missing["executable"] = serde_json::json!(root.0.join("missing"));
    assert_eq!(
        admit(&root.0, &missing).unwrap_err().outcome,
        "artifact-missing"
    );
    let mut mismatch = artifact.clone();
    mismatch["target"]["src_path"] = serde_json::json!(root.0.join("Cargo.toml"));
    assert!(admit(&root.0, &mismatch).is_err());
}

#[test]
fn declared_manifest_mutation_rejects_next_harness_query_and_closeout() {
    let (root, artifact) = fixture("");
    let admission = admit(&root.0, &artifact).unwrap();
    let mut binding = crate::executable_binding::ExecutableBinding::capture(
        &[("fixture", admission.executable.clone())],
        control(),
    )
    .unwrap();
    binding.bind_declared_harness(admission, control()).unwrap();
    assert_eq!(binding.json()["declaration"]["harness"], true);
    binding.verify_harness(control()).unwrap();
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = \"terlan\"\n[lib]\nharness = false\n",
    )
    .unwrap();
    assert_eq!(
        binding.verify_harness(control()).unwrap_err().outcome,
        "harness-declaration-failed"
    );
    assert!(binding.verify(control()).is_err());
    assert!(!binding.verified());
}

#[test]
fn manifest_admission_is_bounded_and_rejects_outside_source_roots() {
    let (root, artifact) = fixture("");
    let outside = temporary_fixture("harness-outside");
    assert!(admit(&outside.0, &artifact).is_err());
    fs::OpenOptions::new()
        .write(true)
        .open(root.0.join("Cargo.toml"))
        .unwrap()
        .set_len(1024 * 1024 + 1)
        .unwrap();
    assert!(admit(&root.0, &artifact).is_err());
}

#[test]
fn manifest_batch_limits_do_not_charge_cached_packages_twice() {
    let (root, artifact) = fixture("");
    let mut admission = ManifestAdmission::new(&root.0).unwrap();
    admission.admit(&artifact, control()).unwrap();
    let size = admission.bytes;
    admission.admit(&artifact, control()).unwrap();
    assert_eq!(admission.manifests.len(), 1);
    assert_eq!(admission.bytes, size);
    let mut over_bytes = ManifestAdmission::new(&root.0).unwrap();
    over_bytes.bytes = 16 * 1024 * 1024;
    assert!(over_bytes.admit(&artifact, control()).is_err());
    assert!(over_bytes.manifests.is_empty());
    let mut over_count = ManifestAdmission::new(&root.0).unwrap();
    for index in 0..256 {
        over_count.manifests.insert(
            root.0.join(index.to_string()),
            ManifestSnapshot {
                document: toml::Value::Table(Default::default()),
                identity: String::new(),
            },
        );
    }
    assert!(over_count.admit(&artifact, control()).is_err());
    assert_eq!(over_count.manifests.len(), 256);
}

#[test]
fn cached_manifest_admission_does_not_bypass_cancellation() {
    use std::sync::atomic::AtomicBool;
    let (root, artifact) = fixture("");
    let mut admission = ManifestAdmission::new(&root.0).unwrap();
    admission.admit(&artifact, control()).unwrap();
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        admission
            .admit(&artifact, control().with_cancellation(&cancelled))
            .unwrap_err()
            .outcome,
        "cancelled"
    );
}

#[test]
fn real_cargo_custom_library_is_rejected_without_executing_inventory() {
    let (root, _) = fixture("[lib]\nharness = false");
    fs::write(
        root.0.join("src/lib.rs"),
        format!(
            "fn main() {{ std::fs::write({:?}, \"executed\").unwrap(); }}\n",
            root.0.join("unexpected-execution")
        ),
    )
    .unwrap();
    let output = ProcessControl::new(Duration::from_secs(30))
        .capture_stdout(
            std::process::Command::new("cargo")
                .current_dir(&root.0)
                .args([
                    "test",
                    "--offline",
                    "--lib",
                    "--no-run",
                    "--message-format=json",
                ])
                .env("CARGO_TARGET_DIR", root.0.join("target")),
            1024 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let error = select(&output, &root.0, control()).unwrap_err();
    assert_eq!(error.outcome, "harness-declaration-failed");
    assert!(error.detail.contains("custom Cargo lib harness"));
    assert!(!root.0.join("unexpected-execution").exists());
}

#[cfg(unix)]
#[test]
fn manifest_links_and_retargeted_source_aliases_cannot_change_the_declared_harness() {
    use std::os::unix::fs::symlink;
    let (root, artifact) = fixture("[lib]\npath = \"alias.rs\"");
    symlink(root.0.join("src/lib.rs"), root.0.join("alias.rs")).unwrap();
    let admitted = admit(&root.0, &artifact).unwrap();
    fs::write(root.0.join("other.rs"), "// replacement\n").unwrap();
    fs::remove_file(root.0.join("alias.rs")).unwrap();
    symlink(root.0.join("other.rs"), root.0.join("alias.rs")).unwrap();
    assert!(admitted.verify(control()).is_err());
    fs::rename(root.0.join("Cargo.toml"), root.0.join("actual.toml")).unwrap();
    symlink(root.0.join("actual.toml"), root.0.join("Cargo.toml")).unwrap();
    assert!(admit(&root.0, &artifact).is_err());
}
