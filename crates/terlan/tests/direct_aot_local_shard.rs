//! Full-cycle direct-AOT coverage for local execution-shard ownership.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds one native image and executes representative VM transitions locally.
#[test]
fn native_image_transitions_execute_on_the_local_shard() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-local-shard-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create local-shard fixture root");
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::write(&source, include_str!("fixtures/direct_aot.terl"))
        .expect("write local-shard fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .output()
        .expect("start local-shard build");
    assert!(
        build.status.success(),
        "local-shard fixture did not build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let image = output_dir.join("vm/direct_aot.tvm");
    for entry in [
        "main",
        "yielded",
        "yielded_twice",
        "send_to_self",
        "send_then_receive_call",
        "spawn_then_send",
        "timer_then_true",
        "resource_then_true",
        "schedule_priority_then_true",
        "schedule_background_then_true",
    ] {
        let run = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
            .arg("run")
            .arg(&image)
            .arg("--entry")
            .arg(entry)
            .arg("--test-eval")
            .env_remove("TERLAN_NATIVE_WORKER")
            .output()
            .unwrap_or_else(|error| panic!("run {entry}: {error}"));
        assert!(
            run.status.success(),
            "local-shard entry `{entry}` failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    fs::remove_dir_all(root).expect("remove local-shard fixture root");
}
