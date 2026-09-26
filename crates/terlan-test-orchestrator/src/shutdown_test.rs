#![cfg(unix)]

use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use crate::{
    launch_ledger::LaunchLedger, process_failure, report_file, ProcessControl, ValidationTier,
};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use terlan_process_owner::OwnedChild;

const FIXTURE_ROOT: &str = "TERLAN_SHUTDOWN_TEST_ROOT";

fn child(root: &Path) {
    let shutdown = Shutdown::install().unwrap();
    let control = ProcessControl::new(Duration::from_secs(10)).with_cancellation(shutdown.flag());
    let mut ledger =
        LaunchLedger::new(&root.join("report.json"), 1, Duration::from_secs(10)).unwrap();
    let error = ledger
        .execute(
            "signal fixture",
            ValidationTier::FastUnit,
            "direct-fixture",
            |observed| {
                control
                    .capture_stdout(
                        Command::new("/bin/sh").args(["-c", "sleep 20"]),
                        1024,
                        |pid| {
                            observed(pid)?;
                            fs::write(root.join("ready"), pid.to_string())
                                .map_err(|error| error.to_string())
                        },
                    )
                    .map_err(process_failure)
            },
        )
        .unwrap_err();
    assert_eq!(error.outcome, "cancelled");
    assert!(shutdown.flag().load(Ordering::Acquire));
    assert!(ledger.finish_unless_cancelled(shutdown.flag()).is_err());
}

#[test]
fn unix_signals_cancel_the_producer_and_preserve_a_failed_report() {
    if let Some(root) = std::env::var_os(FIXTURE_ROOT) {
        child(Path::new(&root));
        return;
    }
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        let fixture = temporary_fixture("shutdown");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "shutdown::tests::unix_signals_cancel_the_producer_and_preserve_a_failed_report",
                "--exact",
            ])
            .env(FIXTURE_ROOT, &fixture.0);
        let mut owner = OwnedChild::spawn(command).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !fixture.0.join("ready").try_exists().unwrap() {
            assert!(
                owner.try_wait().unwrap().is_none(),
                "signal owner exited before readiness"
            );
            assert!(
                Instant::now() < deadline,
                "signal owner did not become ready"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        // kill(1) only signals this fixture's retained direct child, never a host group.
        let status = Command::new("/bin/kill")
            .arg(format!("-{signal}"))
            .arg(owner.id().to_string())
            .status()
            .unwrap();
        assert!(status.success());
        let status = loop {
            if let Some(status) = owner.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "signal cleanup exceeded its deadline"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        assert!(status.success(), "fixture failed for signal {signal}");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.0.join("report.json")).unwrap()).unwrap();
        assert_eq!(value["decision"], "fail");
        assert_eq!(value["direct_process_launch_count"], 1);
        assert_eq!(value["phases"][0]["outcome"], "cancelled");
        // The terminal writer released its report lease; no stale running owner is adopted.
        let _reopened = report_file::ReportFile::open(&fixture.0.join("report.json")).unwrap();
    }
}

#[test]
fn cancellation_at_closeout_cannot_seal_a_passing_run() {
    let fixture = temporary_fixture("cancelled-closeout");
    let report = fixture.0.join("report.json");
    let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
    ledger
        .execute(
            "completed phase",
            ValidationTier::FastUnit,
            "direct-fixture",
            |observed| {
                ProcessControl::new(Duration::from_secs(5))
                    .run(&mut Command::new("/usr/bin/true"), observed)
                    .map_err(process_failure)
            },
        )
        .unwrap();
    assert!(ledger
        .finish_unless_cancelled(&AtomicBool::new(true))
        .is_err());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(value["decision"], "fail");
    assert_eq!(value["phases"][0]["outcome"], "passed");
}
