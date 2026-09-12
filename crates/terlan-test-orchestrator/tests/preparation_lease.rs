//! Exercise the actual Make lease wrapper, including inherited kernel locks.
#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Active(PathBuf);

impl Drop for Active {
    fn drop(&mut self) {
        fs::remove_file(&self.0).unwrap();
    }
}

#[test]
fn lease_child() {
    let Some(root) = std::env::var_os("TERLAN_LEASE_FIXTURE").map(PathBuf::from) else {
        return;
    };
    let stage = std::env::var("TERLAN_LEASE_STAGE").unwrap();
    let named = fs::metadata(root.join("target/quality/preparation.lock")).unwrap();
    let inherited = fs::metadata("/proc/self/fd/9").unwrap();
    assert_eq!(
        (named.dev(), named.ino()),
        (inherited.dev(), inherited.ino())
    );
    let separate = File::open(root.join("target/quality/preparation.lock")).unwrap();
    assert!(
        separate.try_lock().is_err(),
        "outer scope must remain locked"
    );
    let active = root.join("active-owner");
    File::create_new(&active).expect("sibling owners must not overlap");
    let _active = Active(active);
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("events"))
        .unwrap()
        .write_all(format!("{stage}\n").as_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_ne!(
        std::env::var("TERLAN_LEASE_FAIL").ok().as_deref(),
        Some(stage.as_str())
    );
}

fn fixture() -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "terlan-preparation-lease-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let fixture = Fixture(path);
    fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let logical = source.replace("\\\n", " ");
    let wrapper = logical
        .lines()
        .find(|line| line.starts_with("TERLAN_PREPARATION_OWNER = "))
        .unwrap();
    fs::write(fixture.0.join("Makefile"), format!("{wrapper}\n.PHONY: a b\na b:\n\t@TERLAN_LEASE_STAGE=$@ $(TERLAN_PREPARATION_OWNER) \"$(FIXTURE_CHILD)\" --exact lease_child --nocapture\n")).unwrap();
    fixture
}

fn run(root: &Path, shell: &str) -> bool {
    let mut command = Command::new("/bin/sh");
    command
        .current_dir(root)
        .args(["-ec", shell])
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS")
        .env_remove("TERLAN_PREPARATION_LOCK_HELD")
        .env_remove("TERLAN_LEASE_FAIL")
        .env("TERLAN_LEASE_FIXTURE", root)
        .env("FIXTURE_CHILD", std::env::current_exe().unwrap());
    ProcessControl::new(Duration::from_secs(5))
        .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
        .is_ok_and(|output| output.outcome.is_ok())
}

fn released(root: &Path) {
    for name in ["preparation.lock", "preparation-owner.lock"] {
        let file = File::open(root.join("target/quality").join(name)).unwrap();
        file.try_lock()
            .expect("terminated invocation must release both leases");
    }
    assert!(!root.join("active-owner").exists());
}

#[test]
fn standalone_parallel_owners_are_serialized() {
    let fixture = fixture();
    assert!(run(&fixture.0, "make --no-print-directory -j8 a b"));
    let mut events = fs::read_to_string(fixture.0.join("events"))
        .unwrap()
        .lines()
        .map(String::from)
        .collect::<Vec<_>>();
    events.sort();
    assert_eq!(events, ["a", "b"]);
    released(&fixture.0);
}

#[test]
fn inherited_outer_lease_survives_parallel_children() {
    let fixture = fixture();
    assert!(run(
        &fixture.0,
        r#"
exec 9>target/quality/preparation.lock
flock --exclusive 9
export TERLAN_PREPARATION_LOCK_HELD=1
make --no-print-directory -j8 a b
if flock --nonblock target/quality/preparation.lock /bin/true; then exit 90; fi
"#
    ));
    assert_eq!(
        fs::read_to_string(fixture.0.join("events"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    released(&fixture.0);
}

#[test]
fn missing_wrong_or_invalid_inherited_scope_never_runs_a_producer() {
    for setup in [
        "exec 9>&-; export TERLAN_PREPARATION_LOCK_HELD=1",
        "exec 9>other.lock; flock --exclusive 9; export TERLAN_PREPARATION_LOCK_HELD=1",
        "export TERLAN_PREPARATION_LOCK_HELD=invalid",
    ] {
        let fixture = fixture();
        assert!(!run(
            &fixture.0,
            &format!("{setup}\nmake --no-print-directory a")
        ));
        assert!(!fixture.0.join("events").exists());
    }
}

#[test]
fn symlinked_lock_is_rejected_without_modifying_its_target() {
    let fixture = fixture();
    fs::write(fixture.0.join("retained"), "preserved").unwrap();
    symlink(
        fixture.0.join("retained"),
        fixture.0.join("target/quality/preparation.lock"),
    )
    .unwrap();
    assert!(!run(&fixture.0, "make --no-print-directory a"));
    assert_eq!(
        fs::read_to_string(fixture.0.join("retained")).unwrap(),
        "preserved"
    );
    assert!(!fixture.0.join("events").exists());
}

#[test]
fn failed_child_does_not_release_outer_scope_or_block_the_next_owner() {
    let fixture = fixture();
    assert!(run(
        &fixture.0,
        r#"
exec 9>target/quality/preparation.lock
flock --exclusive 9
export TERLAN_PREPARATION_LOCK_HELD=1
if TERLAN_LEASE_FAIL=a make --no-print-directory a; then exit 90; fi
if flock --nonblock target/quality/preparation.lock /bin/true; then exit 91; fi
make --no-print-directory b
"#
    ));
    assert_eq!(
        fs::read_to_string(fixture.0.join("events")).unwrap(),
        "a\nb\n"
    );
    released(&fixture.0);
}

#[test]
fn foreign_outer_lease_is_not_bypassed() {
    let fixture = fixture();
    let file = File::create(fixture.0.join("target/quality/preparation.lock")).unwrap();
    file.lock().unwrap();
    assert!(!run(&fixture.0, "make --no-print-directory a"));
    assert!(!fixture.0.join("events").exists());
    let probe = File::open(fixture.0.join("target/quality/preparation.lock")).unwrap();
    assert!(
        probe.try_lock().is_err(),
        "timeout must preserve another owner's lease"
    );
}
