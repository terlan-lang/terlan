//! One compilation per focused application contract, without worker-wire execution.

use std::fmt::Write as _;
use std::fs;
use std::process::Command;
use std::time::Duration;

use terlan_process_owner::ProcessControl;

#[path = "aot_failure.rs"]
mod failure;
#[path = "aot_fixture_directory.rs"]
mod fixture_directory;

pub(super) fn assert_program(
    module: &str,
    fixture: &str,
    support: &[(&str, &str)],
    checks: &[&str],
    failures: &[(&str, &str)],
) {
    assert!(
        !checks.is_empty(),
        "application contract must contain assertions"
    );
    let directory = fixture_directory::FixtureDirectory::new(module);
    let root = directory.path();
    let source = root.join(format!("{module}.terl"));
    let output = root.join("build");
    let mut program = fixture.to_owned();
    for (module, source) in support {
        let header = format!("module {module}.\n\nimport std.vm.Process.\n");
        program.push_str(source.strip_prefix(&header).expect("shared fixture header"));
    }
    program.push_str("\npub contract(): Bool ->\n    ");
    program.push_str(&checks.join("\n        and "));
    program.push_str(".\n");
    for (index, (expression, _)) in failures.iter().enumerate() {
        writeln!(
            program,
            "\npub failure_{index}(): Bool ->\n    {expression}; true."
        )
        .expect("append failure entry");
    }
    fs::write(&source, program).expect("write focused AOT source");
    let mut build = Command::new(env!("CARGO_BIN_EXE_terlc"));
    build
        .arg("build")
        .arg(&source)
        .args(["--target", "terlan-vm", "--out-dir"])
        .arg(&output)
        .env("RUSTC", root.join("rustc-must-not-run"));
    ProcessControl::new(Duration::from_secs(120))
        .run(&mut build, |_| Ok(()))
        .unwrap_or_else(|error| panic!("build {module}: {error:?}"));
    let image = output.join(format!("vm/{module}.tvm"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-vm"));
    command
        .arg("run")
        .arg(&image)
        .args(["--entry", "contract", "--test-eval"]);
    ProcessControl::new(Duration::from_secs(30))
        .run(&mut command, |_| Ok(()))
        .unwrap_or_else(|error| panic!("execute {module}: {error:?}"));
    for (index, (_, diagnostic)) in failures.iter().enumerate() {
        failure::assert_vm_failure(&image, &format!("failure_{index}"), &[diagnostic]);
    }
    directory.close();
}
