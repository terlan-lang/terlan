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

fn promotion_fixture() -> Fixture {
    let fixture = fixture();
    fixture.git(&["tag", "-d", "vfixture"]);
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source.find("\npublish:\n").unwrap() + 1;
    let end = start
        + source[start..]
            .find("\npublish-release-from-dist:")
            .unwrap();
    let preflight = r#"
.PHONY: publish publish-preflight
publish-preflight:
	@printf 'preflight\n' >> "$$PUBLICATION_CALLS"
	@test "$$TERLAN_PREPARATION_LOCK_HELD" = 1
	@test /dev/fd/7 -ef .git/terlan-publication.lock
	@test /dev/fd/8 -ef target/publication-inputs.lock
	@test /dev/fd/9 -ef target/quality/preparation.lock
	@for lock in .git/terlan-publication.lock target/publication-inputs.lock target/quality/preparation.lock; do \
		if flock --nonblock "$$lock" /bin/true; then exit 92; fi; \
	done
	@if read value; then exit 91; fi
	@test "$$PUBLICATION_MODE" != preflight-failure
"#;
    fs::write(
        fixture.root.join("checkout/Makefile"),
        format!("SHELL := /bin/bash\n{}{preflight}", &source[start..end]),
    )
    .unwrap();
    fixture
}

fn promote(fixture: &Fixture, mode: &str, dry_run: bool) -> Result<(), Failure> {
    let mut shell = command(fixture, "checkout", mode);
    for key in [
        "MAKEFLAGS",
        "MAKEOVERRIDES",
        "MFLAGS",
        "GNUMAKEFLAGS",
        "MAKEFILES",
    ] {
        shell.env_remove(key);
    }
    shell.args([
        "-ec",
        if dry_run {
            "exec make --no-print-directory -n publish VERSION=fixture"
        } else {
            "exec make --no-print-directory publish VERSION=fixture"
        },
    ]);
    ProcessControl::new(Duration::from_secs(15)).run(&mut shell, |_| Ok(()))
}

fn remote_tag(fixture: &Fixture) -> Option<String> {
    let output = fixture
        .command()
        .args([
            "--git-dir=../remote.git",
            "rev-parse",
            "--verify",
            "refs/tags/vfixture",
        ])
        .output()
        .unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap())
}

#[test]
fn make_publication_takes_all_owners_before_preflight_or_tagging() {
    for lock in [
        ".git/terlan-publication.lock",
        "target/quality/preparation.lock",
        "target/publication-inputs.lock",
    ] {
        let fixture = promotion_fixture();
        let file = File::create(fixture.root.join("checkout").join(lock)).unwrap();
        file.lock().unwrap();
        assert!(promote(&fixture, "failure", false).is_err());
        assert!(!fixture.root.join("publisher-calls").exists());
        assert!(remote_tag(&fixture).is_none());
    }
}

#[test]
fn make_publication_preflight_failure_cannot_tag_or_upload() {
    let fixture = promotion_fixture();
    assert!(promote(&fixture, "preflight-failure", false).is_err());
    assert_eq!(
        fs::read_to_string(fixture.root.join("publisher-calls")).unwrap(),
        "preflight\n"
    );
    assert!(remote_tag(&fixture).is_none());
    assert!(!fixture
        .root
        .join("checkout/.git/refs/tags/vfixture")
        .exists());
}

#[test]
fn make_publication_keeps_tag_identity_on_retry_and_holds_preflight_leases() {
    let fixture = promotion_fixture();
    assert!(promote(&fixture, "failure", false).is_err());
    let tag = remote_tag(&fixture).expect("fixture preflight admitted the tag");
    assert!(promote(&fixture, "failure", false).is_err());
    assert_eq!(remote_tag(&fixture).unwrap(), tag);
    assert_eq!(
        fs::read_to_string(fixture.root.join("publisher-calls")).unwrap(),
        "preflight\ngh\nplan\npreflight\ngh\nplan\n"
    );
    assert!(!fixture.root.join("checkout").join(SCRATCH).exists());
}

#[test]
fn make_publication_dry_run_never_takes_owners_or_creates_tags() {
    let fixture = promotion_fixture();
    promote(&fixture, "failure", true).unwrap();
    assert!(!fixture.root.join("publisher-calls").exists());
    assert!(remote_tag(&fixture).is_none());
    assert!(!fixture
        .root
        .join("checkout/.git/terlan-publication.lock")
        .exists());
}
