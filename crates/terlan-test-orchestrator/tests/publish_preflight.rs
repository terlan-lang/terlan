//! Run the production publication preflight against disposable local Git remotes.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture {
    root: PathBuf,
    git: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.git);
        command
            .current_dir(self.root.join("checkout"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }

    fn git(&self, arguments: &[&str]) {
        assert!(self.command().args(arguments).status().unwrap().success());
    }

    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "terlan-publish-preflight-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|path| path.join("git"))
            .find(|path| path.is_file())
            .unwrap();
        let fixture = Self { root, git };
        fs::create_dir(fixture.root.join("checkout")).unwrap();
        fixture.git(&["init", "--quiet", "--initial-branch=main"]);
        fixture.git(&["config", "user.name", "Preflight Fixture"]);
        fixture.git(&["config", "user.email", "fixture@example.invalid"]);
        fixture.git(&[
            "commit",
            "--quiet",
            "--allow-empty",
            "--no-gpg-sign",
            "-m",
            "fixture",
        ]);
        fixture.git(&["init", "--quiet", "--bare", "../remote.git"]);
        fixture.git(&["remote", "add", "origin", "../remote.git"]);
        fixture.git(&["push", "--quiet", "origin", "main"]);
        fs::create_dir(fixture.root.join("bin")).unwrap();
        executable(&fixture.root.join("bin/gh"), "#!/bin/sh\nexit 0\n");
        executable(
            &fixture.root.join("bin/git"),
            r#"#!/bin/sh
if [ "$1" = status ]; then
    printf '%s\n' "$*" >> "$PREFLIGHT_STATUS_CALLS"
    if [ "$PREFLIGHT_FAULT" = status ]; then
        exit 128
    fi
fi
if [ "$1" = ls-remote ]; then
    printf '%s\n' "$*" >> "$PREFLIGHT_CALLS"
    if [ "$PREFLIGHT_FAULT" = network ]; then
        echo 'fixture transport unavailable' >&2
        exit 128
    fi
fi
if [ "$PREFLIGHT_FAULT" = fetch ] && [ "$1" = fetch ] && [ "$4" != main ]; then
    echo 'fixture tag fetch unavailable' >&2
    exit 128
fi
if [ "$PREFLIGHT_FAULT" = local-ref ] && [ "$1" = show-ref ]; then
    exit 128
fi
exec "$PREFLIGHT_REAL_GIT" "$@"
"#,
        );
        let source =
            fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile"))
                .unwrap();
        let begin = source.find("\npublish-source-preflight:\n").unwrap() + 1;
        let end = source[begin..].find("\n# Preparation may build").unwrap() + begin;
        fs::write(fixture.root.join("Makefile"), &source[begin..end]).unwrap();
        fixture
    }

    fn tag(&self, message: &str) {
        self.git(&[
            "-c",
            "tag.gpgSign=false",
            "tag",
            "-a",
            "vfixture",
            "-m",
            message,
        ]);
    }

    fn remote_tag(&self) {
        self.git(&["push", "--quiet", "origin", "refs/tags/vfixture"]);
    }

    fn preflight(&self, fault: &str) -> bool {
        self.run_preflight(fault, 1)
    }

    fn run_preflight(&self, fault: &str, expected_queries: usize) -> bool {
        let mut paths = vec![self.root.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let calls = self.root.join("queries");
        let status_calls = self.root.join("status-queries");
        let status_before = fs::read_to_string(&status_calls)
            .unwrap_or_default()
            .lines()
            .count();
        let before = fs::read_to_string(&calls)
            .unwrap_or_default()
            .lines()
            .count();
        let mut command = Command::new("make");
        command
            .current_dir(self.root.join("checkout"))
            .args(["--no-print-directory", "-f"])
            .arg(self.root.join("Makefile"))
            .args(["VERSION=fixture", "publish-remote-preflight"])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("PREFLIGHT_REAL_GIT", &self.git)
            .env("PREFLIGHT_CALLS", &calls)
            .env("PREFLIGHT_STATUS_CALLS", &status_calls)
            .env("PREFLIGHT_FAULT", fault)
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS");
        let result = ProcessControl::new(Duration::from_secs(30)).run(&mut command, |_| Ok(()));
        assert_eq!(
            fs::read_to_string(status_calls).unwrap().lines().count(),
            status_before + 1,
            "one source checkout observation per preflight"
        );
        let queries = fs::read_to_string(calls).unwrap_or_default();
        assert_eq!(
            queries.lines().count(),
            before + expected_queries,
            "one remote snapshot per preflight"
        );
        if expected_queries > 0 {
            assert_eq!(
                queries.lines().last().unwrap(),
                "ls-remote --tags origin refs/tags/vfixture refs/tags/vfixture^{}"
            );
        }
        result.is_ok()
    }
}

fn executable(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn publication_preflight_uses_one_remote_snapshot_and_rejects_unknown_state() {
    let absent = Fixture::new();
    assert!(!absent.run_preflight("status", 0));
    assert!(absent.preflight(""));
    assert!(!absent.preflight("network"));
    assert!(!absent.preflight("local-ref"));
    absent.tag("candidate");
    assert!(absent.preflight(""), "local-only annotated tag");
    absent.remote_tag();
    assert!(absent.preflight(""), "matching retry");
    assert!(
        !absent.preflight("network"),
        "local tag cannot mask transport failure"
    );
    absent.git(&["tag", "-d", "vfixture"]);
    assert!(!absent.preflight("fetch"));
    assert!(absent.preflight(""), "fetch matching remote-only tag");

    let lightweight = Fixture::new();
    lightweight.git(&["tag", "vfixture"]);
    assert!(!lightweight.preflight(""));
    lightweight.remote_tag();
    assert!(!lightweight.preflight(""));

    let changed = Fixture::new();
    changed.tag("original");
    changed.remote_tag();
    changed.git(&["tag", "-d", "vfixture"]);
    changed.tag("different annotation");
    assert!(
        !changed.preflight(""),
        "different tag object at same commit"
    );

    let stale = Fixture::new();
    stale.tag("old candidate");
    stale.remote_tag();
    stale.git(&[
        "commit",
        "--quiet",
        "--allow-empty",
        "--no-gpg-sign",
        "-m",
        "next candidate",
    ]);
    assert!(!stale.preflight(""), "tag does not identify HEAD");

    let dirty = Fixture::new();
    fs::write(dirty.root.join("checkout/unreviewed.txt"), "unreviewed").unwrap();
    assert!(!dirty.run_preflight("", 0));
}

#[test]
fn interrupted_upload_retry_runs_no_preparation_work() {
    let root = std::env::temp_dir().join(format!(
        "terlan-publish-retry-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    fs::create_dir(root.join("bin")).unwrap();
    executable(
        &root.join("bin/git"),
        r#"#!/bin/sh
printf 'git %s\n' "$*" >> "$PUBLISH_RETRY_LOG"
if [ "$1" = rev-parse ]; then
    exit 1
fi
exit 0
"#,
    );
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let begin = source.find("\npublish: publish-preflight\n").unwrap() + 1;
    let end = source[begin..]
        .find("\npublish-release-from-dist:")
        .unwrap()
        + begin;
    let mut makefile = String::from("SHELL := /bin/bash\nVERSION := fixture\n");
    makefile.push_str(".PHONY: publish-preflight publish-release-from-dist publish\n");
    makefile.push_str("publish-preflight:\n\t@printf 'verify\\n' >> \"$(PUBLISH_RETRY_LOG)\"\n");
    makefile.push_str(
        "publish-release-from-dist:\n\t@printf 'upload\\n' >> \"$(PUBLISH_RETRY_LOG)\"\n\t@if [ ! -e \"$(PUBLISH_RETRY_FAILED)\" ]; then : > \"$(PUBLISH_RETRY_FAILED)\"; exit 1; fi\n\t@printf 'promotion\\n' >> \"$(PUBLISH_RETRY_LOG)\"\n",
    );
    makefile.push_str(&source[begin..end]);
    fs::write(root.join("Makefile"), makefile).unwrap();
    let log = root.join("operations");
    let failed = root.join("failed-upload");
    let mut paths = vec![root.join("bin")];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let run = |log: &Path, failed: &Path| {
        let mut command = Command::new("make");
        command
            .current_dir(&root)
            .args(["--no-print-directory", "publish"])
            .env("PATH", std::env::join_paths(&paths).unwrap())
            .env("PUBLISH_RETRY_LOG", log)
            .env("PUBLISH_RETRY_FAILED", failed)
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS");
        ProcessControl::new(Duration::from_secs(30)).run(&mut command, |_| Ok(()))
    };
    assert!(
        run(&log, &failed).is_err(),
        "first upload must be interrupted"
    );
    assert!(
        run(&log, &failed).is_ok(),
        "retry should promote the prepared candidate"
    );
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "verify\ngit rev-parse -q --verify refs/tags/vfixture\ngit tag --annotate vfixture --message Terlan vfixture\ngit push origin main\ngit push origin vfixture\nupload\nverify\ngit rev-parse -q --verify refs/tags/vfixture\ngit tag --annotate vfixture --message Terlan vfixture\ngit push origin main\ngit push origin vfixture\nupload\npromotion\n"
    );
}

#[test]
fn publication_asset_upload_has_outer_deadline() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let target = source
        .split_once("\npublish-release-from-dist:\n")
        .and_then(|(_, rest)| rest.split_once('\n'))
        .map(|(recipe, _)| recipe)
        .expect("publication target must define a recipe");
    assert!(
        target.contains("timeout 900s bash scripts/publish_release_from_dist.sh"),
        "publication asset upload must be bounded by an outer timeout"
    );
}

#[test]
fn publication_retry_path_contains_no_preparation_or_refresh_targets() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\npublish: publish-preflight\n")
        .expect("publish target must exist")
        + 1;
    let end = source[start..]
        .find("\npublish-release-from-dist:")
        .expect("publish target must end before upload target")
        + start;
    let target = &source[start..end];
    for forbidden in [
        "publish-prepare",
        "publish-evidence-refresh",
        "publish-preparation-cold",
        "publish-preparation-warm",
        "cargo ",
        "terlc test",
    ] {
        assert!(
            !target.contains(forbidden),
            "publication retry path must not replay {forbidden}"
        );
    }
    assert!(target.contains("publish-preflight"));
    assert!(target.contains("publish-release-from-dist"));
}

#[test]
fn publication_verification_commands_have_outer_deadlines() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    assert!(source
        .contains("timeout 300s $(TERLAN_RELEASE_PROMOTION) preflight --version \"$(VERSION)\""));
    assert!(source.contains("timeout 300s $(TERLAN_TVM_PLATFORM_MATRIX) release-verify"));
    assert!(source.contains("timeout 300s $(TERLAN_TVM_PLATFORM_MATRIX) multicore-release-verify"));
}
