use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn no_tvm_json_artifact_rejections() {
    let root = std::env::temp_dir().join(format!(
        "terlan-tvm-transition-rejection-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create transition rejection fixture root");
    let json = root.join("serialized.tvm.json");
    fs::write(
        &json,
        br#"{"vm_ir":{"functions":[{"instructions":["interpret-me"]}]}}"#,
    )
    .expect("write serialized instruction payload");

    for command in ["run", "load"] {
        let output = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
            .arg(command)
            .arg(&json)
            .output()
            .unwrap_or_else(|error| panic!("start terlan-vm {command}: {error}"));
        assert!(!output.status.success(), "{command} accepted .tvm.json");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("error[tvm_json_runtime_removed]"),
            "{command} returned the wrong rejection: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_file(&json).expect("remove transition source fixture");
    fs::remove_dir_all(&root).expect("remove transition rejection fixture root");
}

#[test]
fn no_vmir_interpreter_rejections() {
    let root = std::env::temp_dir().join(format!(
        "terlan-tvm-transition-rejection-sidecar-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create transition rejection fixture root");

    let sidecar = root.join("legacy.tvm.json");
    let tvm = root.join("legacy.tvm");
    fs::write(
        &sidecar,
        br#"{"vm_ir":{"functions":[{"instructions":["interpret-me"]}]}}"#,
    )
    .expect("write stale sidecar");
    fs::write(&tvm, br#"{"not":"a native tvm image"}"#).expect("write fake tvm payload");

    for command in ["run", "load"] {
        let output = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
            .arg(command)
            .arg(&tvm)
            .output()
            .unwrap_or_else(|error| panic!("start stale sidecar {command}: {error}"));
        assert!(
            !output.status.success(),
            "{command} accepted fake tvm path alongside legacy sidecar"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error[tvm_json_runtime_removed]"),
            "{command} tried removed json runtime fallback: {stderr}"
        );
        assert!(
            stderr.contains("error[tvm.image"),
            "{command} did not apply native image admission: {stderr}"
        );
    }

    let output = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&sidecar)
        .output()
        .unwrap_or_else(|error| panic!("start sidecar run: {error}"));
    assert!(
        !output.status.success(),
        "run accepted sidecar file when tvm path was available"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error[tvm_json_runtime_removed]"),
        "run did not reject stale sidecar: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let renamed = root.join("serialized.tvm");
    fs::write(
        &renamed,
        br#"{"vm_ir":{"functions":[{"instructions":["interpret-me"]}]}}"#,
    )
    .expect("write renamed serialized payload");
    for command in ["run", "load"] {
        let output = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
            .arg(command)
            .arg(&renamed)
            .output()
            .unwrap_or_else(|error| panic!("start renamed terlan-vm {command}: {error}"));
        assert!(!output.status.success(), "{command} accepted renamed JSON");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("error[tvm.image"),
            "{command} did not apply native-image admission: {stderr}"
        );
        assert!(
            !stderr.contains("vm_ir"),
            "{command} interpreted renamed JSON"
        );
    }

    fs::remove_dir_all(&root).expect("remove transition rejection fixture root");
}
