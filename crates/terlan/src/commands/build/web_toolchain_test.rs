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

#[cfg(unix)]
#[test]
fn managed_bundler_closes_stdin_preserves_diagnostics_and_rejects_excess_output() {
    let output = run_managed_bundler(Command::new("/bin/sh").args([
        "-c",
        "if read value; then exit 9; fi; printf bundled; printf diagnostic >&2; exit 7",
    ]))
    .expect("bounded completed bundler");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"bundled");
    assert_eq!(output.stderr, b"diagnostic");
    let error =
        run_managed_bundler(Command::new("/bin/sh").args(["-c", "head -c 16777217 /dev/zero"]))
            .expect_err("bundler output must be bounded");
    let cause = std::error::Error::source(&error)
        .expect("bundler retains its typed process error")
        .downcast_ref::<ToolCommandError>()
        .expect("process capture error source");
    assert_eq!(cause.code(), "output_limit_exceeded");
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("output_limit_exceeded"), "{diagnostic}");
    assert!(
        diagnostic.contains("managed browser bundler"),
        "{diagnostic}"
    );
}
