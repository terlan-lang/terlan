//! Exercise snapshot publication and reader admission through the real driver.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    let directory = std::env::temp_dir().join(format!(
        "terlan-driver-snapshot-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    Fixture(directory)
}

fn executable(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn install(driver: &Path, destination: &Path, success: bool) -> String {
    let mut command = Command::new(driver);
    command.arg("--install-snapshot").arg(destination);
    let result = ProcessControl::new(Duration::from_secs(30))
        .capture_stdout_observed(&mut command, 16 * 1024, |_| Ok(()), |_| Ok(()))
        .unwrap();
    assert_eq!(result.outcome.is_ok(), success, "{:?}", result.outcome);
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn installed_driver_survives_build_output_replacement_and_obeys_cross_process_leases() {
    let fixture = fixture();
    let source = executable(&fixture.0, "cargo-output");
    let snapshot = executable(&fixture.0, "snapshot");
    let child_snapshot = executable(&fixture.0, "child-snapshot");
    fs::copy(env!("CARGO_BIN_EXE_terlan-test-orchestrator"), &source).unwrap();
    assert!(install(&source, &snapshot, true).contains("snapshot installed"));
    let installed = fs::read(&snapshot).unwrap();
    let modified = fs::metadata(&snapshot).unwrap().modified().unwrap();
    assert!(install(&source, &snapshot, true).contains("snapshot unchanged"));
    assert_eq!(
        fs::metadata(&snapshot).unwrap().modified().unwrap(),
        modified
    );

    let mut lock_path = snapshot.as_os_str().to_os_string();
    lock_path.push(".lock");
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    lease.try_lock_shared().unwrap();
    install(&source, &snapshot, false);
    // Another reader can dispatch while bootstrap is excluded.
    install(&snapshot, &child_snapshot, true);
    lease.unlock().unwrap();

    lease.try_lock().unwrap();
    let blocked_destination = executable(&fixture.0, "blocked");
    install(&snapshot, &blocked_destination, false);
    assert!(
        !blocked_destination.exists(),
        "writer must exclude dispatch"
    );
    lease.unlock().unwrap();

    fs::write(&source, b"a subsequent Cargo link replaces its output").unwrap();
    assert_eq!(fs::read(&snapshot).unwrap(), installed);
    // Actual execution still succeeds from the old admitted driver bytes.
    assert!(install(&snapshot, &child_snapshot, true).contains("snapshot unchanged"));
}
