use super::*;
use crate::test_orchestrator_test::temporary_fixture;

#[test]
fn larger_metadata_budget_does_not_raise_the_default_suite_limit() {
    let root = temporary_fixture("metadata-report-limit");
    let path = root.0.join("metadata.json");
    let mut report = ReportFile::open_bounded(&path, 2 * MAX_REPORT_BYTES).unwrap();
    let bytes = vec![b'x'; MAX_REPORT_BYTES as usize + 1];
    report.publish(&bytes).unwrap();
    assert!(report
        .publish(&vec![b'x'; 2 * MAX_REPORT_BYTES as usize + 1])
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    drop(report);
    assert!(ReportFile::open(&path).is_err());
    assert!(ReportFile::open_bounded(&path, 0).is_err());
    assert!(ReportFile::open_bounded(&path, MAX_DOCUMENT_BYTES + 1).is_err());
}

#[test]
fn report_lease_rejects_a_second_writer_and_survives_reopen() {
    let root = temporary_fixture("report-lease");
    let path = root.0.join("report.json");
    let first = ReportFile::open(&path).unwrap();
    let run_id = first.run_id().to_owned();
    assert!(ReportFile::open(&path).is_err());
    drop(first);
    let second = ReportFile::open(&path).unwrap();
    assert_ne!(run_id, second.run_id());
}

#[cfg(unix)]
#[test]
fn report_lease_release_does_not_wait_for_a_duplicate_descriptor() {
    let root = temporary_fixture("report-lease-duplicate");
    let path = root.0.join("report.json");
    let first = ReportFile::open(&path).unwrap();
    let duplicate = first._lease.0.try_clone().unwrap();
    drop(first);
    let _second = ReportFile::open(&path).expect("owner explicitly released the shared lock");
    assert!(ReportFile::open(&path).is_err());
    drop(duplicate);
    assert!(
        ReportFile::open(&path).is_err(),
        "old descriptor cannot release new ownership"
    );
}

#[test]
fn interrupted_staging_is_retained_not_promoted_to_passing_evidence() {
    let root = temporary_fixture("report-interrupt");
    let path = root.0.join("report.json");
    fs::write(&path, "old completed report").unwrap();
    fs::write(sibling(&path, ".pending"), "incomplete new report").unwrap();
    let mut file = ReportFile::open(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "old completed report");
    assert_eq!(
        fs::read_to_string(sibling(&path, ".interrupted")).unwrap(),
        "incomplete new report"
    );
    file.publish(b"complete new report").unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "complete new report");
}

#[test]
fn oversized_report_cannot_replace_the_prior_snapshot() {
    let root = temporary_fixture("report-limit");
    let path = root.0.join("report.json");
    let mut file = ReportFile::open(&path).unwrap();
    file.publish(b"prior").unwrap();
    assert!(file
        .publish(&vec![0; MAX_REPORT_BYTES as usize + 1])
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), b"prior");
    assert!(!sibling(&path, ".pending").exists());
}

#[cfg(unix)]
#[test]
fn report_and_staging_symlinks_are_rejected_without_touching_their_targets() {
    for suffix in ["", ".lock", ".pending", ".interrupted"] {
        let root = temporary_fixture("report-symlink");
        let path = root.0.join("report.json");
        let unrelated = root.0.join("unrelated");
        fs::write(&unrelated, "preserve").unwrap();
        if suffix == ".interrupted" {
            fs::write(sibling(&path, ".pending"), "pending").unwrap();
        }
        std::os::unix::fs::symlink(&unrelated, sibling(&path, suffix)).unwrap();
        assert!(ReportFile::open(&path).is_err());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "preserve");
    }
}
