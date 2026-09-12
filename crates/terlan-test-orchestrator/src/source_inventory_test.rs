use super::*;
use crate::{
    launch_ledger::LaunchLedger, test_orchestrator_test::temporary_fixture, ValidationTier,
};
use std::fs::File;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(5))
}

fn git(root: &Path, args: &[&str]) {
    control()
        .run(Command::new("git").arg("-C").arg(root).args(args), |_| {
            Ok(())
        })
        .unwrap();
}

fn snapshot(root: &Path) -> SourceSnapshot {
    capture(root, control(), &mut |_| Ok(())).unwrap()
}

#[test]
fn source_names_are_canonical_bounded_and_order_independent() {
    for value in [
        b"".as_slice(),
        b"no-terminator",
        b"\0",
        b"../escape\0",
        b"/absolute\0",
        b"a/./b\0",
        b"a//b\0",
        b"a/../b\0",
        b"a\\b\0",
        b"\xff\0",
    ] {
        assert!(paths(value).is_err(), "accepted {value:?}");
    }
    assert_eq!(paths(b"b\0a\0a\0").unwrap(), paths(b"a\0b\0").unwrap());
    assert_eq!(paths(b"space and\nnewline\0").unwrap().len(), 1);
    let oversized = (0..=MAX_PATHS)
        .map(|index| format!("{index}\0"))
        .collect::<String>();
    assert!(paths(oversized.as_bytes()).is_err());
}

#[test]
fn working_tree_bytes_not_index_blobs_or_timestamps_define_source() {
    let fixture = temporary_fixture("source-content");
    git(&fixture.0, &["init", "--quiet"]);
    fs::write(fixture.0.join("input.rs"), "first").unwrap();
    git(&fixture.0, &["add", "input.rs"]);
    let first = snapshot(&fixture.0);
    let time = fs::metadata(fixture.0.join("input.rs"))
        .unwrap()
        .modified()
        .unwrap();
    fs::write(fixture.0.join("input.rs"), "other").unwrap();
    File::options()
        .write(true)
        .open(fixture.0.join("input.rs"))
        .unwrap()
        .set_modified(time)
        .unwrap();
    let changed = snapshot(&fixture.0);
    assert_ne!(first, changed);
    assert_eq!(first.files, changed.files);
    assert_eq!(first.bytes, changed.bytes);
    fs::write(fixture.0.join("input.rs"), "first").unwrap();
    assert_eq!(first, snapshot(&fixture.0));
}

#[test]
fn additions_deletions_and_ignore_policy_are_observed_without_hashing_outputs() {
    let fixture = temporary_fixture("source-membership");
    git(&fixture.0, &["init", "--quiet"]);
    fs::write(fixture.0.join(".gitignore"), "target/\n").unwrap();
    fs::write(fixture.0.join("input.rs"), "source").unwrap();
    git(&fixture.0, &["add", ".gitignore", "input.rs"]);
    let first = snapshot(&fixture.0);
    fs::create_dir(fixture.0.join("target")).unwrap();
    fs::write(fixture.0.join("target/output"), "disposable").unwrap();
    assert_eq!(first, snapshot(&fixture.0));
    fs::write(fixture.0.join("new.rs"), "new input").unwrap();
    assert_ne!(first, snapshot(&fixture.0));
    fs::remove_file(fixture.0.join("new.rs")).unwrap();
    fs::remove_file(fixture.0.join("input.rs")).unwrap();
    let deleted = snapshot(&fixture.0);
    assert_eq!(
        deleted.files, first.files,
        "tracked absence remains an explicit input"
    );
    assert_ne!(first, deleted);
    fs::write(fixture.0.join(".gitignore"), "").unwrap();
    assert!(snapshot(&fixture.0).files > deleted.files);
}

#[cfg(unix)]
#[test]
fn source_links_and_modes_are_bound_and_special_files_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = temporary_fixture("source-links");
    git(&fixture.0, &["init", "--quiet"]);
    fs::write(fixture.0.join("one"), "contents").unwrap();
    fs::write(fixture.0.join("two"), "contents").unwrap();
    symlink("one", fixture.0.join("link")).unwrap();
    let first = snapshot(&fixture.0);
    fs::remove_file(fixture.0.join("link")).unwrap();
    symlink("two", fixture.0.join("link")).unwrap();
    let retargeted = snapshot(&fixture.0);
    assert_ne!(first, retargeted);
    fs::set_permissions(fixture.0.join("one"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_ne!(retargeted, snapshot(&fixture.0));
    fs::remove_file(fixture.0.join("link")).unwrap();
    symlink(std::env::current_exe().unwrap(), fixture.0.join("link")).unwrap();
    assert!(capture(&fixture.0, control(), &mut |_| Ok(())).is_err());
    fs::remove_file(fixture.0.join("link")).unwrap();
    control()
        .run(Command::new("mkfifo").arg(fixture.0.join("fifo")), |_| {
            Ok(())
        })
        .unwrap();
    let root = fs::canonicalize(&fixture.0).unwrap();
    assert!(fingerprint(&root, b"fifo\0", control(), Instant::now(), MAX_BYTES).is_err());
}

#[test]
fn source_hashing_honors_byte_deadline_and_cancellation_limits() {
    let fixture = temporary_fixture("source-limits");
    fs::write(fixture.0.join("input"), "bytes").unwrap();
    let root = fs::canonicalize(&fixture.0).unwrap();
    assert!(fingerprint(&root, b"input\0", control(), Instant::now(), 4).is_err());
    let cancelled = AtomicBool::new(true);
    let error = fingerprint(
        &root,
        b"input\0",
        control().with_cancellation(&cancelled),
        Instant::now(),
        9,
    )
    .unwrap_err();
    assert_eq!(error.outcome, "cancelled");
    let expired = ProcessControl::new(Duration::from_millis(1));
    let error = fingerprint(
        &root,
        b"input\0",
        expired,
        Instant::now() - Duration::from_secs(1),
        9,
    )
    .unwrap_err();
    assert_eq!(error.outcome, "timed-out");
}

#[test]
fn source_change_prevents_sealing_and_retains_both_observations() {
    let fixture = temporary_fixture("source-closeout");
    git(&fixture.0, &["init", "--quiet"]);
    fs::write(fixture.0.join("input"), "before").unwrap();
    let before = snapshot(&fixture.0);
    fs::write(fixture.0.join("input"), "after").unwrap();
    let after = snapshot(&fixture.0);
    let report_fixture = temporary_fixture("source-report");
    let report = report_fixture.0.join("report.json");
    let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
    ledger.bind_source(before.clone()).unwrap();
    assert!(ledger.verify_source(after.clone()).is_err());
    assert!(ledger.finish().is_err());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(value["decision"], "fail");
    assert_eq!(value["source_binding"]["before"]["sha256"], before.sha256);
    assert_eq!(value["source_binding"]["after"]["sha256"], after.sha256);
    assert_eq!(value["source_binding"]["verified"], false);
}

#[test]
fn source_admission_requires_closeout_and_cannot_be_replaced() {
    let fixture = temporary_fixture("source-binding");
    git(&fixture.0, &["init", "--quiet"]);
    fs::write(fixture.0.join("input"), "source").unwrap();
    let source = snapshot(&fixture.0);
    let reports = temporary_fixture("source-binding-reports");
    for mode in ["missing", "duplicate", "matching", "late"] {
        let mut ledger =
            LaunchLedger::new(&reports.0.join(mode), 1, Duration::from_secs(5)).unwrap();
        if mode != "late" {
            ledger.bind_source(source.clone()).unwrap();
        }
        ledger
            .execute(
                "producer",
                ValidationTier::FastUnit,
                "direct-fixture",
                |observed| {
                    control()
                        .run(Command::new("git").arg("--version"), observed)
                        .map_err(process_failure)
                },
            )
            .unwrap();
        match mode {
            "duplicate" => {
                assert!(ledger.bind_source(source.clone()).is_err());
            }
            "matching" => {
                ledger.verify_source(source.clone()).unwrap();
            }
            "late" => {
                assert!(ledger.bind_source(source.clone()).is_err());
            }
            _ => {}
        }
        assert_eq!(ledger.finish().is_ok(), mode == "matching");
    }
}
