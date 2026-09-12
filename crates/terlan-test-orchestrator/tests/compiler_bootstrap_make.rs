//! Execute compiler bootstrap through the real bounded owner and snapshot lease.
#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn executable(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-compiler-bootstrap-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for file in [
        "Makefile",
        "mk/rust-coverage.mk",
        "crates/terlan/cli.mk",
        "std/stdlib.mk",
        "editors/editor.mk",
        "mk/code-quality.mk",
    ] {
        let target = fixture.0.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    fs::write(
        fixture.0.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.0.8\"\n",
    )
    .unwrap();
    fs::create_dir_all(fixture.0.join("target/debug")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_terlan-test-orchestrator"),
        fixture.0.join("target/debug/terlan-test-orchestrator"),
    )
    .unwrap();
    executable(
        &fixture.0.join("target/debug/terlan-build-cache"),
        "#!/bin/sh\nset -eu\nprintf 'admission\\n' >> events\n",
    );
    executable(
        &fixture.0.join("cargo"),
        r#"#!/bin/sh
set -eu
case "$*" in
  'build -p terlan-test-orchestrator -p terlan-build-cache'|'build -p terlan-test-orchestrator')
    printf 'support\n' >> events
    if test "$BOOTSTRAP_FIXTURE_MODE" = support-failure; then exit 7; fi
    ;;
  'build -p terlan --bin terlc --bin terlan-vm --bin terlan-native-worker')
    mkdir compiler-started
    printf 'compiler\n' >> events
    if read value; then exit 91; fi
    case "$BOOTSTRAP_FIXTURE_MODE" in
      failure) exit 9 ;;
      timeout) trap '' TERM; sleep 300 & echo $! > descendant; wait ;;
      *) printf 'compiler-completed\n' ;;
    esac
    ;;
  *) exit 92 ;;
esac
"#,
    );
    fs::OpenOptions::new()
        .append(true)
        .open(fixture.0.join("Makefile"))
        .unwrap()
        .write_all(b"\nfixture-a fixture-b: terlan-compiler-bootstrap\n")
        .unwrap();
    fixture
}

fn command(fixture: &Fixture, mode: &str) -> Command {
    let mut command = Command::new("make");
    command.current_dir(&fixture.0).args([
        "--no-print-directory",
        "-j4",
        "fixture-a",
        "fixture-b",
        "CARGO=./cargo",
        "TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS=1",
    ]);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| {
            key.starts_with("TERLAN_")
                || matches!(
                    key,
                    "MAKEFLAGS" | "MAKEOVERRIDES" | "MFLAGS" | "GNUMAKEFLAGS" | "MAKEFILES"
                )
        }) {
            command.env_remove(key);
        }
    }
    command.env("BOOTSTRAP_FIXTURE_MODE", mode).env(
        "TERLAN_PROCESS_ACTIVITY_LOG",
        fixture.0.join("processes.jsonl"),
    );
    command
}

fn descendant_stopped(root: &Path) {
    let pid = fs::read_to_string(root.join("descendant")).unwrap();
    let path = format!("/proc/{}/stat", pid.trim());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Ok(stat) if stat.rsplit_once(") ").unwrap().1.starts_with('Z') => return,
            _ => assert!(
                Instant::now() < deadline,
                "compiler descendant remains live"
            ),
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn compiler_bootstrap_shares_support_and_owns_success_failure_timeout_and_stdin() {
    for mode in ["success", "failure", "timeout", "support-failure"] {
        let fixture = fixture();
        let output = ProcessControl::new(Duration::from_secs(15))
            .capture_stdout_result(&mut command(&fixture, mode), 64 * 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(
            output.outcome.is_ok(),
            mode == "success",
            "{mode}: {output:?}"
        );
        let events = fs::read_to_string(fixture.0.join("events")).unwrap();
        assert_eq!(
            events,
            if mode == "support-failure" {
                "support\n"
            } else {
                "support\nadmission\ncompiler\n"
            }
        );
        let rows: Vec<serde_json::Value> = fs::read_to_string(fixture.0.join("processes.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let cargo: Vec<_> = rows
            .iter()
            .filter(|row| row["program_kind"] == "cargo")
            .collect();
        if mode == "support-failure" {
            assert!(cargo.is_empty());
            continue;
        }
        assert_eq!(cargo.len(), 3, "{rows:?}");
        assert_eq!(cargo[0]["state"], "started");
        assert_eq!(cargo[1]["state"], "spawned");
        assert_eq!(cargo[2]["state"], "reaped");
        assert_eq!(cargo[2]["exit_success"], mode == "success");
        assert!(cargo
            .iter()
            .all(|row| row["attempt"] == cargo[0]["attempt"]));
        assert!(fixture
            .0
            .join("target/validation-tools/terlan-test-orchestrator")
            .is_file());
        if mode == "timeout" {
            descendant_stopped(&fixture.0);
        }
    }
}

/// Exercise the production non-Linux Make branch on a bounded host fixture.
/// This is recipe coverage, not macOS or Windows runtime certification.
#[test]
fn non_linux_bootstrap_runs_compiler_and_propagates_failure() {
    for platform in ["Darwin", "MINGW64_NT-10.0"] {
        for mode in ["success", "failure"] {
            let fixture = fixture();
            executable(
                &fixture.0.join("uname"),
                &format!("#!/bin/sh\nprintf '%s\\n' '{platform}'\n"),
            );
            let mut paths = vec![fixture.0.clone()];
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
            let mut command = command(&fixture, mode);
            command.env("PATH", std::env::join_paths(paths).unwrap());
            let output = ProcessControl::new(Duration::from_secs(15))
                .capture_stdout_result(&mut command, 64 * 1024, |_| Ok(()))
                .unwrap();
            assert_eq!(
                output.outcome.is_ok(),
                mode == "success",
                "{platform}: {output:?}"
            );
            assert_eq!(
                fs::read_to_string(fixture.0.join("events")).unwrap(),
                "support\ncompiler\n",
                "{platform}: compiler skipped or Linux admission selected"
            );
        }
    }
}
