use super::*;
use crate::support::test_fs;

fn write_package(root: &Path, package: &str, version: &str) {
    let package_dir = root.join("node_modules").join(package);
    fs::create_dir_all(&package_dir).expect("create package directory");
    fs::write(
        package_dir.join("package.json"),
        format!("{{\"version\":\"{version}\"}}"),
    )
    .expect("write package metadata");
}

#[test]
fn managed_web_toolchain_rejects_package_version_drift() {
    let root = test_fs::temp_dir("web_toolchain", "version_drift");
    write_package(&root, ANGULAR_TS_PACKAGE, "0.31.0");
    write_package(&root, RSBUILD_PACKAGE, RSBUILD_VERSION);
    write_package(&root, RSPACK_PACKAGE, RSPACK_VERSION);
    let error = validate_managed_web_toolchain(&root).expect_err("drift must fail");
    assert!(error.contains(ANGULAR_TS_VERSION));
    assert!(error.contains("0.31.0"));
}

#[test]
fn exact_angular_ts_dependency_is_managed() {
    assert!(is_managed_js_dependency(
        ANGULAR_TS_PACKAGE,
        ANGULAR_TS_VERSION
    ));
    assert!(!is_managed_js_dependency(ANGULAR_TS_PACKAGE, "latest"));
    assert!(!is_managed_js_dependency("angular", ANGULAR_TS_VERSION));
}
