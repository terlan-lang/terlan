use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use serde_json::json;
use std::fs;
use std::time::Duration;
use terlan_process_owner::ProcessControl;

fn document(extra: &str) -> toml::Value {
    toml::from_str(&format!(
        "[package]\nname = \"pkg-name\"\nedition = \"2021\"\n{extra}"
    ))
    .unwrap()
}

fn target(kind: &str, name: &str, source: &Path) -> serde_json::Value {
    json!({"name": name, "kind": [kind], "src_path": source, "edition": "2021"})
}

#[test]
fn explicit_targets_keep_their_own_harness_policy_even_when_sharing_a_source() {
    let root = temporary_fixture("target-explicit");
    let source = root.0.join("shared.rs");
    fs::write(&source, "fixture").unwrap();
    for kind in ["bin", "test"] {
        let document = document(&format!("[[{kind}]]\nname = \"owned\"\npath = \"shared.rs\"\nharness = true\n[[{kind}]]\nname = \"custom\"\npath = \"shared.rs\"\nharness = false\n"));
        let owned = resolve(&document, &target(kind, "owned", &source), &root.0).unwrap();
        assert_eq!(owned.source, source);
        assert!(owned.explicit_harness);
        assert!(resolve(&document, &target(kind, "custom", &source), &root.0).is_err());
        assert!(resolve(&document, &target(kind, "shared", &source), &root.0).is_err());
    }
}

#[test]
fn target_conventions_reject_disabled_hidden_ambiguous_and_renamed_discovery() {
    let root = temporary_fixture("target-discovery");
    fs::create_dir_all(root.0.join("src/bin/nested")).unwrap();
    fs::create_dir_all(root.0.join("tests")).unwrap();
    fs::write(root.0.join("src/main.rs"), "fixture").unwrap();
    fs::write(root.0.join("src/bin/nested/main.rs"), "fixture").unwrap();
    fs::write(root.0.join("tests/check.rs"), "fixture").unwrap();
    let default = document("");
    for (kind, name, path) in [
        ("bin", "pkg-name", "src/main.rs"),
        ("bin", "nested", "src/bin/nested/main.rs"),
        ("test", "check", "tests/check.rs"),
    ] {
        let source = root.0.join(path);
        assert_eq!(
            resolve(&default, &target(kind, name, &source), &root.0)
                .unwrap()
                .source,
            source
        );
        let disabled = document(if kind == "bin" {
            "autobins = false"
        } else {
            "autotests = false"
        });
        assert!(resolve(&disabled, &target(kind, name, &source), &root.0).is_err());
    }
    fs::write(root.0.join("src/bin/nested.rs"), "fixture").unwrap();
    assert!(resolve(
        &default,
        &target("bin", "nested", &root.0.join("src/bin/nested.rs")),
        &root.0
    )
    .is_err());
    for name in [".hidden", "../outside", "a/b", "a\\b"] {
        assert!(resolve(
            &default,
            &target("test", name, &root.0.join("tests/check.rs")),
            &root.0
        )
        .is_err());
    }
    let renamed = document("[[test]]\nname = \"renamed\"\npath = \"tests/check.rs\"");
    assert!(resolve(
        &renamed,
        &target("test", "check", &root.0.join("tests/check.rs")),
        &root.0
    )
    .is_err());
}

#[test]
fn edition_and_target_declarations_do_not_silently_enable_missing_owners() {
    let root = temporary_fixture("target-edition");
    fs::create_dir(root.0.join("tests")).unwrap();
    fs::write(root.0.join("tests/inferred.rs"), "fixture").unwrap();
    let observed = target("test", "inferred", &root.0.join("tests/inferred.rs"));
    let modern = document("[[test]]\nname = \"declared\"\npath = \"declared.rs\"");
    assert!(resolve(&modern, &observed, &root.0).is_ok());
    let mut legacy = modern.clone();
    legacy["package"]["edition"] = toml::Value::String("2015".into());
    assert!(resolve(&legacy, &observed, &root.0).is_err());
    legacy["package"]
        .as_table_mut()
        .unwrap()
        .insert("autotests".into(), toml::Value::Boolean(true));
    assert!(resolve(&legacy, &observed, &root.0).is_ok());
    let mut inherited = modern.clone();
    inherited["package"]["edition"] = toml::Value::Table(
        [("workspace".into(), toml::Value::Boolean(true))]
            .into_iter()
            .collect(),
    );
    assert!(resolve(&inherited, &observed, &root.0).is_ok());
    let mut unknown = observed.clone();
    unknown["edition"] = serde_json::Value::Null;
    assert!(resolve(&inherited, &unknown, &root.0).is_err());
    for invalid in [
        "autotests = \"true\"",
        "[[test]]\npath = \"custom.rs\"",
        "[[test]]\nname = \"inferred\"\nharness = \"true\"",
        "[[test]]\nname = \"inferred\"\n[[test]]\nname = \"inferred\"",
    ] {
        assert!(
            resolve(&document(invalid), &observed, &root.0).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn real_cargo_library_binary_and_integration_artifacts_follow_declared_ownership() {
    let root = temporary_fixture("target-cargo-real");
    for directory in ["src/bin/nested", "tests/nested"] {
        fs::create_dir_all(root.0.join(directory)).unwrap();
    }
    fs::write(root.0.join("Cargo.toml"), "[package]\nname = \"target-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"library.rs\"\n[[bin]]\nname = \"explicit\"\npath = \"explicit.rs\"\nharness = true\n[[bin]]\nname = \"custom_bin\"\npath = \"custom_bin.rs\"\nharness = false\n[[test]]\nname = \"explicit_test\"\npath = \"explicit_test.rs\"\nharness = true\n[[test]]\nname = \"custom_test\"\npath = \"custom_test.rs\"\nharness = false\n[workspace]\n").unwrap();
    for path in [
        "library.rs",
        "src/main.rs",
        "src/bin/nested/main.rs",
        "tests/nested/main.rs",
        "tests/test-case.rs",
        "explicit.rs",
        "explicit_test.rs",
        "custom_bin.rs",
        "custom_test.rs",
    ] {
        fs::write(root.0.join(path), "fn main() { panic!(\"admission executed a program\"); }\n#[test] fn guarded() { panic!(\"admission executed a test\"); }\n").unwrap();
    }
    let control = ProcessControl::new(Duration::from_secs(30));
    let output = control
        .capture_stdout(
            std::process::Command::new("cargo")
                .current_dir(&root.0)
                .args([
                    "test",
                    "--offline",
                    "--tests",
                    "--no-run",
                    "--message-format=json",
                ])
                .env("CARGO_TARGET_DIR", root.0.join("target")),
            1024 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let artifacts = output
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|artifact| {
            artifact["reason"] == "compiler-artifact"
                && artifact["profile"]["test"] == true
                && artifact["executable"].is_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(artifacts.len(), 9);
    let mut admitted = 0;
    let mut rejected = 0;
    for artifact in artifacts {
        let outcome = crate::cargo_harness_admission::admit(&artifact, &root.0, control);
        if artifact["target"]["name"]
            .as_str()
            .unwrap()
            .starts_with("custom_")
        {
            assert!(outcome.unwrap_err().detail.contains("non-libtest owner"));
            rejected += 1;
        } else {
            let declaration = outcome.unwrap();
            assert_eq!(declaration.json()["target"], artifact["target"]["name"]);
            declaration.verify(control).unwrap();
            admitted += 1;
        }
    }
    assert_eq!((admitted, rejected), (7, 2));
}

#[cfg(unix)]
#[test]
fn automatic_nested_targets_do_not_follow_directory_symlinks() {
    let root = temporary_fixture("target-directory-alias");
    fs::create_dir(root.0.join("tests")).unwrap();
    fs::create_dir(root.0.join("actual")).unwrap();
    fs::write(root.0.join("actual/main.rs"), "fixture").unwrap();
    std::os::unix::fs::symlink(root.0.join("actual"), root.0.join("tests/alias")).unwrap();
    assert!(resolve(
        &document(""),
        &target("test", "alias", &root.0.join("tests/alias/main.rs")),
        &root.0
    )
    .is_err());
    let explicit = document("[[test]]\nname = \"alias\"\npath = \"tests/alias/main.rs\"");
    assert!(resolve(
        &explicit,
        &target("test", "alias", &root.0.join("tests/alias/main.rs")),
        &root.0
    )
    .is_ok());
}
