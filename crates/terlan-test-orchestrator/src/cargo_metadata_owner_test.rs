//! Real and adversarial metadata production without invoking correctness tests.

use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::fs;
use std::process::Command;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(30))
}

fn fixture(root: &Path) {
    control()
        .run(
            Command::new("git").args(["init", "--quiet"]).arg(root),
            |_| Ok(()),
        )
        .unwrap();
    fs::write(root.join(".gitignore"), "/target\n").unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname=\"metadata_owner\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[lib]\npath=\"lib.rs\"\n").unwrap();
    fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname=\"metadata_owner\"\nversion=\"0.0.0\"\n",
    )
    .unwrap();
    fs::write(root.join("lib.rs"), "pub fn value() {}\n").unwrap();
}

fn environment(root: &Path, mode: &str) -> ExecutionEnvironment {
    let cargo_home = root.join("target/cargo-home");
    fs::create_dir_all(&cargo_home).unwrap();
    ExecutionEnvironment::from_entries(
        std::env::vars_os().chain([
            (OsString::from("METADATA_OWNER_MODE"), OsString::from(mode)),
            (OsString::from("CARGO_HOME"), cargo_home.into_os_string()),
        ]),
        root,
    )
    .unwrap()
}

#[test]
fn real_metadata_records_one_query_and_failed_lock_refresh_keeps_prior_output() {
    let fixture_root = temporary_fixture("metadata-owner-real");
    let root = &fixture_root.0;
    fixture(root);
    let output = root.join("target/metadata.json");
    let environment = environment(root, "normal");
    produce(&output, &[OsString::from("cargo")], &environment, control()).unwrap();
    let before = fs::read(&output).unwrap();
    let value: Value = serde_json::from_slice(&before).unwrap();
    let record = &value["terlan_preparation"];
    assert_eq!(record["reusable"], false);
    assert_eq!(record["source"]["verified"], true);
    assert_eq!(record["configuration"]["verified"], true);
    assert_eq!(record["executables"]["verified"], true);
    assert_eq!(record["resolver_cache"]["verified"], true);
    let launches = record["launches"].as_array().unwrap();
    let proxied = record["cargo_dispatch"]["kind"] == "rustup-proxy";
    if proxied {
        assert_eq!(launches[0]["role"], "rustup-cargo-resolution");
        assert_eq!(record["cargo_dispatch"]["native_cargo"]["verified"], true);
    }
    assert_eq!(
        launches
            .iter()
            .skip(usize::from(proxied))
            .map(|row| row["role"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["source-before", "cargo-metadata", "source-after"]
    );
    assert!(launches.iter().all(|row| row["pid"].as_u64().unwrap() > 0));
    assert_eq!(value["workspace_members"].as_array().unwrap().len(), 1);
    let manifest = root.join("Cargo.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("0.0.0", "0.0.1"),
    )
    .unwrap();
    assert!(produce(&output, &[OsString::from("cargo")], &environment, control()).is_err());
    assert_eq!(fs::read(&output).unwrap(), before);
    assert!(!root.join("target/metadata.json.pending").exists());
    let attempt: Value =
        serde_json::from_slice(&fs::read(root.join("target/metadata.json.attempt.json")).unwrap())
            .unwrap();
    assert_eq!(attempt["state"], "failed");
    assert_eq!(
        attempt["launches"].as_array().unwrap().len(),
        2 + usize::from(proxied)
    );
    assert_eq!(
        attempt["launches"][1 + usize::from(proxied)]["role"],
        "cargo-metadata"
    );
    assert_ne!(attempt["run_id"], record["run_id"]);
}

#[test]
fn metadata_admission_rejects_malformed_cross_workspace_and_duplicate_documents() {
    let root = Path::new("/workspace");
    let value = json!({"version":1,"workspace_root":"/workspace","workspace_members":["member"],
        "packages":[{"id":"dependency"},{"id":"member"}]});
    assert!(admit(&serde_json::to_vec(&value).unwrap(), root).is_ok());
    for (key, replacement) in [
        ("version", json!(2)),
        ("workspace_root", json!("/foreign")),
        ("workspace_members", json!([])),
        ("workspace_members", json!([""])),
        ("workspace_members", json!(["missing"])),
        ("workspace_members", json!(["member", "member"])),
        ("packages", json!([{"id":"member"},{"id":"member"}])),
        ("packages", json!([{"id":""}])),
        ("packages", json!([{"id":42}])),
        ("terlan_preparation", json!({"reusable":true})),
    ] {
        let mut invalid = value.clone();
        invalid[key] = replacement;
        assert!(
            admit(&serde_json::to_vec(&invalid).unwrap(), root).is_err(),
            "{invalid}"
        );
    }
    assert!(admit(b"", root).is_err());
    assert!(admit(b"{partial", root).is_err());
}

#[test]
fn producer_rejects_successful_empty_output_and_source_mutation_without_overwriting() {
    let fixture_root = temporary_fixture("metadata-owner-mutation");
    let root = &fixture_root.0;
    fixture(root);
    fs::create_dir(root.join("target")).unwrap();
    let script = root.join("cargo_fixture.rs");
    fs::write(&script, r#"
fn main() {
    assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(), ["metadata","--locked","--all-features","--format-version","1"]);
    let mode = std::env::var("METADATA_OWNER_MODE").unwrap();
    if mode == "empty" { return; }
    if mode == "mutate" { std::fs::write("lib.rs", "pub fn changed() {}\n").unwrap(); }
    if mode == "cache-mutate" {
        let path = std::path::PathBuf::from(std::env::var_os("CARGO_HOME").unwrap()).join("registry/index");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("config.json"), "changed registry input").unwrap();
    }
    println!("{}", std::fs::read_to_string("metadata.input.json").unwrap());
}
"#).unwrap();
    let cargo = root.join("target/cargo-fixture");
    control()
        .run(
            Command::new("rustc").arg(&script).arg("-o").arg(&cargo),
            |_| Ok(()),
        )
        .unwrap();
    fs::write(
        root.join("metadata.input.json"),
        serde_json::to_vec(&json!({"version":1,"workspace_root":root,
        "workspace_members":["member"],"packages":[{"id":"member"}]}))
        .unwrap(),
    )
    .unwrap();
    let output = root.join("target/metadata.json");
    for mode in ["normal", "empty", "cache-mutate", "mutate"] {
        let result = produce(
            &output,
            &[cargo.clone().into_os_string()],
            &environment(root, mode),
            control(),
        );
        assert_eq!(result.is_ok(), mode == "normal", "{mode}");
        if mode == "cache-mutate" {
            assert!(result
                .as_ref()
                .unwrap_err()
                .detail
                .contains("resolver cache changed"));
        }
        let document: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(document["terlan_preparation"]["source"]["verified"], true);
        if mode == "normal" {
            fs::write(root.join("target/prior"), fs::read(&output).unwrap()).unwrap();
        }
        assert_eq!(
            fs::read(&output).unwrap(),
            fs::read(root.join("target/prior")).unwrap()
        );
        assert!(!root.join("target/metadata.json.pending").exists());
        let attempt: Value = serde_json::from_slice(
            &fs::read(root.join("target/metadata.json.attempt.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            attempt["state"],
            if mode == "normal" {
                "succeeded"
            } else {
                "failed"
            }
        );
        assert_eq!(
            attempt["launches"].as_array().unwrap().len(),
            if mode == "empty" { 2 } else { 3 }
        );
        if mode == "mutate" {
            assert_eq!(
                attempt["error"]["detail"],
                "source inputs changed during Cargo metadata"
            );
        }
    }
}

#[test]
fn unfinished_metadata_attempt_retains_actual_launches_without_claiming_success() {
    let fixture_root = temporary_fixture("metadata-owner-unfinished");
    let output = fixture_root.0.join("metadata.json");
    {
        let mut attempt = MetadataAttempt::open(&output, "incomplete-run").unwrap();
        attempt
            .launched("cargo-metadata", std::process::id())
            .unwrap();
        assert!(attempt.finish(&Ok(()), None).is_err());
        // No completion call: model an unfinished observation, not SIGKILL cleanup.
    }
    let value: Value = serde_json::from_slice(
        &fs::read(fixture_root.0.join("metadata.json.attempt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(value["state"], "running");
    assert_eq!(value["run_id"], "incomplete-run");
    assert_eq!(value["reusable"], false);
    assert!(value["metadata_sha256"].is_null());
    assert_eq!(value["launches"][0]["pid"], std::process::id());
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn rustup_proxy_binds_native_cargo_and_rejects_changes_outside_the_source_inventory() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture_root = temporary_fixture("metadata-owner-proxy");
    let root = &fixture_root.0;
    fixture(root);
    fs::create_dir(root.join("target")).unwrap();
    let source = root.join("proxy.rs");
    fs::write(&source, r#"
fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let mode = std::env::var("METADATA_OWNER_MODE").unwrap();
    if arguments == ["which", "cargo"] {
        assert_eq!(std::env::var("RUSTUP_AUTO_INSTALL").unwrap(), "0");
        if mode == "bad-resolution" { println!("relative"); }
        else { println!("{}", std::env::current_dir().unwrap().join("target/native-cargo").display()); }
        return;
    }
    assert_eq!(arguments, ["metadata", "--locked", "--all-features", "--format-version", "1"]);
    if mode == "mutate-native" {
        std::fs::write("target/native-cargo", "changed native Cargo input").unwrap();
    }
    println!("{}", std::fs::read_to_string("metadata.input.json").unwrap());
}
"#).unwrap();
    let rustup = root.join("target/rustup");
    control()
        .run(
            Command::new("rustc").arg(source).arg("-o").arg(&rustup),
            |_| Ok(()),
        )
        .unwrap();
    let cargo = root.join("target/cargo");
    symlink(&rustup, &cargo).unwrap();
    let native = root.join("target/native-cargo");
    fs::write(&native, "original native Cargo input").unwrap();
    fs::set_permissions(&native, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        root.join("metadata.input.json"),
        serde_json::to_vec(&json!({"version":1,"workspace_root":root,
            "workspace_members":["member"],"packages":[{"id":"member"}]}))
        .unwrap(),
    )
    .unwrap();
    let output = root.join("target/metadata.json");
    let mut prior = Vec::new();
    for mode in ["normal", "mutate-native", "bad-resolution"] {
        let result = produce(
            &output,
            &[cargo.clone().into_os_string()],
            &environment(root, mode),
            control(),
        );
        assert_eq!(result.is_ok(), mode == "normal", "{mode}");
        if mode == "normal" {
            prior = fs::read(&output).unwrap();
        }
        assert_eq!(fs::read(&output).unwrap(), prior);
        let attempt: Value = serde_json::from_slice(
            &fs::read(root.join("target/metadata.json.attempt.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(attempt["launches"][0]["role"], "rustup-cargo-resolution");
        assert_eq!(
            attempt["launches"].as_array().unwrap().len(),
            if mode == "bad-resolution" { 1 } else { 4 }
        );
        if mode == "mutate-native" {
            assert_eq!(
                attempt["error"]["detail"],
                "selected executable inputs changed during execution"
            );
        }
    }
    let document: Value = serde_json::from_slice(&prior).unwrap();
    let dispatch = &document["terlan_preparation"]["cargo_dispatch"];
    assert_eq!(dispatch["kind"], "rustup-proxy");
    assert_eq!(dispatch["native_cargo"]["verified"], true);
    assert_eq!(
        dispatch["native_cargo"]["before"][0]["path"],
        native.to_str().unwrap()
    );
}
