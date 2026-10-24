use super::*;
use crate::{CliCommand, CliState};

#[test]
fn application_runtime_cannot_be_a_shell_wrapper() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("runtime");
    fs::write(&script, b"#!/bin/sh\nprintf '1\\n'\n").unwrap();
    assert!(verify_native_runtime(&script)
        .unwrap_err()
        .contains("native executable"));
}

#[test]
fn embedded_native_artifact_replaces_its_build_cache_path() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
    let output = directory.path().join("build");
    fs::create_dir_all(project.join("src/app")).unwrap();
    fs::write(project.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n").unwrap();
    fs::write(
        project.join("src/app/Main.terl"),
        r#"module app.Main.
import std.core.Option.{None, Some}.
import std.core.Result.{Err, Ok}.
import std.io.{Console, File}.
import std.system.Environment.
pub main(): Unit -> Console.println(case Environment.get("TERLAN_TEST_NATIVE_ARTIFACT") {
    Some(path) -> case File.read_text(path) {
        Ok(value) -> value;
        Err(_) -> "native artifact missing"
    };
    None -> "native binding missing"
}).
"#,
    )
    .unwrap();
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![project.display().to_string()],
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            }
        ),
        std::process::ExitCode::SUCCESS
    );
    let artifact = directory.path().join("external-native-artifact");
    fs::write(&artifact, b"embedded native artifact").unwrap();
    let executable = directory
        .path()
        .join(format!("standalone{}", std::env::consts::EXE_SUFFIX));
    write(
        &executable,
        &output,
        ApplicationEntry::Cli {
            image: "vm/app_Main.tvm".into(),
            function: "app.Main.main".into(),
        },
        &[("TERLAN_TEST_NATIVE_ARTIFACT".into(), artifact.clone())],
        false,
    )
    .unwrap();
    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(output).unwrap();
    fs::remove_file(&artifact).unwrap();
    let text = ProcessControl::new(Duration::from_secs(15))
        .capture_stdout(
            Command::new(&executable).env("TERLAN_TEST_NATIVE_ARTIFACT", &artifact),
            4096,
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(text, b"embedded native artifact\n");
}

#[test]
fn legacy_cleanup_preserves_runtimes_shared_with_another_program() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    seed_legacy_bundle(root);
    fs::write(root.join("bin/another-program"), b"another application").unwrap();
    LegacyOutputs::capture(root, "app", Path::new("vm/app_Main.tvm"))
        .remove(root, Path::new("bin/app"))
        .unwrap();
    assert_eq!(
        fs::read(root.join(format!("bin/terlan-vm{}", std::env::consts::EXE_SUFFIX))).unwrap(),
        b"legacy runtime"
    );
    assert_eq!(
        fs::read(root.join("vm/app_Main.tvm")).unwrap(),
        b"legacy image"
    );
    assert_eq!(
        fs::read(root.join("bin/another-program")).unwrap(),
        b"another application"
    );
}

#[cfg(unix)]
#[test]
fn legacy_cleanup_does_not_follow_symlinked_image_directories() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("build");
    seed_legacy_bundle(&root);
    let outside = directory.path().join("outside");
    fs::rename(root.join("vm"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("vm")).unwrap();
    LegacyOutputs::capture(&root, "app", Path::new("vm/app_Main.tvm"))
        .remove(&root, Path::new("bin/app"))
        .unwrap();
    assert_eq!(
        fs::read(outside.join("app_Main.tvm")).unwrap(),
        b"legacy image"
    );
    assert!(root
        .join("vm")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
}

fn seed_legacy_bundle(root: &Path) {
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("vm")).unwrap();
    let executable = format!("bin/app{}", if cfg!(windows) { ".cmd" } else { "" });
    // The names only need to agree with the recorded paths for cleanup admission.
    fs::write(root.join(&executable), b"legacy launcher").unwrap();
    fs::write(
        root.join(format!("bin/terlan-vm{}", std::env::consts::EXE_SUFFIX)),
        b"legacy runtime",
    )
    .unwrap();
    fs::write(root.join("vm/app_Main.tvm"), b"legacy image").unwrap();
    fs::write(
        root.join(super::super::BUILD_PACKAGE_METADATA_FILE),
        serde_json::to_vec(&serde_json::json!({
            "schema": "terlan-package-build-v1", "target": "terlan-vm", "artifact": "terlan-vm",
            "package": {"name": "app"}, "executable": {
                "path": executable, "runtime": format!("bin/terlan-vm{}", std::env::consts::EXE_SUFFIX), "image": "vm/app_Main.tvm"
            }
        }))
        .unwrap(),
    )
    .unwrap();
}
