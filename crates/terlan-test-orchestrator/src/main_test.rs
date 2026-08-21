use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::{
    phase_timeout_from, run_closed_command, select_terlan_harness, test_phases,
    validate_tier_inventory, write_report, PhaseExecutor, PhaseResult, ValidationTier,
    DEFAULT_PHASE_TIMEOUT_SECONDS, EXTERNAL_TIER_OWNERS, INTEGRATION_FILTERS, MAX_CARGO_PHASES,
    TIER_INVENTORY,
};

#[test]
fn orchestrator_runs_shared_runtime_tests_in_the_library_harness() {
    let phases = test_phases(false);
    let library = phases.first().expect("Terlan library phase");

    assert_eq!(library.name, "Terlan library");
    assert_eq!(library.executor, PhaseExecutor::TerlanHarness);
    for filter in INTEGRATION_FILTERS {
        assert!(library
            .args
            .windows(2)
            .any(|arguments| arguments == ["--skip", filter]));
    }
}

#[test]
fn orchestrator_partitions_one_union_feature_harness_without_test_replay() {
    let integration = test_phases(false)
        .into_iter()
        .find(|phase| phase.name == "Terlan union-feature integration")
        .expect("union-feature integration phase");

    assert_eq!(integration.executor, PhaseExecutor::TerlanHarness);
    for filter in INTEGRATION_FILTERS {
        assert!(integration.args.contains(&filter));
    }
    let phases = test_phases(false);
    let terlan_library_phases = phases
        .iter()
        .filter(|phase| phase.executor == PhaseExecutor::TerlanHarness);
    assert_eq!(terlan_library_phases.count(), 4);
}

#[test]
fn orchestrator_runs_ignored_contract_once_in_the_library() {
    let phases = test_phases(false);
    let ignored: Vec<_> = phases
        .iter()
        .filter(|phase| phase.args.contains(&"--ignored"))
        .collect();

    assert_eq!(ignored.len(), 2);
    assert!(ignored
        .iter()
        .all(|phase| phase.executor == PhaseExecutor::TerlanHarness));
    assert!(ignored.iter().all(|phase| phase.args.contains(&"--exact")));
    assert!(ignored
        .iter()
        .any(|phase| phase.name == "generated C++ package evidence"));
}

#[test]
fn release_orchestrator_leaves_only_normal_library_tests_to_coverage() {
    let phases = test_phases(true);

    assert!(!phases.iter().any(|phase| phase.name == "Terlan library"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "workspace support crates"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "ignored std collection contract"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "generated C++ package evidence"));
}

#[test]
fn orchestrator_phase_timeout_is_positive_and_bounded_by_default() {
    assert_eq!(
        phase_timeout_from(None),
        Duration::from_secs(DEFAULT_PHASE_TIMEOUT_SECONDS)
    );
    assert_eq!(phase_timeout_from(Some("7")), Duration::from_secs(7));
    assert_eq!(
        phase_timeout_from(Some("0")),
        Duration::from_secs(DEFAULT_PHASE_TIMEOUT_SECONDS)
    );
}

#[test]
fn orchestrator_report_is_atomic_and_machine_readable() {
    let root =
        std::env::temp_dir().join(format!("terlan-rust-suite-report-{}", std::process::id()));
    let path = root.join("report.json");
    let results = [PhaseResult {
        name: "fixture",
        tier: ValidationTier::FastUnit,
        executor: "cargo-build",
        wall_time_ms: 12,
        outcome: "passed",
    }];

    write_report(
        &path,
        "pass",
        1,
        Duration::from_secs(7),
        Duration::from_millis(13),
        &results,
    )
    .expect("write report");
    write_report(
        &path,
        "pass",
        1,
        Duration::from_secs(7),
        Duration::from_millis(14),
        &results,
    )
    .expect("replace report");

    let report = fs::read_to_string(&path).expect("read report");
    assert!(report.contains("\"schema\": \"terlan.rust-test-suite.v3\""));
    assert!(report.contains("\"closed_stdin\": true"));
    assert!(report.contains("\"tier\":\"fast-unit\""));
    assert!(report.contains("\"executor\":\"cargo-build\""));
    assert!(report.contains("\"tier_inventory\""));
    assert!(report.contains("\"tier_inventory_path\""));
    assert!(report.contains("\"owner\":\"vm-multicore-performance-check\""));
    assert!(report.contains("\"cargo_invocation_count\": 1"));
    assert!(report.contains("\"cargo_invocation_maximum\": 2"));
    assert!(report.contains("\"wall_time_ms\": 14"));
    assert_eq!(
        fs::read_dir(&root).expect("read report directory").count(),
        1
    );
    fs::remove_dir_all(root).expect("remove report directory");
}

#[test]
fn every_ignored_rust_test_has_one_explicit_inventory_row() {
    validate_tier_inventory().expect("valid tier inventory");
    let mut ignored = Vec::new();
    collect_ignored_tests(Path::new("crates/terlan"), &mut ignored);
    collect_ignored_tests(Path::new("crates/terlan-test-orchestrator"), &mut ignored);

    ignored.sort();
    let mut index = 0;
    while index < ignored.len() {
        let name = &ignored[index];
        let source_count = ignored[index..]
            .iter()
            .take_while(|candidate| candidate == &name)
            .count();
        let inventory_count = TIER_INVENTORY.matches(name.as_str()).count();
        assert_eq!(
            inventory_count, source_count,
            "ignored test `{name}` must have exactly one inventory row per definition"
        );
        index += source_count;
    }
}

fn collect_ignored_tests(path: &Path, output: &mut Vec<String>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read Rust source directory") {
            collect_ignored_tests(&entry.expect("read Rust source entry").path(), output);
        }
        return;
    }
    if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
        return;
    }
    let source = fs::read_to_string(path).expect("read Rust source");
    let mut ignored = false;
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with("#[ignore") {
            ignored = true;
        } else if ignored && line.starts_with("fn ") {
            let name = line
                .strip_prefix("fn ")
                .and_then(|function| function.split_once('(').map(|(name, _)| name))
                .expect("ignored test function name");
            output.push(name.to_string());
            ignored = false;
        }
    }
}

#[test]
fn every_orchestrated_phase_has_one_known_tier() {
    let phases = test_phases(false);
    assert!(
        phases
            .iter()
            .filter(|phase| phase.executor == PhaseExecutor::Cargo)
            .count()
            < MAX_CARGO_PHASES
    );
    assert!(phases.iter().all(|phase| matches!(
        phase.tier,
        ValidationTier::FastUnit | ValidationTier::Integration | ValidationTier::AotNativeLink
    )));
    assert_eq!(
        phases
            .iter()
            .filter(|phase| phase.tier == ValidationTier::FastUnit)
            .count(),
        1
    );
    assert_eq!(
        phases
            .iter()
            .filter(|phase| phase.tier == ValidationTier::AotNativeLink)
            .count(),
        1
    );

    for tier in ValidationTier::ALL {
        let owned_by_orchestrator = phases.iter().any(|phase| phase.tier == tier);
        let owned_externally = EXTERNAL_TIER_OWNERS.iter().any(|owner| owner.tier == tier);
        assert_ne!(
            owned_by_orchestrator,
            owned_externally,
            "tier {} must have exactly one execution owner",
            tier.as_str()
        );
    }
}

#[test]
fn cargo_json_selects_one_existing_terlan_library_harness() {
    let root =
        std::env::temp_dir().join(format!("terlan-harness-selection-{}", std::process::id()));
    fs::create_dir_all(&root).expect("create harness fixture");
    let executable = root.join("terlan-fixture");
    fs::write(&executable, b"fixture").expect("write harness fixture");
    let output = format!(
        "{{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"dependency\",\"kind\":[\"lib\"]}},\"profile\":{{\"test\":false}},\"executable\":null}}\n{{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"terlan\",\"kind\":[\"lib\"]}},\"profile\":{{\"test\":true}},\"executable\":{}}}\n",
        serde_json::to_string(&executable).expect("encode harness path")
    );

    assert_eq!(
        select_terlan_harness(output.as_bytes()).expect("select harness"),
        executable
    );
    fs::remove_dir_all(root).expect("remove harness fixture");
}

#[test]
fn cargo_json_rejects_missing_terlan_library_harness() {
    let output = b"{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"terlan\",\"kind\":[\"lib\"]},\"profile\":{\"test\":true},\"executable\":\"/missing/terlan-harness\"}\n";
    let error = select_terlan_harness(output).expect_err("missing harness must fail");

    assert_eq!(error.outcome, "artifact-missing");
}

#[test]
fn cargo_json_rejects_ambiguous_terlan_library_harnesses() {
    let root =
        std::env::temp_dir().join(format!("terlan-harness-ambiguity-{}", std::process::id()));
    fs::create_dir_all(&root).expect("create ambiguity fixture");
    let first = root.join("terlan-first");
    let second = root.join("terlan-second");
    fs::write(&first, b"first").expect("write first harness fixture");
    fs::write(&second, b"second").expect("write second harness fixture");
    let output = format!(
        "{{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"terlan\",\"kind\":[\"lib\"]}},\"profile\":{{\"test\":true}},\"executable\":{}}}\n{{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"terlan\",\"kind\":[\"lib\"]}},\"profile\":{{\"test\":true}},\"executable\":{}}}\n",
        serde_json::to_string(&first).expect("encode first harness path"),
        serde_json::to_string(&second).expect("encode second harness path")
    );
    let error =
        select_terlan_harness(output.as_bytes()).expect_err("ambiguous harnesses must fail");

    assert_eq!(error.outcome, "artifact-ambiguous");
    fs::remove_dir_all(root).expect("remove ambiguity fixture");
}

#[test]
fn closed_stdin_probe_observes_eof_without_waiting_for_a_terminal() {
    let mut command = eof_probe_command();
    let started = Instant::now();

    run_closed_command(&mut command, Duration::from_secs(2))
        .expect("EOF probe must terminate successfully");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn closed_child_timeout_is_classified_and_terminated() {
    let mut command = sleeping_command();
    let error = run_closed_command(&mut command, Duration::from_millis(20))
        .expect_err("sleeping child must time out");

    assert_eq!(error.outcome, "timed-out");
}

#[cfg(unix)]
fn eof_probe_command() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "if IFS= read -r value; then exit 9; else exit 0; fi"]);
    command
}

#[cfg(windows)]
fn eof_probe_command() -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "set /p value= && exit /b 9 || exit /b 0"]);
    command
}

#[cfg(unix)]
fn sleeping_command() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 2"]);
    command
}

#[cfg(windows)]
fn sleeping_command() -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "ping -n 3 127.0.0.1 >NUL"]);
    command
}
