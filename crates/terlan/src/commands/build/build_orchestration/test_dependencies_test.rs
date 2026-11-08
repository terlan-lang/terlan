use super::*;

fn missing_candidate(namespace: &str) -> TestNativeHelperCandidate {
    TestNativeHelperCandidate {
        namespace: Some(namespace.to_string()),
        helper_env: "TERLAN_UNUSED_NATIVE_HELPER".to_string(),
        helper: "unused-native-helper".to_string(),
        features: Vec::new(),
        crate_dir: std::env::temp_dir().join("terlan-unused-native-helper-must-not-build"),
    }
}

#[test]
fn unreachable_native_helpers_are_not_materialized() {
    let dependency = missing_candidate("unused.native");
    let reachable = std::collections::BTreeSet::from(["application.Main".to_string()]);
    assert_eq!(
        build_reachable_test_native_helpers(&[dependency], &reachable).unwrap(),
        Vec::<(String, PathBuf)>::new()
    );
}

#[test]
fn reachable_native_helpers_require_their_provider_manifest() {
    let dependency = missing_candidate("used.native");
    let reachable = std::collections::BTreeSet::from(["used.native.Frame".to_string()]);
    let error = build_reachable_test_native_helpers(&[dependency], &reachable)
        .expect_err("reachable native provider must be materialized");
    assert!(error.contains("manifest is missing"), "{error}");
}

#[test]
fn reachable_native_helpers_refresh_existing_binary_after_source_changes() {
    let fixture = tempfile::tempdir().expect("create native helper fixture");
    let crate_dir = fixture.path().join("native");
    std::fs::create_dir_all(crate_dir.join("src")).expect("create helper source directory");
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"refresh-native-helper\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"refresh-native-helper\"\npath = \"src/main.rs\"\n",
    )
    .expect("write helper manifest");
    let source = crate_dir.join("src/main.rs");
    std::fs::write(&source, "fn main() { println!(\"first\"); }\n")
        .expect("write first helper source");
    let dependency = TestNativeHelperCandidate {
        namespace: Some("refresh".to_string()),
        helper_env: "TERLAN_REFRESH_TEST_NATIVE_BOUNDARY_HELPER_PATH".to_string(),
        helper: "refresh-native-helper".to_string(),
        features: Vec::new(),
        crate_dir,
    };
    let reachable = std::collections::BTreeSet::from(["refresh.Frame".to_string()]);

    let first = build_reachable_test_native_helpers(std::slice::from_ref(&dependency), &reachable)
        .expect("build first helper");
    let helper = &first[0].1;
    let output = std::process::Command::new(helper)
        .output()
        .expect("run first helper");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "first\n");

    std::fs::write(&source, "fn main() { println!(\"second-version\"); }\n")
        .expect("write second helper source");
    let second = build_reachable_test_native_helpers(&[dependency], &reachable)
        .expect("refresh existing helper");
    let output = std::process::Command::new(&second[0].1)
        .output()
        .expect("run refreshed helper");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "second-version\n");
}
