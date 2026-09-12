use super::*;
use crate::execution_environment::ExecutionEnvironment;
use crate::test_orchestrator_test::temporary_fixture;
use std::fs;
use std::process::Command;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(10))
}

fn package(root: &Path) -> Value {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='example'\nversion='0.0.0'\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn example() {}\n").unwrap();
    json!({"manifest_path":root.join("Cargo.toml"), "targets":[{"src_path":root.join("src/lib.rs")}]})
}

fn inputs(root: &Path) -> (SourceSnapshot, resolver_cache::Binding) {
    fs::create_dir_all(root.join("target/cargo-home")).unwrap();
    control()
        .run(
            Command::new("git").args(["init", "--quiet"]).arg(root),
            |_| Ok(()),
        )
        .unwrap();
    let environment = ExecutionEnvironment::from_entries(
        std::env::vars_os().chain([(
            "CARGO_HOME".into(),
            root.join("target/cargo-home").into_os_string(),
        )]),
        root,
    )
    .unwrap();
    let source = crate::source_inventory::capture_with_git(
        root,
        Path::new("git"),
        &environment,
        control(),
        &mut |_| Ok(()),
    )
    .unwrap();
    let cache = resolver_cache::Binding::capture(&environment, control()).unwrap();
    (source, cache)
}

#[test]
fn ignored_package_sources_are_bound_without_rehashing_git_owned_files() {
    let fixture = temporary_fixture("package-source-ignored");
    fs::write(fixture.0.join(".gitignore"), "local/ignored.rs\ntarget/\n").unwrap();
    let local = package(&fixture.0.join("local"));
    fs::write(fixture.0.join("local/ignored.rs"), "hidden input").unwrap();
    let document = json!({"target_directory":fixture.0.join("target"), "packages":[local]});
    let (source, cache) = inputs(&fixture.0);
    let mut binding = Binding::capture(&document, &fixture.0, &source, &cache, control()).unwrap();
    assert_eq!(binding.json()["before"]["hashed_files"], 1);
    assert_eq!(
        binding.json()["before"]["content_bytes"],
        "hidden input".len()
    );
    binding.verify(control()).unwrap();
    assert_eq!(binding.json()["verified"], true);
    assert!(binding.verify(control()).is_err());
    let mut changed = Binding::capture(&document, &fixture.0, &source, &cache, control()).unwrap();
    fs::write(fixture.0.join("local/ignored.rs"), "changed input").unwrap();
    assert!(changed
        .verify(control())
        .unwrap_err()
        .detail
        .contains("sources changed"));
}

#[test]
fn external_dependency_contents_and_file_discovery_invalidate_closeout() {
    let fixture = temporary_fixture("package-source-workspace");
    let external = temporary_fixture("package-source-external");
    fs::write(fixture.0.join(".gitignore"), "target/\n").unwrap();
    let dependency = package(&external.0);
    let document = json!({"target_directory":fixture.0.join("target"), "packages":[dependency]});
    let (source, cache) = inputs(&fixture.0);
    for mode in ["edit", "new", "delete"] {
        let mut binding =
            Binding::capture(&document, &fixture.0, &source, &cache, control()).unwrap();
        match mode {
            "edit" => fs::write(external.0.join("src/lib.rs"), "pub fn changed() {}\n").unwrap(),
            "new" => fs::write(external.0.join("src/extra.rs"), "new target input").unwrap(),
            "delete" => fs::remove_file(external.0.join("src/extra.rs")).unwrap(),
            _ => unreachable!(),
        }
        assert!(binding.verify(control()).is_err(), "{mode}");
        assert_eq!(binding.json()["verified"], false);
    }
}

#[test]
fn build_outputs_are_excluded_but_cannot_be_declared_rust_sources() {
    let fixture = temporary_fixture("package-source-outputs");
    fs::write(fixture.0.join(".gitignore"), "target/\n.terlan/\n").unwrap();
    let local = package(&fixture.0);
    let mut document = json!({"target_directory":fixture.0.join("target"), "packages":[local]});
    let (source, cache) = inputs(&fixture.0);
    let mut binding = Binding::capture(&document, &fixture.0, &source, &cache, control()).unwrap();
    fs::create_dir_all(fixture.0.join("target")).unwrap();
    fs::write(fixture.0.join("target/output.rs"), "generated output").unwrap();
    fs::create_dir_all(fixture.0.join(".terlan")).unwrap();
    fs::write(fixture.0.join(".terlan/output"), "generated output").unwrap();
    binding.verify(control()).unwrap();
    assert_eq!(binding.json()["before"]["content_bytes"], 0);
    document["packages"][0]["targets"][0]["src_path"] = json!(fixture.0.join("target/output.rs"));
    assert!(Binding::capture(&document, &fixture.0, &source, &cache, control()).is_err());
}

#[cfg(unix)]
#[test]
fn source_file_links_bind_the_resolved_bytes_and_retargeting() {
    use std::os::unix::fs::symlink;
    let fixture = temporary_fixture("package-source-links");
    let external = temporary_fixture("package-source-linked-external");
    fs::write(fixture.0.join(".gitignore"), "local/linked.rs\ntarget/\n").unwrap();
    let local = package(&fixture.0.join("local"));
    let target = external.0.join("input.rs");
    fs::write(&target, "linked source").unwrap();
    symlink(&target, fixture.0.join("local/linked.rs")).unwrap();
    let document = json!({"target_directory":fixture.0.join("target"), "packages":[local]});
    let (source, cache) = inputs(&fixture.0);
    let mut binding = Binding::capture(&document, &fixture.0, &source, &cache, control()).unwrap();
    fs::write(target, "changed linked source").unwrap();
    assert!(binding.verify(control()).is_err());
}

#[test]
fn production_metadata_covers_optional_path_dependency_sources_without_building() {
    let fixture = temporary_fixture("package-source-optional");
    fs::write(fixture.0.join(".gitignore"), "target/\nvendor/\n").unwrap();
    package(&fixture.0);
    package(&fixture.0.join("vendor/optional"));
    fs::write(
        fixture.0.join("vendor/optional/Cargo.toml"),
        "[package]\nname='optional-dependency'\nversion='0.0.0'\n",
    )
    .unwrap();
    fs::write(fixture.0.join("Cargo.toml"), "[package]\nname='example'\nversion='0.0.0'\n[dependencies]\noptional-dependency={path='vendor/optional', optional=true}\n").unwrap();
    fs::write(fixture.0.join("Cargo.lock"), "version = 4\n[[package]]\nname='example'\nversion='0.0.0'\ndependencies=['optional-dependency']\n[[package]]\nname='optional-dependency'\nversion='0.0.0'\n").unwrap();
    let (source, cache) = inputs(&fixture.0);
    let environment = ExecutionEnvironment::from_entries(
        std::env::vars_os().chain([(
            "CARGO_HOME".into(),
            fixture.0.join("target/cargo-home").into_os_string(),
        )]),
        &fixture.0,
    )
    .unwrap();
    let output = fixture.0.join("target/metadata.json");
    super::super::produce(&output, &["cargo".into()], &environment, control()).unwrap();
    let document: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(
        document["terlan_preparation"]["query"],
        json!([
            "metadata",
            "--locked",
            "--all-features",
            "--format-version",
            "1"
        ])
    );
    assert_eq!(document["packages"].as_array().unwrap().len(), 2);
    let mut binding = Binding::capture(&document, &fixture.0, &source, &cache, control()).unwrap();
    // The containing package root also covers this dependency; no second tree walk.
    assert_eq!(binding.json()["roots"], 1);
    assert_eq!(binding.json()["before"]["hashed_files"], 2);
    fs::write(
        fixture.0.join("vendor/optional/src/lib.rs"),
        "pub fn changed_dependency() {}\n",
    )
    .unwrap();
    assert!(binding.verify(control()).is_err());
}

#[test]
fn package_source_admission_respects_cancellation_before_filesystem_discovery() {
    let fixture = temporary_fixture("package-source-cancel");
    fs::write(fixture.0.join(".gitignore"), "target/\n").unwrap();
    let local = package(&fixture.0);
    let (source, cache) = inputs(&fixture.0);
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let document = json!({"target_directory":fixture.0.join("target"), "packages":[local]});
    assert!(Binding::capture(
        &document,
        &fixture.0,
        &source,
        &cache,
        control().with_cancellation(&cancelled)
    )
    .is_err());
}
