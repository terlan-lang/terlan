use super::*;

fn prepare_fake_build_output(root: &Path) {
    fs::create_dir_all(root.join("bin")).expect("create fake bin output");
    fs::create_dir_all(root.join("vm")).expect("create fake vm output");
    fs::create_dir_all(root.join("web/.terlan/serve-aot/runtime"))
        .expect("create fake service web output");
    fs::write(root.join("bin/terlan-registry"), b"launcher\n").expect("write launcher");
    fs::write(root.join("bin/terlan-vm"), b"runtime\n").expect("write runtime");
    fs::write(root.join("bin/terlan-native-worker"), b"worker\n").expect("write worker");
    fs::write(root.join("bin/terlan-serve-runtime"), b"service runtime\n")
        .expect("write service runtime");
    fs::write(root.join("vm/terlan_registry_Main.tvm"), b"image\n").expect("write image");
    fs::write(
        root.join("web/manifest.json"),
        b"{\"schema\":\"terlan-browser-package-v2\"}\n",
    )
    .expect("write service manifest");
    fs::write(
        root.join("web/.terlan/serve-aot/runtime/active.json"),
        b"{\"generation\":\"one\"}\n",
    )
    .expect("write service runtime metadata");
    fs::write(
        root.join(BUILD_PACKAGE_METADATA_FILE),
        r#"{
  "executable": {
    "path": "bin/terlan-registry",
    "image": "vm/terlan_registry_Main.tvm",
    "runtime": "bin/terlan-vm",
    "native_worker": "bin/terlan-native-worker",
    "service_runtime": "bin/terlan-serve-runtime",
    "web_root": "web"
  }
}
"#,
    )
    .expect("write package metadata");
}

fn release_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/release-bundle-v1")
}

#[test]
fn release_bundle_is_complete_portable_and_deterministic() {
    let root = make_temp_dir("release_bundle_determinism");
    let first_output = root.join("first");
    let second_output = root.join("second");
    prepare_fake_build_output(&first_output);
    prepare_fake_build_output(&second_output);
    let fixture = release_fixture();
    let manifest = project_manifest::read_project_manifest(&fixture.join("terlan.toml"))
        .expect("parse release fixture");

    let first_state = CliState {
        out_dir: first_output.clone(),
        ..CliState::default()
    };
    let second_state = CliState {
        out_dir: second_output.clone(),
        ..CliState::default()
    };
    let first = release_bundle::write_release_bundle(&fixture, &manifest, &first_state)
        .expect("write first bundle");
    let second = release_bundle::write_release_bundle(&fixture, &manifest, &second_state)
        .expect("write second bundle");

    for required in [
        "manifest.json",
        "checksums.json",
        "deploy-plan.json",
        "health.json",
        "runtime.json",
        "routes.json",
        "capabilities.json",
        "sources.json",
        "artifact/bin/terlan-registry",
        "artifact/bin/terlan-serve-runtime",
        "artifact/bin/terlan-vm",
        "artifact/bin/terlan-native-worker",
        "artifact/vm/terlan_registry_Main.tvm",
        "artifact/web/manifest.json",
        "artifact/web/.terlan/serve-aot/runtime/active.json",
    ] {
        assert!(first.join(required).is_file(), "missing {required}");
    }

    let first_files =
        release_bundle::collect_file_identities(&first, None).expect("fingerprint first bundle");
    let second_files =
        release_bundle::collect_file_identities(&second, None).expect("fingerprint second bundle");
    assert_eq!(
        serde_json::to_value(first_files).expect("serialize first identities"),
        serde_json::to_value(second_files).expect("serialize second identities")
    );

    let manifest_text = fs::read_to_string(first.join("manifest.json")).expect("read manifest");
    assert!(!manifest_text.contains(&root.display().to_string()));
    assert!(!manifest_text.contains("postgres://"));
}
