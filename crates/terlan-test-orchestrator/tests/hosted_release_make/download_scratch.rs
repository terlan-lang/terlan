//! Exercise the real hosted downloader's temporary-output ownership without
//! contacting GitHub or modifying a release, using an explicitly failing CLI.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::{Failure, ProcessControl};

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const SCRATCH: &str = "target/publication-downloads/.partial.dlwork";

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-download-scratch-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let fixture = Fixture(root);
    for path in [
        "scripts",
        "bin",
        "tmp",
        "target/publication-downloads",
        "target/quality",
    ] {
        fs::create_dir_all(fixture.0.join(path)).unwrap();
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::copy(
        repository.join("scripts/download_validated_release_artifacts.sh"),
        fixture
            .0
            .join("scripts/download_validated_release_artifacts.sh"),
    )
    .unwrap();
    fs::write(
        fixture.0.join("target/publication-downloads/verified"),
        "keep cache",
    )
    .unwrap();
    fs::write(fixture.0.join("target/quality/evidence"), "keep evidence").unwrap();
    let cli = fixture.0.join("bin/gh");
    fs::write(
        &cli,
        r#"#!/bin/sh
set -eu
case "$1 $2" in
  'auth status') exit 0 ;;
  'repo view') printf 'fixture/repository\n'; exit 0 ;;
esac
case "$2" in
  */commits/*/status)
    printf '{"state":"success","target_url":"https://github.com/fixture/repository/actions/runs/123"}\n'
    ;;
  */actions/runs/123)
    if test "$DOWNLOAD_MODE" = killed; then kill -KILL "$PPID"; exit 0; fi
    if test "$DOWNLOAD_MODE" = terminated; then kill -TERM "$PPID"; exit 0; fi
    exit 41
    ;;
  *) exit 88 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(cli, fs::Permissions::from_mode(0o700)).unwrap();
    fixture
}

fn command(fixture: &Fixture, mode: &str) -> Command {
    let mut search = vec![fixture.0.join("bin")];
    search.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let mut command = Command::new("bash");
    command
        .current_dir(&fixture.0)
        .args(["scripts/download_validated_release_artifacts.sh", REVISION])
        .env("PATH", std::env::join_paths(search).unwrap())
        .env("TMPDIR", fixture.0.join("tmp"))
        .env("DOWNLOAD_MODE", mode);
    command
}

fn run(fixture: &Fixture, mode: &str) -> Result<(), Failure> {
    ProcessControl::new(Duration::from_secs(15)).run(&mut command(fixture, mode), |_| Ok(()))
}

fn assert_preserved(fixture: &Fixture) {
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/publication-downloads/verified")).unwrap(),
        "keep cache"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("target/quality/evidence")).unwrap(),
        "keep evidence"
    );
}

#[test]
fn killed_download_resumes_under_the_same_owner_and_retires_scratch() {
    let fixture = fixture();
    let failure = run(&fixture, "killed").unwrap_err();
    assert!(failure.detail.contains("SIGKILL"), "{failure:?}");
    assert!(
        fixture.0.join(SCRATCH).is_dir(),
        "interrupted scratch must be discoverable"
    );
    assert_eq!(fs::read_dir(fixture.0.join("tmp")).unwrap().count(), 0);
    assert!(run(&fixture, "failure").is_err());
    assert!(!fixture.0.join(SCRATCH).exists());
    assert_preserved(&fixture);
}

#[test]
fn ordinary_download_failure_retires_scratch_without_removing_verified_inputs() {
    let fixture = fixture();
    assert!(run(&fixture, "failure").is_err());
    assert!(!fixture.0.join(SCRATCH).exists());
    assert_eq!(fs::read_dir(fixture.0.join("tmp")).unwrap().count(), 0);
    assert_preserved(&fixture);
}

#[test]
fn download_owner_preserves_unknown_scratch_entries() {
    let fixture = fixture();
    fs::create_dir(fixture.0.join(SCRATCH)).unwrap();
    let unknown = fixture.0.join(SCRATCH).join("unrecognized");
    fs::write(&unknown, "keep unknown").unwrap();
    assert!(run(&fixture, "failure").is_err());
    assert_eq!(fs::read_to_string(unknown).unwrap(), "keep unknown");
    assert_preserved(&fixture);
}

#[test]
fn download_owner_rejects_redirected_scratch_without_touching_its_target() {
    let fixture = fixture();
    let protected = fixture.0.join("protected");
    fs::create_dir(&protected).unwrap();
    fs::write(protected.join("keep"), "untouched").unwrap();
    symlink(&protected, fixture.0.join(SCRATCH)).unwrap();
    assert!(run(&fixture, "failure").is_err());
    assert_eq!(
        fs::read_to_string(protected.join("keep")).unwrap(),
        "untouched"
    );
    assert!(fs::symlink_metadata(fixture.0.join(SCRATCH))
        .unwrap()
        .is_symlink());
    assert_preserved(&fixture);
}

#[test]
fn terminated_download_retires_scratch() {
    let fixture = fixture();
    let failure = run(&fixture, "terminated").unwrap_err();
    assert!(failure.detail.contains("143"), "{failure:?}");
    assert!(!fixture.0.join(SCRATCH).exists());
    assert_preserved(&fixture);
}

#[test]
fn verified_input_restore_retires_interrupted_scratch() {
    let fixture = fixture();
    fs::create_dir(fixture.0.join("dist")).unwrap();
    let inputs = fixture.0.join("target/publication-inputs").join(REVISION);
    fs::create_dir_all(&inputs).unwrap();
    let payloads = [
        "terlc-linux-x86_64.tar.gz",
        "terlc",
        "terlan-vm",
        "terlan-native-worker",
        "terlan-lsp",
        "terlan-release.json",
        "SHA256SUMS",
        "terlan-install-manifest.json",
    ];
    for name in &payloads {
        fs::write(inputs.join(name), name).unwrap();
    }
    let mut checksum = Command::new("sha256sum");
    checksum.current_dir(&inputs).args(payloads);
    let sums = ProcessControl::new(Duration::from_secs(10))
        .capture_stdout(&mut checksum, 8192, |_| Ok(()))
        .unwrap();
    fs::write(inputs.join("verified-inputs.sha256"), &sums).unwrap();
    let git = fixture.0.join("bin/git");
    fs::write(&git, format!("#!/bin/sh\nprintf '%s\\n' '{REVISION}'\n")).unwrap();
    fs::set_permissions(git, fs::Permissions::from_mode(0o700)).unwrap();
    for child in ["downloads", "extracted", "evidence", "cache"] {
        let directory = fixture.0.join(SCRATCH).join(child);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("partial"), "interrupted bytes").unwrap();
    }
    let mut restore = command(&fixture, "must-not-contact-github");
    restore.arg("--restore");
    ProcessControl::new(Duration::from_secs(15))
        .run(&mut restore, |_| Ok(()))
        .unwrap();
    assert!(!fixture.0.join(SCRATCH).exists());
    for name in payloads {
        assert_eq!(
            fs::read_to_string(fixture.0.join("dist").join(name)).unwrap(),
            name
        );
        assert_eq!(fs::read_to_string(inputs.join(name)).unwrap(), name);
    }
    assert_eq!(
        fs::read(inputs.join("verified-inputs.sha256")).unwrap(),
        sums
    );
    assert_preserved(&fixture);
}

#[test]
fn owner_boundary_active_lease_prevents_scratch_retirement() {
    let fixture = fixture();
    let directory = fixture.0.join(SCRATCH).join("downloads");
    fs::create_dir_all(&directory).unwrap();
    let partial = directory.join("partial");
    fs::write(&partial, "live download").unwrap();
    let downloader = command(&fixture, "failure");
    let mut holder = Command::new("flock");
    holder
        .current_dir(&fixture.0)
        .arg(fixture.0.join("target/publication-inputs.lock"))
        .arg(downloader.get_program())
        .args(downloader.get_args());
    for (key, value) in downloader.get_envs() {
        if let Some(value) = value {
            holder.env(key, value);
        } else {
            holder.env_remove(key);
        }
    }
    assert!(ProcessControl::new(Duration::from_secs(15))
        .run(&mut holder, |_| Ok(()))
        .is_err());
    assert_eq!(fs::read_to_string(partial).unwrap(), "live download");
    assert_preserved(&fixture);
}

#[test]
fn owner_boundary_redirected_target_is_untouched() {
    let fixture = fixture();
    let protected = fixture.0.join("protected");
    fs::rename(fixture.0.join("target"), &protected).unwrap();
    symlink(&protected, fixture.0.join("target")).unwrap();
    assert!(run(&fixture, "failure").is_err());
    assert!(!protected.join("publication-inputs.lock").exists());
    assert_preserved(&fixture);
}

#[test]
fn owner_boundary_redirected_cache_parent_is_untouched() {
    let fixture = fixture();
    let protected = fixture.0.join("protected");
    fs::rename(fixture.0.join("target/publication-downloads"), &protected).unwrap();
    symlink(&protected, fixture.0.join("target/publication-downloads")).unwrap();
    assert!(run(&fixture, "failure").is_err());
    assert!(!protected.join(".partial.dlwork").exists());
    assert_preserved(&fixture);
}
