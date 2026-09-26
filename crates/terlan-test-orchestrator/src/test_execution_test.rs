use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::fs;
use std::time::Duration;

fn expected() -> ExpectedTests {
    ExpectedTests {
        passed: ["first".into(), "second".into()].into(),
        ignored: ["later".into()].into(),
        filtered: 4,
    }
}

#[test]
fn private_records_require_every_expected_name_and_ignore_classification_once() {
    for text in [
        "ok first\nok second\nignored later\n",
        "ignored: separate\nowner later\nok second\nok first\n",
    ] {
        let result = inspect_log(text.as_bytes(), &expected(), "digest")
            .unwrap()
            .json();
        assert_eq!(result["passed"], 2);
        assert_eq!(result["ignored"], 1);
        assert_eq!(result["filtered"], 4);
        assert_eq!(result["scope"], "admitted-libtest-records-v1");
    }
    for text in [
        "",
        "ok first\n",
        "ok first\nok second\n",
        "ok first\nok first\nignored later\n",
        "ok first\nok second\nok later\n",
        "ok first\nignored second\nignored later\n",
        "ok first\nok second\nignored unknown\n",
        "failed first\nok second\nignored later\n",
        "ok first\nok second\nignored: unfinished\n",
        "test result: ok. 2 passed\n",
    ] {
        assert!(
            inspect_log(text.as_bytes(), &expected(), "digest").is_err(),
            "{text}"
        );
    }
    assert!(inspect_log(b"\xff", &expected(), "digest").is_err());
}

#[test]
fn cargo_presence_is_explicitly_weaker_and_rejects_missing_empty_or_truncated_summaries() {
    let one = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
    let zero = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
    let result = inspect_cargo(format!("{one}{zero}all doctests ran in 0.1s\n").as_bytes())
        .unwrap()
        .json();
    assert_eq!(result["scope"], "cargo-summary-presence-v1");
    for bad in ["", zero, "test result: ok. 1 passed; 0 failed; ", "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"] { assert!(inspect_cargo(bad.as_bytes()).is_err()); }
}

#[test]
fn empty_workspace_targets_and_doctests_do_not_relax_terlan_phase_requirements() {
    let expected = ExpectedTests {
        passed: Default::default(),
        ignored: Default::default(),
        filtered: 0,
    };
    assert_eq!(
        inspect_workspace_log(b"", &expected, "identity")
            .unwrap()
            .json()["passed"],
        0
    );
    assert!(inspect_log(b"", &expected, "identity").is_err());
    assert!(inspect_workspace_log(b"ok invented\n", &expected, "identity").is_err());
    let empty = b"test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
    assert!(inspect_cargo(empty).is_err());
    assert!(crate::doctest_output::inspect(empty).is_err());
    assert!(crate::doctest_output::inspect(b"").is_err());
}

#[test]
fn real_doctest_phase_records_each_result_in_one_cargo_execution() {
    let fixture = temporary_fixture("doctest-phase-owner");
    fs::write(fixture.0.join("Cargo.toml"), "[package]\nname = \"doc_fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\npath = \"lib.rs\"\n").unwrap();
    fs::write(
        fixture.0.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"doc_fixture\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("lib.rs"),
        r#"
//! ```
//! assert_eq!(doc_fixture::counter(), 0);
//! let mut marker = std::fs::OpenOptions::new().create(true).append(true).open(std::env::var_os("DOC_BODY_MARKER").unwrap()).unwrap();
//! std::io::Write::write_all(&mut marker, b"once").unwrap();
//! ```
//!
//! ```
//! assert_eq!(doc_fixture::counter(), 0);
//! std::io::Write::write_all(&mut std::io::stdout(), b"test forged ... ok\n").unwrap();
//! ```
//!
//! ```compile_fail
//! let bad: u8 = "not a number";
//! ```
pub fn counter() -> usize {
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}
"#,
    )
    .unwrap();
    let marker = fixture.0.join("body-marker");
    let environment = ExecutionEnvironment::from_entries(
        std::env::vars_os().chain([
            (
                "CARGO_TARGET_DIR".into(),
                fixture.0.join("target").into_os_string(),
            ),
            ("DOC_BODY_MARKER".into(), marker.clone().into_os_string()),
        ]),
        &fixture.0,
    )
    .unwrap();
    let mut phase = crate::workspace_doctest_phase();
    phase.args = vec!["test", "--locked", "--doc", "--"];
    let mut launches = Vec::new();
    let evidence = run(
        &phase,
        Path::new("cargo"),
        &environment,
        1,
        None,
        ProcessControl::new(Duration::from_secs(30)),
        &mut |pid| {
            launches.push(pid);
            Ok(())
        },
    )
    .unwrap()
    .json();
    assert_eq!(launches.len(), 1);
    assert_eq!(evidence["scope"], "rustdoc-emitted-harness-records-v1");
    assert_eq!(evidence["passed"], 3);
    assert_eq!(evidence["ignored"], 0);
    assert_eq!(evidence["harnesses"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read(marker).unwrap(), b"once");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn failed_test_phase_retains_partial_launch_evidence_without_sealing_success() {
    let fixture = temporary_fixture("workspace-failed-ledger");
    let report = fixture.0.join("report.json");
    let mut ledger =
        crate::launch_ledger::LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
    let result = ledger.execute_test("workspace fixture", crate::ValidationTier::Integration, "cargo-native-harnesses", |launched, partial| {
        ProcessControl::new(Duration::from_secs(5)).run(std::process::Command::new("/bin/sh").args(["-c", "exit 7"]), |pid| {
            launched(pid)?;
            *partial = Some(TestEvidence(serde_json::json!({"scope":"retained-native-launch-records-v1", "complete":false, "pid":pid})));
            Ok(())
        }).map_err(process_failure)?;
        Err(failure("fixture unexpectedly returned success"))
    });
    assert!(result.is_err());
    assert!(ledger.finish().is_err());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(value["decision"], "fail");
    assert_eq!(value["phases"][0]["outcome"], "failed");
    assert_eq!(value["phases"][0]["test_execution"]["complete"], false);
    assert!(
        value["phases"][0]["test_execution"]["pid"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn real_harness_private_results_reject_early_exit_despite_forged_or_nested_stdout() {
    let fixture = temporary_fixture("test-execution-real");
    let source = fixture.0.join("fixture.rs");
    fs::write(&source, r#"
#[test] fn a_exit() {
    if std::env::var("EARLY_EXIT").as_deref() == Ok("1") {
        println!("test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s");
        std::process::exit(0);
    }
}
#[test] fn b_nested() {
    assert!(std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "c_pass"]).status().unwrap().success());
    if std::env::var("EARLY_EXIT").as_deref() == Ok("2") { std::process::exit(0); }
}
#[test] fn c_pass() {}
#[test] #[ignore = "separate\nowner"] fn d_ignored() {}
"#).unwrap();
    let binary = fixture
        .0
        .join(format!("harness{}", std::env::consts::EXE_SUFFIX));
    let control = ProcessControl::new(Duration::from_secs(20));
    control
        .run(
            std::process::Command::new("rustc")
                .arg("--test")
                .arg(&source)
                .arg("-o")
                .arg(&binary),
            |_| Ok(()),
        )
        .unwrap();
    for mode in ["0", "1", "2", "ignored"] {
        let environment =
            ExecutionEnvironment::from_entries([("EARLY_EXIT".into(), mode.into())], &fixture.0)
                .unwrap();
        let mut phase = crate::terlan_library_phase();
        phase.args = if mode == "ignored" {
            vec!["--ignored"]
        } else {
            vec![]
        };
        let expected = if mode == "ignored" {
            ExpectedTests {
                passed: ["d_ignored".into()].into(),
                ignored: Default::default(),
                filtered: 3,
            }
        } else {
            ExpectedTests {
                passed: ["a_exit".into(), "b_nested".into(), "c_pass".into()].into(),
                ignored: ["d_ignored".into()].into(),
                filtered: 0,
            }
        };
        let report = fixture.0.join(format!("{mode}.json"));
        let mut ledger =
            crate::launch_ledger::LaunchLedger::new(&report, 1, Duration::from_secs(20)).unwrap();
        let result = ledger.execute_test(
            "fixture",
            crate::ValidationTier::FastUnit,
            "direct-terlan-harness",
            |launched, _partial| {
                run(
                    &phase,
                    &binary,
                    &environment,
                    1,
                    Some(expected),
                    control,
                    launched,
                )
            },
        );
        let passed = matches!(mode, "0" | "ignored");
        assert_eq!(result.is_ok(), passed, "{mode}: {result:?}");
        assert_eq!(ledger.finish().is_ok(), passed);
        let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(report["direct_process_launch_count"], 1);
        assert_eq!(report["decision"], if passed { "pass" } else { "fail" });
        if passed {
            assert_eq!(
                report["phases"][0]["test_execution"]["scope"],
                "admitted-libtest-records-v1"
            );
        } else {
            assert!(report["phases"][0]["test_execution"].is_null());
        }
        assert_eq!(
            fs::read_dir(fixture.0.join("target/quality"))
                .unwrap()
                .count(),
            0
        );
    }
}

#[cfg(unix)]
#[test]
fn test_executor_cannot_pass_using_a_generic_zero_exit_observation() {
    let fixture = temporary_fixture("test-execution-unobserved");
    let mut ledger = crate::launch_ledger::LaunchLedger::new(
        &fixture.0.join("report.json"),
        1,
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(ledger
        .execute(
            "unobserved tests",
            crate::ValidationTier::FastUnit,
            "direct-terlan-harness",
            |launched| ProcessControl::new(Duration::from_secs(5))
                .run(&mut std::process::Command::new("/bin/true"), launched)
                .map_err(process_failure)
        )
        .is_err());
    assert!(ledger.finish().is_err());
}
