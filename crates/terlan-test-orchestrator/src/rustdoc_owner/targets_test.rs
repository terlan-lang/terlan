use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use serde_json::json;

fn package(root: &Path) -> Value {
    std::fs::write(root.join("Cargo.toml"), "[package]\nname='library'\n").unwrap();
    std::fs::write(root.join("lib.rs"), "").unwrap();
    json!({"name":"library", "manifest_path":root.join("Cargo.toml"), "targets":[{
        "name":"some-library", "src_path":root.join("lib.rs"), "edition":"2024", "kind":["lib"], "doctest":true
    }]})
}

#[test]
fn inventory_selects_enabled_libraries_and_excludes_main_package() {
    let fixture = temporary_fixture("doc-target-selection");
    let value = package(&fixture.0);
    let selected = project(&[&value], &fixture.0).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].crate_name, "some_library");
    assert_eq!(selected[0].source, fixture.0.join("lib.rs"));
    let mut excluded = value.clone();
    excluded["name"] = json!("terlan");
    let mut disabled = value.clone();
    disabled["targets"][0]["doctest"] = json!(false);
    let mut binary = value.clone();
    binary["targets"][0]["kind"] = json!(["bin"]);
    assert_eq!(
        project(&[&excluded, &disabled, &binary, &value], &fixture.0).unwrap(),
        selected
    );
    assert!(project(&[], &fixture.0).unwrap().is_empty());
}

#[test]
fn inventory_rejects_missing_policy_duplicates_and_outside_sources() {
    let fixture = temporary_fixture("doc-target-invalid");
    let value = package(&fixture.0);
    assert!(project(&[&value, &value], &fixture.0).is_err());
    for (field, invalid) in [
        ("doctest", Value::Null),
        ("edition", json!("unknown")),
        ("name", json!("")),
        ("src_path", json!("lib.rs")),
        ("kind", Value::Null),
    ] {
        let mut changed = value.clone();
        changed["targets"][0][field] = invalid;
        assert!(project(&[&changed], &fixture.0).is_err(), "{field}");
    }
    let outside = temporary_fixture("doc-target-outside");
    let external = package(&outside.0);
    assert!(project(&[&external], &fixture.0).is_err());
    let mut missing = value;
    missing.as_object_mut().unwrap().remove("targets");
    assert!(project(&[&missing], &fixture.0).is_err());
}

#[test]
fn invocation_requires_exact_target_edition_and_unmodified_runner() {
    let fixture = temporary_fixture("doc-target-arguments");
    let value = package(&fixture.0);
    let mut selected = project(&[&value], &fixture.0).unwrap();
    let target = selected.pop().unwrap();
    let args = [
        "--edition=2024",
        "--crate-name",
        "some_library",
        "--test",
        "lib.rs",
    ]
    .map(OsString::from)
    .to_vec();
    assert!(target.matches(&args, &fixture.0));
    for additional in [
        "--test-runtool=custom",
        "--edition=2015",
        "--crate-name=wrong",
        "lib.rs",
    ] {
        let mut changed = args.clone();
        changed.push(additional.into());
        assert!(!target.matches(&changed, &fixture.0), "{additional}");
    }
    let mut missing = args.clone();
    missing.remove(3);
    assert!(!target.matches(&missing, &fixture.0));
    let mut old = target;
    old.edition = "2015".into();
    let mut args = args;
    args.remove(0);
    assert!(old.matches(&args, &fixture.0));
    args.extend(["--edition=2015".into(), "--edition=2015".into()]);
    assert!(!old.matches(&args, &fixture.0));
}
