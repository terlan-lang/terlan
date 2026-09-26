use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::time::Duration;

#[test]
fn private_log_reads_exact_bounded_bytes_and_cleans_normal_and_unwind_paths() {
    let fixture = temporary_fixture("result-log");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    let log = TestResultLog::create(&environment).unwrap();
    let directory = log.directory.clone().unwrap();
    fs::write(log.path(), b"ok fixture\n").unwrap();
    assert_eq!(log.read(control).unwrap().0, b"ok fixture\n");
    log.close().unwrap();
    assert!(!directory.exists());
    let log = TestResultLog::create(&environment).unwrap();
    let directory = log.directory.clone().unwrap();
    fs::File::create(log.path())
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(log.read(control).is_err());
    drop(log);
    assert!(!directory.exists());
    let log = TestResultLog::create(&environment).unwrap();
    let directory = log.directory.clone().unwrap();
    let _ = std::panic::catch_unwind(|| {
        let _owned = log;
        panic!("fixture unwind");
    });
    assert!(!directory.exists());
}

#[cfg(unix)]
#[test]
fn private_log_rejects_symlink_substitution_without_removing_the_target() {
    let fixture = temporary_fixture("result-log-link");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    let target = fixture.0.join("outside");
    fs::write(&target, b"untouched").unwrap();
    let log = TestResultLog::create(&environment).unwrap();
    std::os::unix::fs::symlink(&target, log.path()).unwrap();
    assert!(log
        .read(ProcessControl::new(Duration::from_secs(5)))
        .is_err());
    drop(log);
    assert_eq!(fs::read(&target).unwrap(), b"untouched");
}
