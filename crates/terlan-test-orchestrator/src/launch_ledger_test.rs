use super::*;
use crate::test_orchestrator_test::temporary_fixture;

fn snapshot(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[cfg(unix)]
#[test]
fn configuration_closeout_cannot_seal_missing_changed_repeated_or_late_inputs() {
    let root = temporary_fixture("configuration-ledger");
    let cargo_home = root.0.join("cargo");
    let rustup_home = root.0.join("rustup");
    std::fs::create_dir(&cargo_home).unwrap();
    let config = cargo_home.join("config.toml");
    let environment = execution_environment::ExecutionEnvironment::from_entries(
        [
            ("CARGO_HOME".into(), cargo_home.into_os_string()),
            ("RUSTUP_HOME".into(), rustup_home.into_os_string()),
        ],
        &root.0,
    )
    .unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    for mode in ["missing", "changed", "matching", "duplicate", "late"] {
        std::fs::write(&config, "[build]\njobs = 1\n").unwrap();
        let report = root.0.join(mode);
        let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
        ledger.bind_environment(&environment, &[]).unwrap();
        if mode != "late" {
            ledger.admit_configuration(&environment, control).unwrap();
        }
        ledger
            .execute(
                "configuration fixture",
                ValidationTier::FastUnit,
                "fixture",
                |observe| {
                    control
                        .run(&mut Command::new("/bin/true"), observe)
                        .map_err(process_failure)
                },
            )
            .unwrap();
        match mode {
            "changed" => {
                std::fs::write(&config, "[build]\njobs = 2\n").unwrap();
                assert!(ledger.verify_configuration(control).is_err());
                let binding = &snapshot(&report)["tool_configuration_binding"];
                assert_ne!(binding["before"], binding["after"]);
            }
            "matching" => ledger.verify_configuration(control).unwrap(),
            "duplicate" => {
                ledger.verify_configuration(control).unwrap();
                assert!(ledger.verify_configuration(control).is_err());
            }
            "late" => assert!(ledger.admit_configuration(&environment, control).is_err()),
            _ => {}
        }
        assert_eq!(ledger.finish().is_ok(), mode == "matching");
        assert_eq!(
            snapshot(&report)["decision"],
            if mode == "matching" { "pass" } else { "fail" }
        );
        assert_eq!(snapshot(&report)["direct_process_launch_count"], 1);
    }
}

#[test]
fn environment_binding_is_persisted_once_before_producers() {
    let root = temporary_fixture("environment-ledger");
    let path = root.0.join("report.json");
    let environment = execution_environment::ExecutionEnvironment::capture().unwrap();
    let phases = test_phases(false);
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(5)).unwrap();
    assert!(snapshot(&path)["environment_binding"].is_null());
    ledger.bind_environment(&environment, &phases).unwrap();
    assert_eq!(
        snapshot(&path)["environment_binding"],
        environment.json(&phases)
    );
    assert_eq!(snapshot(&path)["direct_process_launch_count"], 0);
    assert!(ledger.bind_environment(&environment, &phases).is_err());
    assert_eq!(snapshot(&path)["decision"], "fail");
    assert!(ledger.finish().is_err());
}

#[test]
fn late_environment_admission_cannot_relabel_existing_launches() {
    let root = temporary_fixture("environment-late");
    let path = root.0.join("report.json");
    let environment = execution_environment::ExecutionEnvironment::capture().unwrap();
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(5)).unwrap();
    ledger
        .execute(
            "fixture observation",
            ValidationTier::FastUnit,
            "fixture",
            |observe| observe(42).map_err(failure),
        )
        .unwrap();
    assert!(ledger.bind_environment(&environment, &[]).is_err());
    assert_eq!(snapshot(&path)["decision"], "fail");
    assert!(snapshot(&path)["environment_binding"].is_null());
    assert!(ledger.finish().is_err());
}

#[cfg(unix)]
#[test]
fn executable_binding_cannot_seal_missing_or_failed_closeout() {
    use std::os::unix::fs::PermissionsExt;
    let root = temporary_fixture("executable-ledger");
    let tool = root.0.join("tool");
    std::fs::write(&tool, "first").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    for mode in ["missing", "changed", "matching", "duplicate", "late"] {
        std::fs::write(&tool, "first").unwrap();
        let report = root.0.join(mode);
        let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
        ledger
            .observe_executables(|binding| {
                *binding = executable_binding::ExecutableBinding::capture(
                    &[("fixture", tool.clone())],
                    control,
                )?;
                Ok(())
            })
            .unwrap();
        ledger
            .execute(
                "completed fixture",
                ValidationTier::FastUnit,
                "direct-fixture",
                |observe| {
                    control
                        .run(&mut Command::new("/bin/true"), observe)
                        .map_err(process_failure)
                },
            )
            .unwrap();
        match mode {
            "changed" => {
                std::fs::write(&tool, "other").unwrap();
                assert!(ledger.verify_executables(control).is_err());
            }
            "matching" => ledger.verify_executables(control).unwrap(),
            "duplicate" => {
                ledger.verify_executables(control).unwrap();
                assert!(ledger.verify_executables(control).is_err());
            }
            "late" => assert!(ledger
                .admit_executables(
                    &execution_environment::ExecutionEnvironment::capture().unwrap(),
                    control
                )
                .is_err()),
            _ => {}
        }
        assert_eq!(ledger.finish().is_ok(), mode == "matching");
        assert_eq!(
            snapshot(&report)["decision"],
            if mode == "matching" { "pass" } else { "fail" }
        );
        assert_eq!(snapshot(&report)["direct_process_launch_count"], 1);
    }
}

#[cfg(unix)]
#[test]
fn changed_harness_is_rejected_without_a_new_process_launch() {
    use std::os::unix::fs::PermissionsExt;
    let root = temporary_fixture("harness-admission-ledger");
    let tool = root.0.join("tool");
    std::fs::write(&tool, "before").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    let report = root.0.join("report");
    let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
    ledger
        .observe_executables(|binding| {
            *binding = executable_binding::ExecutableBinding::capture(
                &[("fixture", tool.clone())],
                control,
            )?;
            Ok(())
        })
        .unwrap();
    ledger.bind_harness(&tool, control).unwrap();
    let inputs = ledger.executables().clone();
    std::fs::write(&tool, "change").unwrap();
    let mut entered_producer = false;
    assert!(ledger
        .execute(
            "guarded harness",
            ValidationTier::FastUnit,
            "direct-terlan-harness",
            |_| {
                inputs.verify_harness(control)?;
                entered_producer = true;
                Ok(())
            }
        )
        .is_err());
    assert!(!entered_producer);
    assert!(ledger.finish().is_err());
    assert_eq!(snapshot(&report)["direct_process_launch_count"], 0);
    assert_eq!(
        snapshot(&report)["phases"][0]["outcome"],
        "executable-identity-failed"
    );
}

#[test]
fn spawn_failure_is_an_attempt_not_a_cargo_launch() {
    let root = temporary_fixture("launch-failure");
    let path = root.0.join("report.json");
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
    let error = ledger
        .execute(
            "Cargo",
            ValidationTier::FastUnit,
            "cargo-build",
            |observe| {
                terlan_process_owner::run_with_launch(
                    &mut Command::new("missing-terlan-launch-probe"),
                    Duration::from_secs(1),
                    observe,
                )
                .map_err(process_failure)
            },
        )
        .unwrap_err();
    assert_eq!(error.outcome, "launch-failed");
    let report = snapshot(&path);
    assert_eq!(report["decision"], "fail");
    assert_eq!(report["direct_cargo_launch_count"], 0);
    assert_eq!(report["direct_process_launch_count"], 0);
    assert!(report["phases"][0]["child_pid"].is_null());
}

#[test]
fn success_without_a_spawn_is_rejected_and_empty_suite_cannot_pass() {
    let root = temporary_fixture("launch-required");
    let path = root.0.join("report.json");
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
    assert!(ledger.finish().is_err());
    assert!(ledger
        .execute("missing", ValidationTier::FastUnit, "cargo-build", |_| Ok(
            ()
        ))
        .is_err());
    assert!(ledger.finish().is_err());
    assert_eq!(snapshot(&path)["decision"], "fail");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn running_snapshot_precedes_completion_and_records_actual_pid() {
    let root = temporary_fixture("launch-observed");
    let path = root.0.join("report.json");
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
    ledger
        .execute(
            "probe",
            ValidationTier::FastUnit,
            "cargo-build",
            |observe| {
                terlan_process_owner::capture_stdout_with_launch(
                    &mut shell("printf done"),
                    Duration::from_secs(2),
                    1024,
                    |pid| {
                        observe(pid)?;
                        let report = snapshot(&path);
                        assert_eq!(report["decision"], "running");
                        assert_eq!(report["direct_cargo_launch_count"], 1);
                        assert_eq!(report["phases"][0]["child_pid"], pid);
                        assert_eq!(report["phases"][0]["outcome"], "running");
                        Ok(())
                    },
                )
                .map_err(process_failure)
            },
        )
        .unwrap();
    ledger.finish().unwrap();
    assert_eq!(snapshot(&path)["decision"], "pass");
    let mut reran = false;
    assert!(ledger
        .execute("probe", ValidationTier::FastUnit, "cargo-build", |_| {
            reran = true;
            Ok(())
        })
        .is_err());
    assert!(!reran);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn failed_child_is_still_a_real_launch() {
    let root = temporary_fixture("launch-nonzero");
    let path = root.0.join("report.json");
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
    assert!(ledger
        .execute(
            "probe",
            ValidationTier::FastUnit,
            "direct-terlan-harness",
            |observe| {
                terlan_process_owner::run_with_launch(
                    &mut shell("exit 7"),
                    Duration::from_secs(2),
                    observe,
                )
                .map_err(process_failure)
            }
        )
        .is_err());
    let report = snapshot(&path);
    assert_eq!(report["direct_process_launch_count"], 1);
    assert_eq!(report["direct_cargo_launch_count"], 0);
    assert_eq!(report["phases"][0]["outcome"], "failed");
}

#[test]
fn ignored_observation_errors_cannot_be_sealed_as_success() {
    let root = temporary_fixture("launch-ignored-error");
    let path = root.0.join("report.json");
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
    assert!(ledger
        .execute(
            "probe",
            ValidationTier::FastUnit,
            "cargo-build",
            |observe| {
                observe(42).unwrap();
                let _ = observe(43);
                Ok(())
            }
        )
        .is_err());
    assert!(ledger.finish().is_err());
    assert_eq!(snapshot(&path)["decision"], "fail");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn duplicate_owner_poisoning_and_launch_budget_prevent_more_processes() {
    for duplicate in [false, true] {
        let root = temporary_fixture("launch-budget");
        let path = root.0.join("report.json");
        let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
        for name in ["first", "second", "third"] {
            ledger
                .execute(name, ValidationTier::FastUnit, "cargo-build", |observe| {
                    terlan_process_owner::run_with_launch(
                        &mut shell("exit 0"),
                        Duration::from_secs(2),
                        observe,
                    )
                    .map_err(process_failure)
                })
                .unwrap();
        }
        let mut called = false;
        assert!(ledger
            .execute(
                if duplicate { "first" } else { "fourth" },
                ValidationTier::FastUnit,
                if duplicate {
                    "direct-terlan-harness"
                } else {
                    "cargo"
                },
                |_| {
                    called = true;
                    Ok(())
                }
            )
            .is_err());
        assert!(!called);
        assert!(ledger.finish().is_err());
        let report = snapshot(&path);
        assert_eq!(report["decision"], "fail");
        assert_eq!(report["direct_cargo_launch_count"], 3);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn inventory_io_failure_cannot_continue_or_pass() {
    let root = temporary_fixture("launch-storage-failure");
    let path = root.0.join("report.json");
    let pending = path.with_file_name("report.json.pending");
    let mut ledger = LaunchLedger::new(&path, 1, Duration::from_secs(2)).unwrap();
    assert!(ledger
        .execute(
            "probe",
            ValidationTier::FastUnit,
            "cargo-build",
            |observe| {
                terlan_process_owner::run_with_launch(
                    &mut shell("sleep 5"),
                    Duration::from_secs(2),
                    |pid| {
                        std::fs::create_dir(&pending).unwrap();
                        observe(pid)
                    },
                )
                .map_err(process_failure)
            }
        )
        .is_err());
    std::fs::remove_dir(pending).unwrap();
    assert!(ledger.finish().is_err());
    let mut called = false;
    assert!(ledger
        .execute("another", ValidationTier::FastUnit, "cargo-build", |_| {
            called = true;
            Ok(())
        })
        .is_err());
    assert!(!called);
    assert_ne!(snapshot(&path)["decision"], "pass");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn launch_snapshot_survives_real_sigkill_without_claiming_completion() {
    const PROBE: &str = "TERLAN_LEDGER_SIGKILL_FIXTURE";
    if let Some(root) = std::env::var_os(PROBE) {
        let root = PathBuf::from(root);
        let mut ledger =
            LaunchLedger::new(&root.join("report.json"), 1, Duration::from_secs(5)).unwrap();
        let _: Result<(), _> = ledger.execute(
            "probe",
            ValidationTier::FastUnit,
            "cargo-build",
            |observe| {
                terlan_process_owner::run_with_launch(
                    &mut shell("exit 0"),
                    Duration::from_secs(2),
                    observe,
                )
                .map_err(process_failure)?;
                // The producer is already reaped. Kill only the test owner in the
                // gap before the phase outcome is sealed, leaving no orphan job.
                std::fs::write(root.join("ready"), "producer reaped").unwrap();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            },
        );
        panic!("crash probe unexpectedly returned");
    }
    let root = temporary_fixture("launch-sigkill");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["launch_ledger::tests::launch_snapshot_survives_real_sigkill_without_claiming_completion", "--exact"])
        .env(PROBE, &root.0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = terlan_process_owner::OwnedChild::spawn(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.0.join("ready").exists() {
        assert!(
            Instant::now() < deadline,
            "crash probe did not reach its checkpoint"
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "crash probe exited early"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let path = root.0.join("report.json");
    let before = std::fs::read(&path).unwrap();
    let observed = snapshot(&path);
    assert_eq!(observed["direct_cargo_launch_count"], 1);
    assert_eq!(observed["phases"][0]["outcome"], "running");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(child.finish().unwrap().signal(), Some(9));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let _next_owner = ReportFile::open(&path).expect("SIGKILL released the suite lease");
    assert_eq!(snapshot(&path)["decision"], "running");
}
