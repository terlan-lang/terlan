use super::*;
use crate::executable_binding::{resolve_program, ExecutableBinding};
use crate::source_inventory::{capture_with_git, SourceBinding};
use crate::test_orchestrator_test::temporary_fixture;
use crate::tool_configuration::ToolConfiguration;
use std::ffi::OsString;
use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn suite_admits_one_generation_and_rejects_changed_inputs_without_querying_cargo() {
    let fixture = temporary_fixture("suite-metadata");
    let root = &fixture.0;
    let control = ProcessControl::new(Duration::from_secs(30));
    control
        .run(
            Command::new("git").args(["init", "--quiet"]).arg(root),
            |_| Ok(()),
        )
        .unwrap();
    fs::write(root.join(".gitignore"), "/target\n").unwrap();
    fs::create_dir_all(root.join("target/cargo-home")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='member'\nversion='0.0.0'\n",
    )
    .unwrap();
    let source = root.join("cargo.rs");
    fs::write(&source, r#"
fn main() {
    assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(), ["metadata", "--locked", "--all-features", "--format-version", "1"]);
    let marker = "target/query";
    assert!(!std::path::Path::new(marker).exists(), "duplicate metadata query");
    std::fs::write(marker, "one query").unwrap();
    let root = std::env::current_dir().unwrap();
    println!("{{\"version\":1,\"workspace_root\":{:?},\"target_directory\":{:?},\"workspace_members\":[\"member-id\"],\"packages\":[{{\"id\":\"member-id\",\"name\":\"member\",\"targets\":[],\"manifest_path\":{:?}}}]}}", root, root.join("target"), root.join("Cargo.toml"));
}
"#).unwrap();
    let cargo = root.join("target/cargo");
    control
        .run(
            Command::new("rustc").arg(source).arg("-o").arg(&cargo),
            |_| Ok(()),
        )
        .unwrap();
    let environment = ExecutionEnvironment::from_entries(
        std::env::vars_os().chain([(
            OsString::from("CARGO_HOME"),
            root.join("target/cargo-home").into_os_string(),
        )]),
        root,
    )
    .unwrap();
    let output = root.join("target/quality/rust-cargo-metadata.json");
    super::super::produce(
        &output,
        &[cargo.clone().into_os_string()],
        &environment,
        control,
    )
    .unwrap();
    let path = environment.value("PATH").unwrap();
    let git = resolve_program("git", &path).unwrap();
    let rustup = resolve_program("rustup", &path).unwrap();
    let mut inputs = ValidationInputs {
        source: SourceBinding {
            before: Some(
                capture_with_git(root, &git, &environment, control, &mut |_| Ok(())).unwrap(),
            ),
            after: None,
        },
        configuration: ToolConfiguration::capture(&environment, control).unwrap(),
        executables: ExecutableBinding::capture(
            &[
                ("cargo", cargo.clone()),
                ("git", git.clone()),
                ("rustup", rustup),
            ],
            control,
        )
        .unwrap(),
        ..ValidationInputs::default()
    };
    let mut admitted = Handoff::admit(&inputs, &environment, control).unwrap();
    let document: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    let original_attempt: Value =
        serde_json::from_slice(&fs::read(output.with_extension("json.attempt.json")).unwrap())
            .unwrap();
    let digest = original_attempt["metadata_sha256"].as_str().unwrap();
    for (key, replacement) in [
        ("schema", json!("wrong")),
        ("state", json!("running")),
        ("run_id", json!("other")),
        ("reusable", json!(true)),
        ("metadata_sha256", json!("other")),
        ("launches", json!([])),
    ] {
        let mut invalid = original_attempt.clone();
        invalid[key] = replacement;
        assert!(
            admit_generation(&document, &invalid, root, digest).is_err(),
            "{key}"
        );
    }
    for key in ["source", "configuration", "executables", "resolver_cache"] {
        let mut invalid = document.clone();
        invalid["terlan_preparation"][key]["after"] = Value::Null;
        assert!(
            admit_generation(&invalid, &original_attempt, root, digest).is_err(),
            "{key}"
        );
    }
    assert!(admit_generation(
        &document,
        &original_attempt,
        Path::new("/another-workspace"),
        digest
    )
    .is_err());
    let packages = admitted.packages();
    Handoff::admit_harness(
        &packages,
        &json!({"package":"member", "manifest":root.join("Cargo.toml")}),
    )
    .unwrap();
    for declaration in [
        json!({"package":"outside", "manifest":root.join("Cargo.toml")}),
        json!({"package":"member", "manifest":root.join("another/Cargo.toml")}),
        json!({}),
    ] {
        assert!(Handoff::admit_harness(&packages, &declaration).is_err());
    }
    admitted.verify(control).unwrap();
    assert!(admitted.verify(control).is_err());
    assert_eq!(admitted.json()["resolver_cache"]["verified"], true);

    let mut command = environment.command(Path::new("not-executed"));
    command.env("TERLAN_RUST_SUITE_REPORT", "a-different-report");
    let report_environment = ExecutionEnvironment::from_entries(
        command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned()))),
        root,
    )
    .unwrap();
    Handoff::admit(&inputs, &report_environment, control).unwrap();
    command.env("CARGO_REGISTRIES_EXAMPLE_INDEX", "changed-resolution");
    let changed_environment = ExecutionEnvironment::from_entries(
        command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned()))),
        root,
    )
    .unwrap();
    assert!(Handoff::admit(&inputs, &changed_environment, control)
        .err()
        .expect("changed environment rejected")
        .detail
        .contains("environment"));

    let saved_source = inputs.source.before.clone();
    fs::write(root.join("new.rs"), "new source").unwrap();
    inputs.source.before =
        Some(capture_with_git(root, &git, &environment, control, &mut |_| Ok(())).unwrap());
    assert!(Handoff::admit(&inputs, &environment, control)
        .err()
        .expect("changed source rejected")
        .detail
        .contains("source"));
    inputs.source.before = saved_source;
    let mut pending_closeout = Handoff::admit(&inputs, &environment, control).unwrap();
    fs::create_dir_all(root.join("target/cargo-home/registry/index")).unwrap();
    fs::write(
        root.join("target/cargo-home/registry/index/config.json"),
        "changed index",
    )
    .unwrap();
    assert!(pending_closeout.verify(control).is_err());
    assert_eq!(pending_closeout.json()["resolver_cache"]["verified"], false);
    assert!(Handoff::admit(&inputs, &environment, control)
        .err()
        .expect("changed cache rejected")
        .detail
        .contains("resolver cache"));

    let mut attempt: Value =
        serde_json::from_slice(&fs::read(output.with_extension("json.attempt.json")).unwrap())
            .unwrap();
    attempt["state"] = json!("failed");
    fs::write(
        output.with_extension("json.attempt.json"),
        serde_json::to_vec(&attempt).unwrap(),
    )
    .unwrap();
    assert!(Handoff::admit(&inputs, &environment, control).is_err());
    assert_eq!(
        fs::read_to_string(root.join("target/query")).unwrap(),
        "one query"
    );
}

#[test]
fn workspace_projection_requires_exact_members_and_unambiguous_paths() {
    let root = Path::new("/workspace");
    let value = json!({"workspace_members":["member-id"], "packages":[{"id":"dep"}, {"id":"member-id", "name":"member", "manifest_path":"/workspace/Cargo.toml", "targets":[]}]});
    assert_eq!(workspace_packages(&value, root).unwrap().0.len(), 1);
    for path in [
        "Cargo.toml",
        "/outside/Cargo.toml",
        "/workspace/../outside/Cargo.toml",
        "/workspace/not-a-manifest",
    ] {
        let mut invalid = value.clone();
        invalid["packages"][1]["manifest_path"] = json!(path);
        assert!(workspace_packages(&invalid, root).is_err());
    }
    let mut invalid = value.clone();
    invalid["workspace_members"] = json!(["missing"]);
    assert!(workspace_packages(&invalid, root).is_err());
}
