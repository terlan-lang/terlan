//! Exercise the real typed cache wrapper with an installed, observed build owner.
#![cfg(target_os = "linux")]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "terlan-typed-owner-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("target/validation-tools")).unwrap();
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts/build_typed_validator.sh"),
            root.join("scripts/build_typed_validator.sh"),
        )
        .unwrap();
        fs::write(root.join("Input.terl"), b"module fixture.Input.\n").unwrap();
        let fixture = Self(root);
        let mut install = Command::new(env!("CARGO_BIN_EXE_terlan-test-orchestrator"));
        install.arg("--install-snapshot").arg(fixture.owner());
        ProcessControl::new(Duration::from_secs(10))
            .run(&mut install, |_| Ok(()))
            .unwrap();
        fixture
    }

    fn owner(&self) -> PathBuf {
        self.0
            .join("target/validation-tools/terlan-test-orchestrator")
    }

    fn run(&self, label: &str, success: bool, expected_launches: usize) {
        let log = self.0.join(format!("{label}.jsonl"));
        let mut command = Command::new("bash");
        command
            .current_dir(&self.0)
            .args([
                "scripts/build_typed_validator.sh",
                "output.tvm",
                "Input.terl",
                "--",
                "/bin/cp",
                "Input.terl",
                "output.tvm",
            ])
            .env("TERLAN_PROCESS_ACTIVITY_LOG", &log);
        let captured = ProcessControl::new(Duration::from_secs(10))
            .capture_stdout_result(&mut command, 16 * 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(
            captured.outcome.is_ok(),
            success,
            "{label}: {:?}",
            captured.outcome
        );
        let rows: Vec<serde_json::Value> = fs::read_to_string(log)
            .unwrap()
            .lines()
            .map(|row| serde_json::from_str(row).unwrap())
            .collect();
        let starts: Vec<_> = rows
            .iter()
            .filter(|row| row["state"] == "started")
            .collect();
        assert_eq!(starts.len(), expected_launches, "{label}: {rows:?}");
        assert_eq!(rows.len(), expected_launches * 3, "{label}: {rows:?}");
        for start in starts {
            let attempt: Vec<_> = rows
                .iter()
                .filter(|row| row["attempt"] == start["attempt"])
                .collect();
            assert_eq!(attempt.len(), 3);
            assert_eq!(attempt[1]["state"], "spawned");
            assert_eq!(attempt[2]["state"], "reaped");
            assert_eq!(attempt[1]["child_pid"], attempt[2]["child_pid"]);
            assert_eq!(attempt[2]["exit_success"], success);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn typed_cache_observes_only_cold_producers_and_requires_matching_owner_bytes() {
    let fixture = Fixture::new();
    fixture.run("cold", true, 2);
    let output = fixture.0.join("output.tvm");
    let stamp = fixture.0.join("output.tvm.inputs.sha256");
    let cold_stamp = fs::read(&stamp).unwrap();
    let cold_modified = fs::metadata(&output).unwrap().modified().unwrap();
    fixture.run("warm", true, 1);
    assert_eq!(fs::read(&stamp).unwrap(), cold_stamp);
    assert_eq!(
        fs::metadata(&output).unwrap().modified().unwrap(),
        cold_modified
    );

    // An otherwise usable image is not admitted with a missing execution owner.
    let saved = fixture.0.join("saved-owner");
    fs::rename(fixture.owner(), &saved).unwrap();
    fixture.run("missing-owner", false, 1);
    assert_eq!(fs::read(&stamp).unwrap(), cold_stamp);
    fs::rename(saved, fixture.owner()).unwrap();
    fixture.run("restored-owner", true, 1);

    // Linux ELF permits trailing bytes: change tool identity, not tool behavior.
    OpenOptions::new()
        .append(true)
        .open(fixture.owner())
        .unwrap()
        .write_all(b"\nchanged fixture identity\n")
        .unwrap();
    fixture.run("changed-owner", true, 2);
    assert_ne!(fs::read(&stamp).unwrap(), cold_stamp);
    fixture.run("changed-owner-warm", true, 1);
    fs::rename(fixture.owner(), fixture.owner().with_extension("exe")).unwrap();
    fixture.run("executable-suffix", true, 1);
    assert_eq!(
        fs::read(&output).unwrap(),
        fs::read(fixture.0.join("Input.terl")).unwrap()
    );
}

#[test]
fn blanket_cleanup_preserves_live_outputs_and_interrupted_publication_evidence() {
    let fixture = Fixture::new();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in ["scripts/clean_build_outputs.sh", "crates/terlan/cli.mk"] {
        let destination = fixture.0.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(repository.join(relative), destination).unwrap();
    }
    fs::write(fixture.0.join("Cargo.toml"), b"[workspace]\n").unwrap();
    fs::write(fixture.0.join(".git"), b"fixture\n").unwrap();
    let protected = [
        "dist/release.tar.zst",
        "scripts/tool/.terlan/native-aot/module.o",
        "target/self-validation/tool.tvm",
        "target/self-validation/tool.tvm.inputs.sha256",
        "target/self-validation/tool.tvm.partial",
        "target/self-validation/tool.tvm.work/journal",
        "target/self-validation/tool.tvm.work/previous.image",
        "target/self-validation/tool.tvm.writer-lease",
        "target/quality/preparation/candidate/owner.json",
        "std/summaries/fixture.erl",
    ];
    for relative in protected {
        let path = fixture.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, relative.as_bytes()).unwrap();
    }
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                .0
                .join("target/self-validation/tool.tvm.writer-lease"),
        )
        .unwrap();
    lease.try_lock().unwrap();
    for held in [true, false] {
        if !held {
            lease.unlock().unwrap();
        }
        for (arguments, succeeds) in [
            (vec![], false),
            (vec!["--dry-run"], true),
            (vec!["--check-partials"], false),
            (vec!["--dry-run", "unexpected"], false),
        ] {
            let mut command = Command::new("bash");
            command
                .current_dir(&fixture.0)
                .arg("scripts/clean_build_outputs.sh")
                .args(arguments);
            let result = ProcessControl::new(Duration::from_secs(10))
                .capture_stdout_result(&mut command, 64 * 1024, |_| Ok(()))
                .unwrap();
            assert_eq!(result.outcome.is_ok(), succeeds);
            for relative in protected {
                assert_eq!(
                    fs::read(fixture.0.join(relative)).unwrap(),
                    relative.as_bytes()
                );
            }
        }
    }
    let mut clean = Command::new("make");
    clean.current_dir(&fixture.0).args([
        "--no-print-directory",
        "-f",
        "crates/terlan/cli.mk",
        "cli-clean",
        "CARGO=touch cargo-was-run",
    ]);
    let result = ProcessControl::new(Duration::from_secs(10))
        .capture_stdout_result(&mut clean, 64 * 1024, |_| Ok(()))
        .unwrap();
    assert!(result.outcome.is_err());
    assert!(!fixture.0.join("cargo-was-run").exists());
    for relative in protected {
        assert_eq!(
            fs::read(fixture.0.join(relative)).unwrap(),
            relative.as_bytes()
        );
    }
}
