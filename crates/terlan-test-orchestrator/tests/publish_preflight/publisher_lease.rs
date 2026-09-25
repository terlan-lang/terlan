//! Publication entry-point ownership tested with real private Git repositories
//! and a fail-closed local GitHub stand-in. No remote publication is possible.

use super::{executable, Fixture};
use std::fs::{self, File};
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use terlan_process_owner::{Failure, ProcessControl};

const SCRATCH: &str = "target/quality/publication.pending";

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.tag("publication fixture");
    prepare_checkout(&fixture, "checkout");
    executable(
        &fixture.root.join("bin/gh"),
        r#"#!/bin/sh
set -eu
printf 'gh\n' >> "$PUBLICATION_CALLS"
case "$1 $2" in
  'auth status') exit 0 ;;
  'repo view') printf 'fixture/repository\n'; exit 0 ;;
  *) exit 91 ;;
esac
"#,
    );
    fixture
}

fn prepare_checkout(fixture: &Fixture, name: &str) {
    let root = fixture.root.join(name);
    for path in ["scripts", "target/debug", "target/quality"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/publish_release_from_dist.sh"),
        root.join("scripts/publish_release_from_dist.sh"),
    )
    .unwrap();
    executable(
        &root.join("target/debug/terlan-vm"),
        r#"#!/bin/sh
set -eu
printf 'plan\n' >> "$PUBLICATION_CALLS"
if test "$PUBLICATION_MODE" = killed; then kill -KILL "$PPID"; exit 0; fi
if test "$PUBLICATION_MODE" = terminated; then kill -TERM "$PPID"; exit 0; fi
exit 23
"#,
    );
}

fn command(fixture: &Fixture, checkout: &str, mode: &str) -> Command {
    let mut search = vec![fixture.root.join("bin")];
    search.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let mut command = Command::new("bash");
    command
        .current_dir(fixture.root.join(checkout))
        .env("PATH", std::env::join_paths(search).unwrap())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PREFLIGHT_REAL_GIT", &fixture.git)
        .env("PREFLIGHT_FAULT", "")
        .env("PUBLICATION_CALLS", fixture.root.join("publisher-calls"))
        .env("PUBLICATION_MODE", mode)
        .env_remove("TERLAN_PREPARATION_LOCK_HELD");
    command
}

fn run(fixture: &Fixture, checkout: &str, mode: &str) -> Result<(), Failure> {
    ProcessControl::new(Duration::from_secs(15)).run(
        command(fixture, checkout, mode).args(["scripts/publish_release_from_dist.sh", "fixture"]),
        |_| Ok(()),
    )
}

#[test]
fn active_candidate_or_publication_owner_prevents_any_network_or_verification() {
    for lock in [
        ".git/terlan-publication.lock",
        "target/quality/preparation.lock",
        "target/publication-inputs.lock",
    ] {
        let fixture = fixture();
        let file = File::create(fixture.root.join("checkout").join(lock)).unwrap();
        file.lock().unwrap();
        assert!(run(&fixture, "checkout", "failure").is_err());
        assert!(
            !fixture.root.join("publisher-calls").exists(),
            "ignored owner {lock}"
        );
    }
}

#[test]
fn normal_failure_releases_all_leases_and_retires_scratch() {
    let fixture = fixture();
    let failure = run(&fixture, "checkout", "failure").unwrap_err();
    assert!(failure.detail.contains("23"), "{failure:?}");
    assert!(!fixture.root.join("checkout").join(SCRATCH).exists());
    for lock in [
        ".git/terlan-publication.lock",
        "target/quality/preparation.lock",
        "target/publication-inputs.lock",
    ] {
        File::open(fixture.root.join("checkout").join(lock))
            .unwrap()
            .try_lock()
            .unwrap();
    }
}

#[test]
fn repository_publication_lease_is_shared_by_git_worktrees() {
    let fixture = fixture();
    fixture.git(&["worktree", "add", "--quiet", "--detach", "../other", "HEAD"]);
    prepare_checkout(&fixture, "other");
    let file = File::create(fixture.root.join("checkout/.git/terlan-publication.lock")).unwrap();
    file.lock().unwrap();
    assert!(run(&fixture, "other", "failure").is_err());
    assert!(!fixture.root.join("publisher-calls").exists());
}

#[test]
fn publisher_borrows_the_real_outer_lease_without_releasing_it() {
    let fixture = fixture();
    let mut shell = command(&fixture, "checkout", "failure");
    shell.args([
        "-ec",
        r#"
exec 9>>target/quality/preparation.lock
flock --exclusive 9
export TERLAN_PREPARATION_LOCK_HELD=1
if bash scripts/publish_release_from_dist.sh fixture; then exit 90; else status=$?; fi
test "$status" = 23
if flock --nonblock target/quality/preparation.lock /bin/true; then exit 91; fi
"#,
    ]);
    ProcessControl::new(Duration::from_secs(15))
        .run(&mut shell, |_| Ok(()))
        .unwrap();
    File::open(
        fixture
            .root
            .join("checkout/target/quality/preparation.lock"),
    )
    .unwrap()
    .try_lock()
    .unwrap();
    assert!(!fixture.root.join("checkout").join(SCRATCH).exists());
}

#[test]
fn false_or_missing_inherited_lease_is_rejected_before_network_access() {
    for setup in [
        "export TERLAN_PREPARATION_LOCK_HELD=invalid",
        "exec 9>&-; export TERLAN_PREPARATION_LOCK_HELD=1",
        "exec 9>foreign.lock; export TERLAN_PREPARATION_LOCK_HELD=1",
    ] {
        let fixture = fixture();
        let mut shell = command(&fixture, "checkout", "failure");
        shell.args([
            "-ec",
            &format!("{setup}\nbash scripts/publish_release_from_dist.sh fixture"),
        ]);
        assert!(ProcessControl::new(Duration::from_secs(15))
            .run(&mut shell, |_| Ok(()))
            .is_err());
        assert!(!fixture.root.join("publisher-calls").exists());
    }
}

#[test]
fn killed_publisher_resumes_without_anonymous_scratch_or_stale_leases() {
    let fixture = fixture();
    let failure = run(&fixture, "checkout", "killed").unwrap_err();
    assert!(failure.detail.contains("SIGKILL"), "{failure:?}");
    assert!(fixture.root.join("checkout").join(SCRATCH).is_dir());
    let failure = run(&fixture, "checkout", "failure").unwrap_err();
    assert!(failure.detail.contains("23"), "{failure:?}");
    assert!(!fixture.root.join("checkout").join(SCRATCH).exists());
}

#[test]
fn terminated_publisher_retires_scratch() {
    let fixture = fixture();
    let failure = run(&fixture, "checkout", "terminated").unwrap_err();
    assert!(failure.detail.contains("143"), "{failure:?}");
    assert!(!fixture.root.join("checkout").join(SCRATCH).exists());
}

#[test]
fn redirected_leases_preserve_their_targets() {
    for lock in [
        ".git/terlan-publication.lock",
        "target/quality/preparation.lock",
        "target/publication-inputs.lock",
    ] {
        let fixture = fixture();
        let retained = fixture.root.join("retained");
        fs::write(&retained, "retained contents").unwrap();
        symlink(&retained, fixture.root.join("checkout").join(lock)).unwrap();
        assert!(run(&fixture, "checkout", "failure").is_err());
        assert_eq!(fs::read_to_string(retained).unwrap(), "retained contents");
        assert!(!fixture.root.join("publisher-calls").exists());
    }
}

#[test]
fn unknown_scratch_entries_are_preserved_without_contacting_github() {
    let fixture = fixture();
    let scratch = fixture.root.join("checkout").join(SCRATCH);
    fs::create_dir(&scratch).unwrap();
    fs::write(scratch.join("unknown"), "preserved").unwrap();
    assert!(run(&fixture, "checkout", "failure").is_err());
    assert_eq!(
        fs::read_to_string(scratch.join("unknown")).unwrap(),
        "preserved"
    );
    assert!(!fixture.root.join("publisher-calls").exists());
}
