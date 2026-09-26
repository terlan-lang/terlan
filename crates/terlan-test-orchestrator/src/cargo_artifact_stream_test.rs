use super::*;
use crate::test_orchestrator_test::{temporary_fixture, CargoFixture};
use std::fs;
use std::time::Duration;

const FINISHED: &[u8] = b"{\"reason\":\"build-finished\",\"success\":true}\n";

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(5))
}

fn fixture() -> (CargoFixture, Value) {
    let root = temporary_fixture("cargo-artifact-stream");
    fs::create_dir_all(root.0.join("src/bin")).unwrap();
    fs::write(root.0.join("src/lib.rs"), "// library").unwrap();
    fs::write(root.0.join("src/bin/tool.rs"), "// binary").unwrap();
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = \"terlan\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(root.0.join("library"), "fixture executable").unwrap();
    fs::write(root.0.join("tool"), "fixture executable").unwrap();
    let artifact = serde_json::json!({"reason":"compiler-artifact", "package_id":"fixture", "manifest_path":root.0.join("Cargo.toml"), "target":{"name":"terlan", "kind":["lib"], "src_path":root.0.join("src/lib.rs")}, "profile":{"test":true}, "executable":root.0.join("library")});
    (root, artifact)
}

fn record(artifact: &Value) -> Vec<u8> {
    format!("{artifact}\n").into_bytes()
}

#[test]
fn workspace_phase_rejects_a_recompiled_or_unproven_main_library_unit() {
    let (root, artifact) = fixture();
    for fresh in [
        Value::Null,
        serde_json::json!(false),
        serde_json::json!(true),
    ] {
        let mut message = artifact.clone();
        message["fresh"] = fresh.clone();
        let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
        stream.require_fresh_main();
        let result = stream.observe(&record(&message), control(), |_| Ok(()));
        assert_eq!(result.is_ok(), fresh == true);
    }
}

fn second(root: &Path, artifact: &Value) -> Value {
    let mut artifact = artifact.clone();
    artifact["target"] =
        serde_json::json!({"name":"tool", "kind":["bin"], "src_path":root.join("src/bin/tool.rs")});
    artifact["executable"] = serde_json::json!(root.join("tool"));
    artifact
}

#[test]
fn complete_records_are_admitted_before_closeout_across_arbitrary_byte_boundaries() {
    let (root, artifact) = fixture();
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    let mut bytes = b"{\"reason\":\"compiler-message\",\"message\":\"".to_vec();
    bytes.extend_from_slice("λ".as_bytes());
    bytes.extend_from_slice(b"\"}\n");
    bytes.extend(record(&artifact));
    let mut ready = 0;
    for byte in bytes {
        stream
            .observe(&[byte], control(), |harness| {
                assert_eq!(harness.executable, root.0.join("library"));
                ready += 1;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(ready, 1);
    assert!(!stream.build_finished);
    stream
        .observe(FINISHED, control(), |_| {
            panic!("completion is not another artifact")
        })
        .unwrap();
    stream
        .finish_terlan_library()
        .unwrap()
        .verify(control())
        .unwrap();
}

#[test]
fn duplicate_target_and_shared_executable_cannot_notify_a_second_owner() {
    let (root, artifact) = fixture();
    for shared_executable in [false, true] {
        let mut stream = CargoArtifactStream::new(&root.0, |_| true).unwrap();
        let mut ready = 0;
        stream
            .observe(&record(&artifact), control(), |_| {
                ready += 1;
                Ok(())
            })
            .unwrap();
        let mut duplicate = if shared_executable {
            second(&root.0, &artifact)
        } else {
            artifact.clone()
        };
        duplicate["executable"] = artifact["executable"].clone();
        assert!(stream
            .observe(&record(&duplicate), control(), |_| {
                ready += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(ready, 1);
        assert!(stream.observe(FINISHED, control(), |_| Ok(())).is_err());
        assert!(stream.finish().is_err());
    }
}

#[test]
fn framing_protocol_and_ignored_callback_errors_cannot_seal_success() {
    let (root, artifact) = fixture();
    for malformed in [
        b"not-json\n".as_slice(),
        b"{}\n",
        b"\n",
        b"{\"reason\":\"build-finished\",\"success\":false}\n",
        b"{\"reason\":\"build-finished\",\"success\":\"true\"}\n",
    ] {
        let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
        assert!(stream.observe(malformed, control(), |_| Ok(())).is_err());
        assert!(stream.finish().is_err());
    }
    for tail in [
        b"".as_slice(),
        b"{\"reason\":\"build-finished\",\"success\":true}",
    ] {
        let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
        stream
            .observe(&record(&artifact), control(), |_| Ok(()))
            .unwrap();
        stream.observe(tail, control(), |_| Ok(())).unwrap();
        assert!(stream.finish().is_err());
    }
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    assert!(stream
        .observe(&record(&artifact), control(), |_| Err(failure(
            "cannot publish authorization"
        )))
        .is_err());
    assert!(stream.finish().is_err());
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    stream.observe(FINISHED, control(), |_| Ok(())).unwrap();
    assert!(stream
        .observe(&record(&artifact), control(), |_| Ok(()))
        .is_err());
    assert!(stream.finish().is_err());
}

#[test]
fn byte_and_artifact_limits_reject_before_additional_notifications() {
    let (root, artifact) = fixture();
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    assert!(stream
        .observe(&vec![b' '; MAX_RECORD_BYTES + 1], control(), |_| panic!(
            "overlong record"
        ))
        .is_err());
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    stream.bytes = MAX_STREAM_BYTES;
    assert!(stream
        .observe(b"x", control(), |_| panic!("overlong stream"))
        .is_err());
    let mut stream = CargoArtifactStream::new(&root.0, |_| true).unwrap();
    stream
        .observe(&record(&artifact), control(), |_| Ok(()))
        .unwrap();
    stream
        .harnesses
        .resize(MAX_HARNESSES, stream.harnesses[0].clone());
    assert!(stream
        .observe(&record(&second(&root.0, &artifact)), control(), |_| panic!(
            "too many artifacts"
        ))
        .is_err());
}

#[test]
fn custom_missing_and_malformed_artifacts_do_not_reach_the_runner_callback() {
    let (root, artifact) = fixture();
    for field in [
        "package_id",
        "manifest_path",
        "executable",
        "target",
        "profile",
    ] {
        let mut invalid = artifact.clone();
        invalid[field] = Value::Null;
        let mut stream = CargoArtifactStream::new(&root.0, |_| true).unwrap();
        assert!(stream
            .observe(&record(&invalid), control(), |_| panic!(
                "unadmitted target"
            ))
            .is_err());
    }
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = \"terlan\"\n[lib]\nharness = false\n",
    )
    .unwrap();
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    assert!(stream
        .observe(&record(&artifact), control(), |_| panic!("custom harness"))
        .is_err());
}

#[test]
fn package_snapshot_is_reused_but_mutated_declarations_fail_before_execution() {
    let (root, artifact) = fixture();
    let mut stream = CargoArtifactStream::new(&root.0, |_| true).unwrap();
    stream
        .observe(&record(&artifact), control(), |_| Ok(()))
        .unwrap();
    // Parsing again would fail; the second target must use the same admitted snapshot.
    fs::write(root.0.join("Cargo.toml"), "not valid TOML !").unwrap();
    stream
        .observe(&record(&second(&root.0, &artifact)), control(), |_| Ok(()))
        .unwrap();
    stream.observe(FINISHED, control(), |_| Ok(())).unwrap();
    let declarations = stream.finish().unwrap();
    assert_eq!(declarations.len(), 2);
    for declaration in declarations {
        assert!(declaration.verify(control()).is_err());
    }
}

#[test]
fn callback_unwind_poisoning_survives_a_catching_caller() {
    let (root, artifact) = fixture();
    let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stream.observe(&record(&artifact), control(), |_| {
            panic!("fixture callback panic")
        })
    }))
    .is_err());
    assert!(stream.observe(FINISHED, control(), |_| Ok(())).is_err());
    assert!(stream.finish().is_err());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn capture_retains_failed_exit_and_rejects_repeated_launch_before_spawning() {
    let (root, artifact) = fixture();
    for exit in ["0", "7"] {
        let mut stream = CargoArtifactStream::terlan_library(&root.0).unwrap();
        let mut output = record(&artifact);
        output.extend_from_slice(FINISHED);
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf %s \"$STREAM_RECORD\"; exit \"$STREAM_EXIT\""])
            .env("STREAM_RECORD", String::from_utf8(output).unwrap())
            .env("STREAM_EXIT", exit);
        let mut launches = 0;
        let result = stream.capture(
            &mut command,
            control(),
            &mut |_| {
                launches += 1;
                Ok(())
            },
            |_| Ok(()),
        );
        if exit == "0" {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err().outcome, "failed");
        }
        assert!(stream
            .capture(
                &mut command,
                control(),
                &mut |_| {
                    launches += 1;
                    Ok(())
                },
                |_| Ok(())
            )
            .is_err());
        assert_eq!(launches, 1);
        assert!(stream.finish().is_err());
    }
}
