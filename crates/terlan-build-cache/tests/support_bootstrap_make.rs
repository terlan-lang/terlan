//! Execute production Make and the real OS sandbox with a counted toolchain
//! fixture. The real receipt owner validates cold observations and warm reuse.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
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
        "mk/hermetic-support.mk",
        "crates/terlan/cli.mk",
        "std/stdlib.mk",
        "editors/editor.mk",
        "mk/code-quality.mk",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
    ] {
        let target = fixture.0.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    fs::write(fixture.0.join(".gitignore"), "/target/\n").unwrap();
    fs::create_dir_all(fixture.0.join("target/toolchain/bin")).unwrap();
    fs::create_dir_all(fixture.0.join("target/host-bin")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_terlan-build-cache"),
        fixture.0.join("target/toolchain/owner"),
    )
    .unwrap();
    executable(
        &fixture.0.join("target/host-bin/rustup"),
        "#!/bin/sh\nset -eu\ntest \"$1\" = which\nprintf '%s/target/toolchain/bin/cargo\\n' \"$PWD\"\n",
    );
    for name in ["rustc", "rustdoc"] {
        executable(
            &fixture.0.join("target/toolchain/bin").join(name),
            "#!/bin/sh\necho 'rustc 1.96.0 (fixture)'\n",
        );
    }
    mode(&fixture, "success");
    git(&fixture.0, &["init", "--quiet"]);
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "--quiet", "-m", "fixture"]);
    fixture
}

fn mode(fixture: &Fixture, selected: &str) {
    executable(
        &fixture.0.join("target/toolchain/bin/cargo"),
        &format!(
            "#!/bin/sh\nset -eu\nmode='{selected}'\n{}",
            r#"
if test "$1" = --version; then echo 'cargo 1.96.0 (fixture)'; exit; fi
test "$PWD" = /source
test "$CARGO_HOME" = /cargo
test "$RUSTC" = /toolchain/bin/rustc
test -z "${RUSTC_WRAPPER:-}${CARGO_ENCODED_RUSTFLAGS:-}${CUSTOM_BUILD_INPUT:-}"
test -z "${CARGO_PROFILE_DEV_DEBUG:-}${CARGO_TARGET_DIR:-}"
test ! -e /source/.cargo/config.toml
test ! -e /cargo/config.toml
test ! -e /source/.git
if read value; then exit 91; fi
mkdir -p target/debug
printf 'cargo\n' >> target/launches
case "$mode" in
  timeout) sleep 300 ;;
  held) printf 'started\n' > target/in-flight; sleep 300 ;;
  paused) printf 'started\n' > target/in-flight; while test ! -f target/continue; do sleep 0.02; done ;;
  changed) printf 'mutation\n' >> /source/Cargo.toml ;;
  interrupted) kill -KILL $$ ;;
esac
cp /toolchain/owner target/debug/owner.pending
cp /source/Cargo.toml target/observed-source
chmod 700 target/debug/owner.pending
mv target/debug/owner.pending target/debug/terlan-build-cache
printf '#!/bin/sh\nexit 0\n' > target/debug/terlan-test-orchestrator
chmod 700 target/debug/terlan-test-orchestrator
for name in terlan-build-cache terlan-test-orchestrator; do
  printf '{"reason":"compiler-artifact","target":{"name":"%s","kind":["bin"]},"profile":{"test":false},"executable":"%s/target/debug/%s"}\n' "$name" "$PWD" "$name"
done
if test "$mode" != incomplete; then printf '{"reason":"build-finished","success":true}\n'; fi
if test "$mode" = failure; then exit 7; fi
"#
        ),
    );
}

fn command(fixture: &Fixture) -> Command {
    let mut command = Command::new("make");
    command.current_dir(&fixture.0).args([
        "--no-print-directory",
        "terlan-build-owner-bootstrap",
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
    command.env(
        "PATH",
        format!(
            "{}:{}",
            fixture.0.join("target/host-bin").display(),
            std::env::var("PATH").unwrap(),
        ),
    );
    command
}

fn run(fixture: &Fixture, entries: &[(&str, &str)]) -> bool {
    let mut command = command(fixture);
    command.envs(entries.iter().copied());
    ProcessControl::new(Duration::from_secs(30))
        .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
        .unwrap()
        .outcome
        .is_ok()
}

fn cache(fixture: &Fixture) -> PathBuf {
    fixture.0.join("target/hermetic-support/target")
}

fn receipt(fixture: &Fixture) -> PathBuf {
    cache(fixture).join("quality/hermetic-support.json")
}

fn launches(fixture: &Fixture) -> String {
    fs::read_to_string(cache(fixture).join("launches")).unwrap_or_default()
}

fn assert_no_scratch(fixture: &Fixture) {
    assert!(!fixture
        .0
        .join("target/quality/hermetic-support.pending")
        .exists());
}

#[test]
fn first_success_seals_and_identical_warm_build_launches_no_cargo() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    let first = fs::read(receipt(&fixture)).unwrap();
    assert!(run(&fixture, &[]));
    assert_eq!(launches(&fixture), "cargo\n");
    assert_eq!(first, fs::read(receipt(&fixture)).unwrap());
    assert_no_scratch(&fixture);
    // Installed copies are repaired from verified outputs without Cargo.
    fs::write(
        fixture.0.join("target/debug/terlan-test-orchestrator"),
        "tampered",
    )
    .unwrap();
    assert!(run(&fixture, &[]));
    assert_eq!(launches(&fixture), "cargo\n");
    // A changed producer output requires a new build.
    fs::write(
        cache(&fixture).join("debug/terlan-test-orchestrator"),
        "tampered",
    )
    .unwrap();
    assert!(run(&fixture, &[]));
    assert_eq!(launches(&fixture), "cargo\ncargo\n");
}

#[test]
fn hidden_source_changes_invalidate_a_warm_receipt() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    let first = fs::read(receipt(&fixture)).unwrap();
    git(
        &fixture.0,
        &["update-index", "--assume-unchanged", "std/stdlib.mk"],
    );
    let path = fixture.0.join("std/stdlib.mk");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{original}\n# hidden source edit\n")).unwrap();
    assert!(git(&fixture.0, &["status", "--porcelain"]).is_empty());
    assert!(run(&fixture, &[]));
    assert_ne!(first, fs::read(receipt(&fixture)).unwrap());
    assert!(run(&fixture, &[]));
    assert_eq!(launches(&fixture), "cargo\ncargo\n");
}

#[test]
fn older_support_owner_is_rebuilt_once_before_using_the_new_receipt_protocol() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    executable(
        &cache(&fixture).join("debug/terlan-build-cache"),
        "#!/bin/sh\necho unsupported-owner-option >&2\nexit 2\n",
    );
    assert!(run(&fixture, &[]));
    assert!(run(&fixture, &[]));
    assert!(receipt(&fixture).is_file());
    assert_eq!(launches(&fixture), "cargo\ncargo\n");
}

#[test]
fn stale_scratch_after_success_does_not_replay_a_verified_producer() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    let verified = fs::read(receipt(&fixture)).unwrap();
    let scratch = fixture.0.join("target/quality/hermetic-support.pending");
    fs::create_dir(&scratch).unwrap();
    fs::write(
        scratch.join("source.tar"),
        b"interrupted scratch retirement",
    )
    .unwrap();
    assert!(run(&fixture, &[]));
    assert_no_scratch(&fixture);
    assert_eq!(verified, fs::read(receipt(&fixture)).unwrap());
    assert_eq!(launches(&fixture), "cargo\n");
}

#[test]
fn failed_incomplete_interrupted_or_mutating_cold_builds_cannot_install() {
    for selected in ["failure", "incomplete", "interrupted", "timeout", "changed"] {
        let fixture = fixture();
        mode(&fixture, selected);
        assert!(!run(&fixture, &[]), "{selected}");
        assert!(!receipt(&fixture).exists(), "{selected}");
        assert!(!fixture.0.join("target/debug/terlan-build-cache").exists());
        assert_no_scratch(&fixture);
        mode(&fixture, "success");
        assert!(run(&fixture, &[]), "{selected}");
        assert!(run(&fixture, &[]));
        assert_eq!(launches(&fixture), "cargo\ncargo\n");
    }
}

#[test]
fn concurrent_cold_invocations_share_one_producer_under_the_bootstrap_lock() {
    let fixture = fixture();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| run(&fixture, &[]));
        let second = scope.spawn(|| run(&fixture, &[]));
        assert!(first.join().unwrap());
        assert!(second.join().unwrap());
    });
    assert!(receipt(&fixture).is_file());
    assert_eq!(launches(&fixture), "cargo\n");
}

#[test]
fn actual_source_bytes_not_commit_metadata_own_reuse() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    let original = fs::read(receipt(&fixture)).unwrap();
    fs::write(fixture.0.join("tracked-input"), "new dependency").unwrap();
    assert!(run(&fixture, &[]));
    assert_ne!(original, fs::read(receipt(&fixture)).unwrap());
    assert!(run(&fixture, &[]));
    git(&fixture.0, &["add", "tracked-input"]);
    git(&fixture.0, &["commit", "--quiet", "-m", "changed input"]);
    assert!(run(&fixture, &[]));
    assert_eq!(launches(&fixture), "cargo\ncargo\n");
}

#[test]
fn ambient_compiler_flags_profiles_and_build_inputs_do_not_enter_support_builds() {
    let fixture = fixture();
    let entries = [
        ("RUSTFLAGS", "invalid"),
        ("CARGO_ENCODED_RUSTFLAGS", "invalid"),
        ("CARGO_PROFILE_DEV_DEBUG", "invalid"),
        ("CUSTOM_BUILD_INPUT", "invalid"),
        ("CARGO_TARGET_DIR", "/nonexistent"),
        ("RUSTC_WRAPPER", "/nonexistent"),
    ];
    assert!(run(&fixture, &entries));
    let before = fs::read(receipt(&fixture)).unwrap();
    assert!(run(&fixture, &[]));
    assert_eq!(before, fs::read(receipt(&fixture)).unwrap());
    assert_eq!(launches(&fixture), "cargo\n");
}

#[test]
fn ambient_and_checkout_cargo_configuration_are_excluded() {
    let fixture = fixture();
    let cargo_home = fixture.0.join("target/ambient-cargo");
    fs::create_dir_all(&cargo_home).unwrap();
    fs::create_dir(fixture.0.join(".cargo")).unwrap();
    for configuration in [
        cargo_home.join("config.toml"),
        fixture.0.join(".cargo/config.toml"),
    ] {
        fs::write(configuration, "invalid Cargo configuration\n").unwrap();
    }
    let entries = [("CARGO_HOME", cargo_home.to_str().unwrap())];
    assert!(run(&fixture, &entries));
    fs::write(
        cargo_home.join("config.toml"),
        "changed invalid configuration\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join(".cargo/config.toml"),
        "changed invalid configuration\n",
    )
    .unwrap();
    assert!(run(&fixture, &entries));
    assert_eq!(launches(&fixture), "cargo\n");
}

#[test]
fn changed_tool_bytes_invalidate_a_warm_receipt() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    mode(&fixture, "another-success");
    assert!(run(&fixture, &[]));
    assert!(run(&fixture, &[]));
    assert_eq!(launches(&fixture), "cargo\ncargo\n");
}

#[test]
fn failed_warm_producer_preserves_the_previous_receipt_and_installed_outputs() {
    let fixture = fixture();
    assert!(run(&fixture, &[]));
    let before = fs::read(receipt(&fixture)).unwrap();
    let installed = fs::read(fixture.0.join("target/debug/terlan-build-cache")).unwrap();
    mode(&fixture, "failure");
    assert!(!run(&fixture, &[]));
    assert_eq!(before, fs::read(receipt(&fixture)).unwrap());
    assert_eq!(
        installed,
        fs::read(fixture.0.join("target/debug/terlan-build-cache")).unwrap()
    );
}

fn started(fixture: &Fixture, selected: &str) -> OwnedChild {
    mode(fixture, selected);
    let mut command = command(fixture);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !cache(fixture).join("in-flight").exists() {
        assert!(child.try_wait().unwrap().is_none(), "producer exited early");
        assert!(Instant::now() < deadline, "producer did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    child
}

#[test]
fn killed_bootstrap_cannot_install_and_resume_retires_scratch() {
    let fixture = fixture();
    let mut child = started(&fixture, "held");
    assert!(!child.finish().unwrap().success());
    assert!(!receipt(&fixture).exists());
    mode(&fixture, "success");
    assert!(run(&fixture, &[]));
    assert!(run(&fixture, &[]));
    assert_no_scratch(&fixture);
    assert_eq!(launches(&fixture), "cargo\ncargo\n");
}

#[test]
fn scratch_retirement_preserves_redirected_and_unrecognized_entries() {
    for selected in ["directory-link", "file-link", "unexpected"] {
        let fixture = fixture();
        let outside = fixture.0.join("target/preserved");
        fs::create_dir_all(&outside).unwrap();
        let protected = outside.join("keep");
        fs::write(&protected, b"not bootstrap scratch").unwrap();
        let scratch = fixture.0.join("target/quality/hermetic-support.pending");
        fs::create_dir_all(scratch.parent().unwrap()).unwrap();
        if selected == "directory-link" {
            symlink(&outside, &scratch).unwrap();
        } else {
            fs::create_dir(&scratch).unwrap();
            if selected == "file-link" {
                symlink(&protected, scratch.join("source.tar")).unwrap();
            } else {
                fs::write(scratch.join("keep"), b"unrecognized").unwrap();
            }
        }
        assert!(!run(&fixture, &[]), "{selected}");
        assert!(launches(&fixture).is_empty(), "{selected}");
        assert!(fs::symlink_metadata(&scratch).is_ok(), "{selected}");
        assert_eq!(fs::read(&protected).unwrap(), b"not bootstrap scratch");
    }
}

#[test]
fn source_symlinks_cannot_import_inputs_outside_the_frozen_checkout() {
    let fixture = fixture();
    symlink("/etc/hosts", fixture.0.join("external-input")).unwrap();
    assert!(!run(&fixture, &[]));
    assert!(launches(&fixture).is_empty());
    assert_no_scratch(&fixture);
}

#[test]
fn source_or_tool_changes_during_execution_reject_installation() {
    for selected in ["source", "tool"] {
        let fixture = fixture();
        let before = fs::read(fixture.0.join("Cargo.toml")).unwrap();
        let mut child = started(&fixture, "paused");
        if selected == "source" {
            fs::write(fixture.0.join("Cargo.toml"), "# changed input\n").unwrap();
        } else {
            executable(
                &fixture.0.join("target/toolchain/bin/rustdoc"),
                "#!/bin/sh\necho changed-tool\n",
            );
        }
        fs::write(cache(&fixture).join("continue"), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(!status.success(), "{selected}");
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        // Compilation saw only the frozen source, but admission still refuses
        // to install it into a checkout whose source or tools have since changed.
        assert_eq!(
            before,
            fs::read(cache(&fixture).join("observed-source")).unwrap()
        );
        assert!(!fixture.0.join("target/debug/terlan-build-cache").exists());
        assert_no_scratch(&fixture);
    }
}

#[test]
fn occupied_bootstrap_lease_prevents_snapshot_and_producer_launch() {
    let fixture = fixture();
    fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
    let lock = fs::File::create(fixture.0.join("target/quality/bootstrap-owner.lock")).unwrap();
    lock.lock().unwrap();
    let mut command = command(&fixture);
    command.arg("TERLAN_BOOTSTRAP_LOCK_WAIT_SECONDS=0.05");
    let output = ProcessControl::new(Duration::from_secs(5))
        .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
        .unwrap();
    lock.unlock().unwrap();
    assert!(output.outcome.is_err());
    assert!(launches(&fixture).is_empty());
    assert_no_scratch(&fixture);
}

#[test]
fn redirected_persistent_cache_is_rejected_without_modifying_its_target() {
    let fixture = fixture();
    let protected = fixture.0.join("target/preserved");
    fs::create_dir(&protected).unwrap();
    fs::write(protected.join("keep"), "untouched").unwrap();
    symlink(&protected, fixture.0.join("target/hermetic-support")).unwrap();
    assert!(!run(&fixture, &[]));
    assert_eq!(fs::read_dir(&protected).unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(protected.join("keep")).unwrap(),
        "untouched"
    );
    assert_no_scratch(&fixture);
}

#[test]
fn redirected_scratch_parent_and_install_destination_are_rejected() {
    for relative in [
        "target/quality",
        "target/debug",
        "target/debug/terlan-build-cache",
    ] {
        let fixture = fixture();
        let protected = fixture.0.join("target/preserved");
        fs::create_dir(&protected).unwrap();
        fs::write(protected.join("keep"), "untouched").unwrap();
        let destination = fixture.0.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        if relative.ends_with("terlan-build-cache") {
            symlink(protected.join("keep"), &destination).unwrap();
        } else {
            symlink(&protected, &destination).unwrap();
        }
        assert!(!run(&fixture, &[]), "{relative}");
        assert!(fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_dir(&protected).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(protected.join("keep")).unwrap(),
            "untouched"
        );
    }
}
