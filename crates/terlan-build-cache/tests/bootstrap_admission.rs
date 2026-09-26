//! Execute the real Make bootstrap with the real admission tool and a tiny Cargo boundary.
#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn fixture() -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "terlan-admission-make-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
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
    fs::write(fixture.0.join("Cargo.lock"), "# fixture lock\n").unwrap();
    // Disk admission is the boundary under test here; hermetic support builds
    // have their own production-Make lifecycle suite.
    fs::write(
        fixture.0.join("mk/hermetic-support.mk"),
        "hermetic-support-root:\n\ttest -f target/support-ready || { ./cargo build -p terlan-test-orchestrator -p terlan-build-cache </dev/null && touch target/support-ready; }\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"fixture\"\n",
    )
    .unwrap();
    for file in [
        "crates/terlan-build-cache/Cargo.toml",
        "crates/terlan-test-orchestrator/Cargo.toml",
        "crates/terlan/Cargo.toml",
    ] {
        let target = fixture.0.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    fs::write(
        fixture.0.join(".gitignore"),
        "target/\nevents\noutput\npublication-events\n",
    )
    .unwrap();
    fs::create_dir_all(fixture.0.join("target/debug")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_terlan-build-cache"),
        fixture.0.join("target/debug/terlan-build-cache"),
    )
    .unwrap();
    executable(
        &fixture.0.join("cargo"),
        r#"#!/bin/sh
set -eu
case "$*" in
  'build -p terlan-test-orchestrator -p terlan-build-cache'|'--locked build -p terlan-test-orchestrator -p terlan-build-cache') printf 'cache-bootstrap\n' >> events ;;
  *) printf 'compiler-build\n' >> events ;;
esac
"#,
    );
    for binary in [
        "terlc",
        "terlan-vm",
        "terlan-native-worker",
        "terlan-test-orchestrator",
    ] {
        executable(
            &fixture.0.join("target/debug").join(binary),
            "#!/bin/sh\nexit 0\n",
        );
    }
    // The real owner lifecycle is tested by the orchestrator's bootstrap tests;
    // this boundary fixture isolates real disk admission and Make ordering.
    executable(
        &fixture.0.join("target/debug/terlan-test-orchestrator"),
        "#!/bin/sh\nset -eu\ncase \"$1\" in\n--install-snapshot) mkdir -p \"$(dirname \"$2\")\"; cp \"$0\" \"$2\";;\n--run-owned) shift 4; exec \"$@\";;\n*) exit 91;;\nesac\n",
    );
    fs::OpenOptions::new()
        .append(true)
        .open(fixture.0.join("Makefile"))
        .unwrap()
        .write_all(b"\nfixture-a fixture-b: terlan-compiler-bootstrap\n")
        .unwrap();
    assert!(Command::new("git")
        .current_dir(&fixture.0)
        .args(["init", "-q"])
        .status()
        .unwrap()
        .success());
    for (key, value) in [
        ("user.email", "fixture@example.invalid"),
        ("user.name", "fixture"),
    ] {
        assert!(Command::new("git")
            .current_dir(&fixture.0)
            .args(["config", key, value])
            .status()
            .unwrap()
            .success());
    }
    assert!(Command::new("git")
        .current_dir(&fixture.0)
        .args(["add", "."])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(&fixture.0)
        .args(["commit", "-qm", "fixture"])
        .status()
        .unwrap()
        .success());
    fixture
}

fn run(fixture: &Fixture, floor: &str, prebuilt: bool) -> (bool, String) {
    let output = fs::File::create(fixture.0.join("output")).unwrap();
    let mut command = Command::new("make");
    command
        .current_dir(&fixture.0)
        .args([
            "--no-print-directory",
            "-j4",
            "fixture-a",
            "fixture-b",
            "CARGO=./cargo",
        ])
        .arg(format!("TERLAN_BUILD_MINIMUM_FREE_BYTES={floor}"))
        .stdin(Stdio::null())
        .stdout(output.try_clone().unwrap())
        .stderr(output);
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
    if prebuilt {
        command.arg("TERLAN_BUILD_ARTIFACTS_PREBUILT=1");
    }
    let mut child = Running(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            return (
                status.success(),
                fs::read_to_string(fixture.0.join("output")).unwrap(),
            );
        }
        assert!(Instant::now() < deadline, "Make admission exceeded 30s");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn insufficient_space_stops_real_make_before_compiler_launch() {
    let fixture = fixture();
    let (passed, output) = run(&fixture, "18446744073709551615", false);
    assert!(!passed, "{output}");
    assert!(output.contains("error[build.resources]"), "{output}");
    assert_eq!(
        fs::read_to_string(fixture.0.join("events")).unwrap(),
        "cache-bootstrap\n"
    );
}

#[test]
fn unsafe_cache_namespace_stops_real_make_before_compiler_launch() {
    let fixture = fixture();
    std::os::unix::fs::symlink(&fixture.0, fixture.0.join("target/debug/incremental")).unwrap();
    let (passed, output) = run(&fixture, "1", false);
    assert!(!passed, "{output}");
    assert!(
        output.contains("unsafe incremental cache layout"),
        "{output}"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("events")).unwrap(),
        "cache-bootstrap\n"
    );
    assert!(fixture.0.join("Cargo.toml").is_file());
}

#[test]
fn adequate_space_admits_one_shared_bootstrap_and_prebuilt_use_replays_nothing() {
    let fixture = fixture();
    let (passed, output) = run(&fixture, "1", false);
    assert!(passed, "{output}");
    assert!(
        output.contains("terlan.build-resource-admission.v1"),
        "{output}"
    );
    let expected = "cache-bootstrap\ncompiler-build\n";
    assert_eq!(
        fs::read_to_string(fixture.0.join("events")).unwrap(),
        expected
    );
    // Keep the production absolute snapshot path, and verify real receipt reuse
    // without a prebuilt flag before checking the explicit prebuilt route.
    let (passed, output) = run(&fixture, "1", false);
    assert!(passed, "{output}");
    assert!(output.contains("\"decision\":\"reused\""), "{output}");
    assert_eq!(
        fs::read_to_string(fixture.0.join("events")).unwrap(),
        expected
    );
    let (passed, output) = run(&fixture, "18446744073709551615", true);
    assert!(passed, "{output}");
    assert!(
        !output.contains("terlan.build-resource-admission.v1"),
        "{output}"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("events")).unwrap(),
        expected
    );
}
