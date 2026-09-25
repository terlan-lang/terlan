//! Execute the production clean-checkout support bootstrap, including its first
//! build when neither support binary exists. Cargo is a counted fixture producer.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terlan_process_owner::{OwnedChild, ProcessControl};

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

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Bootstrap test",
            "-c",
            "user.email=bootstrap@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn fixture() -> Fixture {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(
        std::env::temp_dir().join(format!("terlan-support-{}-{stamp}", std::process::id())),
    );
    fs::create_dir(&fixture.0).unwrap();
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
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "crates/terlan-build-cache/Cargo.toml",
        "crates/terlan-test-orchestrator/Cargo.toml",
    ] {
        let target = fixture.0.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    fs::write(fixture.0.join(".gitignore"), "/target/\n").unwrap();
    executable(
        &fixture.0.join("cargo-fixture"),
        r#"#!/bin/sh
set -eu
if read value; then exit 91; fi
mkdir -p target/debug
printf 'cargo\n' >> target/launches
case "$SUPPORT_MODE" in
  timeout) sleep 300 ;;
  held) printf 'started\n' > target/in-flight; sleep 300 ;;
  interrupted) kill -KILL $$ ;;
esac
cp "$SUPPORT_OWNER_BINARY" target/debug/owner.pending
chmod 700 target/debug/owner.pending
mv target/debug/owner.pending target/debug/terlan-build-cache
printf '#!/bin/sh\nexit 0\n' > target/debug/terlan-test-orchestrator
chmod 700 target/debug/terlan-test-orchestrator
for name in terlan-build-cache terlan-test-orchestrator; do
  printf '{"reason":"compiler-artifact","target":{"name":"%s","kind":["bin"]},"profile":{"test":false},"executable":"%s/target/debug/%s"}\n' "$name" "$PWD" "$name"
done
if test "$SUPPORT_MODE" != incomplete; then
  printf '{"reason":"build-finished","success":true}\n'
fi
if test "$SUPPORT_MODE" = failure; then exit 7; fi
if test "$SUPPORT_MODE" = changed; then printf '\n# mutated\n' >> Cargo.toml; fi
if test "$SUPPORT_MODE" = config-changed; then printf '\n# changed during build\n' >> "$CARGO_HOME/config.toml"; fi
"#,
    );
    git(&fixture.0, &["init", "--quiet"]);
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "--quiet", "-m", "fixture"]);
    fixture
}

fn run(fixture: &Fixture, mode: &str) -> bool {
    run_with_environment(fixture, mode, &[])
}

fn make_command(fixture: &Fixture, mode: &str, entries: &[(&str, &str)]) -> Command {
    let mut command = Command::new("make");
    command.current_dir(&fixture.0).args([
        "--no-print-directory",
        "terlan-build-owner-bootstrap",
        "CARGO=./cargo-fixture",
        "TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS=2",
    ]);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| {
            key.starts_with("TERLAN_")
                || matches!(
                    key,
                    "MAKEFLAGS"
                        | "MAKEOVERRIDES"
                        | "MFLAGS"
                        | "GNUMAKEFLAGS"
                        | "MAKEFILES"
                        | "CARGO_TARGET_DIR"
                )
        }) {
            command.env_remove(key);
        }
    }
    command.env("SUPPORT_MODE", mode).env(
        "SUPPORT_OWNER_BINARY",
        env!("CARGO_BIN_EXE_terlan-build-cache"),
    );
    command.envs(entries.iter().copied());
    command
}

fn run_with_environment(fixture: &Fixture, mode: &str, entries: &[(&str, &str)]) -> bool {
    let mut command = make_command(fixture, mode, entries);
    let output = ProcessControl::new(Duration::from_secs(30))
        .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
        .unwrap();
    if output.outcome.is_err() && mode == "success" {
        panic!("{output:?}");
    }
    output.outcome.is_ok()
}

fn receipt(fixture: &Fixture) -> PathBuf {
    fixture.0.join(format!(
        "target/quality/preparation/bootstrap/{}/support.json",
        git(&fixture.0, &["rev-parse", "HEAD"])
    ))
}

fn assert_no_support_scratch(fixture: &Fixture) {
    for entry in fs::read_dir(fixture.0.join("target/quality")).unwrap() {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.starts_with("support-cargo.") && name != "support-bootstrap.pending",
            "unretired support scratch: {name}"
        );
    }
}

#[test]
fn first_success_seals_and_identical_warm_build_launches_no_cargo() {
    let fixture = fixture();
    assert!(run(&fixture, "success"));
    let first = fs::read(receipt(&fixture)).unwrap();
    assert!(run(&fixture, "success"));
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\n"
    );
    assert_eq!(first, fs::read(receipt(&fixture)).unwrap());
    assert_no_support_scratch(&fixture);
    // Tampering with a declared output invalidates reuse and runs the producer.
    fs::write(
        fixture.0.join("target/debug/terlan-test-orchestrator"),
        "#!/bin/sh\nexit 1\n",
    )
    .unwrap();
    assert!(run(&fixture, "success"));
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\ncargo\n"
    );
}

#[test]
fn older_support_owner_is_rebuilt_once_before_using_the_new_receipt_protocol() {
    let fixture = fixture();
    fs::create_dir_all(fixture.0.join("target/debug")).unwrap();
    executable(
        &fixture.0.join("target/debug/terlan-build-cache"),
        "#!/bin/sh\necho unsupported-owner-option >&2\nexit 2\n",
    );
    executable(
        &fixture.0.join("target/debug/terlan-test-orchestrator"),
        "#!/bin/sh\nexit 0\n",
    );
    assert!(run(&fixture, "success"));
    assert!(run(&fixture, "success"));
    assert!(receipt(&fixture).is_file());
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\n"
    );
}

#[test]
fn stale_scratch_after_success_does_not_replay_a_verified_producer() {
    let fixture = fixture();
    assert!(run(&fixture, "success"));
    let verified = fs::read(receipt(&fixture)).unwrap();
    let scratch = fixture.0.join("target/quality/support-bootstrap.pending");
    fs::create_dir(&scratch).unwrap();
    fs::write(
        scratch.join("cargo.jsonl"),
        b"interrupted scratch retirement",
    )
    .unwrap();
    assert!(run(&fixture, "success"));
    assert_no_support_scratch(&fixture);
    assert_eq!(verified, fs::read(receipt(&fixture)).unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\n"
    );
}

#[test]
fn failed_incomplete_interrupted_or_changed_cold_builds_cannot_seal() {
    for mode in ["failure", "incomplete", "interrupted", "timeout", "changed"] {
        let fixture = fixture();
        assert!(!run(&fixture, mode), "{mode}");
        assert!(!receipt(&fixture).exists(), "{mode}");
        assert_no_support_scratch(&fixture);
        if mode != "changed" {
            assert!(run(&fixture, "success"));
            assert!(receipt(&fixture).is_file());
            assert!(run(&fixture, "success"));
            assert_eq!(
                fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
                "cargo\ncargo\n"
            );
        }
    }
}

#[test]
fn concurrent_cold_invocations_share_one_producer_under_the_bootstrap_lock() {
    let fixture = fixture();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| run(&fixture, "success"));
        let second = scope.spawn(|| run(&fixture, "success"));
        assert!(first.join().unwrap());
        assert!(second.join().unwrap());
    });
    assert!(receipt(&fixture).is_file());
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\n"
    );
}

#[test]
fn dirty_source_is_uncached_and_a_new_commit_cannot_reuse_an_old_receipt() {
    let fixture = fixture();
    assert!(run(&fixture, "success"));
    let original_receipt = receipt(&fixture);
    let original_bytes = fs::read(&original_receipt).unwrap();
    fs::write(fixture.0.join("tracked-input"), "new dependency").unwrap();
    assert!(run(&fixture, "success"));
    assert!(run(&fixture, "success"));
    assert_eq!(original_bytes, fs::read(&original_receipt).unwrap());
    git(&fixture.0, &["add", "tracked-input"]);
    git(&fixture.0, &["commit", "--quiet", "-m", "changed input"]);
    assert_ne!(receipt(&fixture), original_receipt);
    assert!(run(&fixture, "success"));
    assert!(receipt(&fixture).is_file());
    assert!(run(&fixture, "success"));
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\ncargo\ncargo\ncargo\n"
    );
}

#[test]
fn changed_compiler_flags_profile_and_build_script_inputs_invalidate_reuse() {
    for key in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_PROFILE_DEV_DEBUG",
        "CUSTOM_BUILD_INPUT",
    ] {
        let fixture = fixture();
        assert!(run_with_environment(&fixture, "success", &[(key, "one")]));
        let before = fs::read(receipt(&fixture)).unwrap();
        assert!(run_with_environment(&fixture, "success", &[(key, "two")]));
        let after = fs::read(receipt(&fixture)).unwrap();
        assert_ne!(before, after, "{key}");
        assert!(run_with_environment(&fixture, "success", &[(key, "two")]));
        assert_eq!(after, fs::read(receipt(&fixture)).unwrap());
        assert_eq!(
            fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
            "cargo\ncargo\n",
            "{key}"
        );
    }
}

#[test]
fn changed_external_cargo_configuration_invalidates_an_unchanged_environment() {
    let fixture = fixture();
    let cargo_home = fixture.0.join("target/ambient-cargo");
    fs::create_dir_all(&cargo_home).unwrap();
    let configuration = cargo_home.join("config.toml");
    fs::write(&configuration, "[build]\nrustflags = ['-Cdebuginfo=1']\n").unwrap();
    let entries = [("CARGO_HOME", cargo_home.to_str().unwrap())];
    assert!(run_with_environment(&fixture, "success", &entries));
    fs::write(&configuration, "[build]\nrustflags = ['-Cdebuginfo=2']\n").unwrap();
    assert!(run_with_environment(&fixture, "success", &entries));
    assert!(run_with_environment(&fixture, "success", &entries));
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\ncargo\n"
    );
}

#[test]
fn changed_external_wrapper_bytes_invalidate_an_unchanged_path() {
    let fixture = fixture();
    fs::create_dir_all(fixture.0.join("target")).unwrap();
    let wrapper = fixture.0.join("target/compiler-wrapper");
    executable(&wrapper, "#!/bin/sh\n# first revision\nexec \"$@\"\n");
    let entries = [("RUSTC_WRAPPER", wrapper.to_str().unwrap())];
    assert!(run_with_environment(&fixture, "success", &entries));
    executable(&wrapper, "#!/bin/sh\n# second revision\nexec \"$@\"\n");
    assert!(run_with_environment(&fixture, "success", &entries));
    assert!(run_with_environment(&fixture, "success", &entries));
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\ncargo\n"
    );
}

#[test]
fn warm_producer_rejects_configuration_mutation_and_preserves_previous_receipt() {
    let fixture = fixture();
    let cargo_home = fixture.0.join("target/ambient-cargo");
    fs::create_dir_all(&cargo_home).unwrap();
    fs::write(cargo_home.join("config.toml"), "[build]\nrustflags = []\n").unwrap();
    let entries = [("CARGO_HOME", cargo_home.to_str().unwrap())];
    assert!(run_with_environment(&fixture, "success", &entries));
    let original = fs::read(receipt(&fixture)).unwrap();
    assert!(!run_with_environment(&fixture, "config-changed", &entries));
    assert_eq!(original, fs::read(receipt(&fixture)).unwrap());
    assert!(run_with_environment(&fixture, "success", &entries));
    assert!(run_with_environment(&fixture, "success", &entries));
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\ncargo\ncargo\n"
    );
}

#[test]
fn killed_bootstrap_owner_leaves_no_reusable_receipt_and_resume_retires_its_scratch() {
    let fixture = fixture();
    let mut command = make_command(&fixture, "held", &[]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !fixture.0.join("target/in-flight").exists() {
        assert!(child.try_wait().unwrap().is_none(), "producer exited early");
        assert!(Instant::now() < deadline, "producer did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    // SIGKILL cannot run shell traps. The next acquisition must wait for any
    // bounded descendants still holding the bootstrap lease before retirement.
    assert!(!child.finish().unwrap().success());
    assert!(fixture
        .0
        .join("target/quality/support-bootstrap.pending/cargo.jsonl")
        .is_file());
    assert!(!receipt(&fixture).exists());
    assert!(run(&fixture, "success"));
    assert!(run(&fixture, "success"));
    assert_no_support_scratch(&fixture);
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/launches")).unwrap(),
        "cargo\ncargo\n"
    );
}

#[test]
fn scratch_retirement_preserves_redirected_and_unrecognized_entries() {
    use std::os::unix::fs::symlink;

    for mode in ["directory-link", "file-link", "unexpected"] {
        let fixture = fixture();
        let outside = fixture.0.join("target/preserved");
        fs::create_dir_all(&outside).unwrap();
        let protected = outside.join("keep");
        fs::write(&protected, b"not bootstrap scratch").unwrap();
        let scratch = fixture.0.join("target/quality/support-bootstrap.pending");
        fs::create_dir_all(scratch.parent().unwrap()).unwrap();
        if mode == "directory-link" {
            symlink(&outside, &scratch).unwrap();
        } else {
            fs::create_dir(&scratch).unwrap();
            if mode == "file-link" {
                symlink(&protected, scratch.join("cargo.jsonl")).unwrap();
            } else {
                fs::write(scratch.join("keep"), b"unrecognized").unwrap();
            }
        }
        assert!(!run(&fixture, "invalid-scratch"), "{mode}");
        assert!(!fixture.0.join("target/launches").exists(), "{mode}");
        assert!(fs::symlink_metadata(&scratch).is_ok(), "{mode}");
        assert_eq!(fs::read(&protected).unwrap(), b"not bootstrap scratch");
        if mode == "unexpected" {
            assert_eq!(fs::read(scratch.join("keep")).unwrap(), b"unrecognized");
        }
    }
}
