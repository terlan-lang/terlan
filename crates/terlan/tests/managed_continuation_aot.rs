//! Full-cycle managed continuation coverage for direct-AOT execution.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds one native image and executes every managed suspension graph shape.
#[test]
fn managed_values_survive_every_supported_suspension_graph() {
    let root = std::env::temp_dir().join(format!(
        "terlan-managed-continuation-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create managed continuation fixture root");
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::write(&source, include_str!("fixtures/direct_aot.terl"))
        .expect("write managed continuation fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .output()
        .expect("start managed continuation build");
    assert!(
        build.status.success(),
        "managed continuation fixture did not build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let image = output_dir.join("vm/direct_aot.tvm");
    for entry in [
        "managed_entry_resume",
        "managed_branch_left",
        "managed_branch_right",
        "managed_nested",
        "managed_repeated",
        "managed_tail",
        "managed_non_tail",
        "managed_non_tail_repeated",
    ] {
        let run = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
            .arg("run")
            .arg(&image)
            .arg("--entry")
            .arg(entry)
            .env_remove("TERLAN_NATIVE_WORKER")
            .output()
            .unwrap_or_else(|error| panic!("run {entry}: {error}"));
        assert!(
            run.status.success(),
            "managed suspension entry `{entry}` failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    fs::remove_dir_all(root).expect("remove managed continuation fixture root");
}
