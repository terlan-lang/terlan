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
