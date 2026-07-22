//! Fail-closed direct-AOT source rejection assertions.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verifies that unsupported application code cannot fall back to interpretation.
#[test]
fn direct_aot_rejects_an_unsupported_application_without_runtime_fallback() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-rejection-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create direct-AOT rejection root");
    let source = root.join("unsupported.terl");
    fs::write(
        &source,
        r#"module direct_aot_rejection.

pub condition_composition_too_deep(
    a: Bool, b: Bool, c: Bool, d: Bool, e: Bool,
    f: Bool, g: Bool, h: Bool, i: Bool, value: Bool
): Int ->
    if {
        a and (b or (c and (d or (e and (f or (g and (h or (i and
            (Process.yield_now(); value))))))))) -> 66;
        true -> 67
    }.
"#,
    )
    .expect("write unsupported direct-AOT source");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(root.join("build"))
        .output()
        .expect("start rejecting direct-AOT build");

    assert!(
        !build.status.success(),
        "unsupported source unexpectedly built"
    );
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(
        stderr.contains("error[native_ir.unsupported_application_function]")
            && stderr.contains("runtime CoreIR interpretation has been removed"),
        "unsupported application did not fail closed:\n{stderr}"
    );
    fs::remove_dir_all(&root).expect("remove direct-AOT rejection root");
}
