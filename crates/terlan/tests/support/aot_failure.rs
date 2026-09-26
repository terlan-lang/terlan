//! Expected native failures remain bounded, owned, and diagnostically checked.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use terlan_process_owner::{OwnedChild, ProcessControl};

pub(super) fn assert_vm_failure(image: &Path, entry: &str, diagnostics: &[&str]) {
    let log = image.with_extension(format!("{entry}.stderr"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-vm"));
    command
        .arg("run")
        .arg(image)
        .args(["--entry", entry, "--test-eval"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).expect("owned diagnostic file"));
    // ProcessControl::run deliberately inherits stderr. Retain this file
    // redirection while using the same owner cleanup and deadline checks.
    let control = ProcessControl::new(Duration::from_secs(30));
    let started = Instant::now();
    let mut child = OwnedChild::spawn_command(&mut command).expect("start invalid VM call");
    let status = loop {
        control.check(started).expect("invalid VM call deadline");
        if let Some(status) = child.try_wait().expect("observe invalid VM call") {
            break status;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(!status.success(), "{entry}: invalid request must fail");
    assert!(status.code().is_some(), "{entry}: VM must not crash");
    let stderr = fs::read_to_string(&log).expect("read VM failure diagnostic");
    for diagnostic in diagnostics {
        assert!(
            stderr.contains(diagnostic),
            "{entry}: missing {diagnostic}: {stderr}"
        );
    }
}
