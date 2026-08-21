use super::*;
use crate::support::test_fs;

#[test]
fn docs_runtime_uses_exact_managed_angular_dependency() {
    let root = test_fs::temp_dir("docs_artifacts", "angular_runtime");
    let source = root.join("src/site/Site.terl");
    fs::create_dir_all(source.parent().expect("source parent")).expect("create source root");
    fs::write(&source, "module site.\n").expect("write source");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"docs\"\nversion = \"0.0.1\"\n\n[target.js.dependencies]\nangular_ts = { npm = \"@angular-wave/angular.ts\", version = \"0.32.0\" }\n",
    )
    .expect("write manifest");

    assert_eq!(
        resolve_docs_browser_runtime(&source).expect("resolve runtime"),
        DocsBrowserRuntime::ManagedAngularTs
    );
}

#[test]
fn docs_runtime_rejects_angular_version_drift() {
    let root = test_fs::temp_dir("docs_artifacts", "angular_drift");
    let source = root.join("src/Site.terl");
    fs::create_dir_all(source.parent().expect("source parent")).expect("create source root");
    fs::write(&source, "module site.\n").expect("write source");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"docs\"\nversion = \"0.0.1\"\n\n[target.js.dependencies]\nangular_ts = { npm = \"@angular-wave/angular.ts\", version = \"0.31.0\" }\n",
    )
    .expect("write manifest");

    let error = resolve_docs_browser_runtime(&source).expect_err("version drift must fail");
    assert!(error.contains("web_toolchain_drift"));
    assert!(error.contains("0.32.0"));
}
