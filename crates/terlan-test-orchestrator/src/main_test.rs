use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::{
    cargo_program, phase_timeout_from, run_closed_command, run_closed_command_captured,
    test_phases, validate_tier_inventory, write_report, PhaseExecutor, PhaseResult, ValidationTier,
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
fn workspace_execution_includes_terlan_integration_targets_under_one_build_selection() {
    let phases = test_phases(false);
    let phase = phases
        .iter()
        .find(|phase| phase.executor == PhaseExecutor::CargoNative)
        .unwrap();
    assert_eq!(phase.args, super::phase_plan::workspace_native_arguments());
    assert!(phase.args.contains(&"--workspace"));
    assert!(phase.args.contains(&"--tests"));
    assert!(!phase.args.contains(&"--exclude"));
    assert_eq!(
        phases
            .iter()
            .filter(|phase| phase.executor.is_cargo())
            .count()
            + 1,
        MAX_CARGO_PHASES
    );
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
    assert_eq!(terlan_library_phases.count(), 9);
}

#[test]
fn orchestrator_runs_ignored_contract_once_in_the_library() {
    let phases = test_phases(false);
    let ignored: Vec<_> = phases
        .iter()
        .filter(|phase| phase.args.contains(&"--ignored"))
        .collect();

    assert_eq!(ignored.len(), 7);
    assert!(ignored
        .iter()
        .all(|phase| phase.executor == PhaseExecutor::TerlanHarness));
    assert!(ignored.iter().all(|phase| phase.args.contains(&"--exact")));
    assert!(ignored
        .iter()
        .any(|phase| phase.name == "generated C++ package evidence"));
    assert!(ignored
        .iter()
        .any(|phase| phase.name == "generated capability event pump"));
    assert!(ignored
        .iter()
        .any(|phase| phase.name == "EPMD discovery transport full cycle"));
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
        child_pid: Some(123),
        test_execution: None,
    }];
    let mut file = super::report_file::ReportFile::open(&path).expect("own report");

    write_report(
        &mut file,
        "pass",
        1,
        Duration::from_secs(7),
        Duration::from_millis(13),
        &results,
        &super::validation_inputs::ValidationInputs::default(),
    )
    .expect("write report");
    write_report(
        &mut file,
        "pass",
        1,
        Duration::from_secs(7),
        Duration::from_millis(14),
        &results,
        &super::validation_inputs::ValidationInputs::default(),
    )
    .expect("replace report");

    let report = fs::read_to_string(&path).expect("read report");
    assert!(report.contains("\"schema\": \"terlan.rust-test-suite.v4\""));
    assert!(report.contains("\"closed_stdin\": true"));
    assert!(report.contains("\"tier\":\"fast-unit\""));
    assert!(report.contains("\"executor\":\"cargo-build\""));
    assert!(report.contains("\"tier_inventory\""));
    assert!(report.contains("\"tier_inventory_path\""));
    assert!(report.contains("\"owner\":\"vm-multicore-performance-record\""));
    assert!(report.contains("\"direct_cargo_launch_count\": 1"));
    assert!(report.contains("\"direct_cargo_launch_maximum\": 3"));
    assert!(report.contains("\"direct_process_launch_count\": 1"));
    assert!(report.contains("\"child_pid\":123"));
    assert!(report.contains("\"wall_time_ms\": 14"));
    assert_eq!(
        fs::read_dir(&root).expect("read report directory").count(),
        2
    );
    drop(file);
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
            .filter(|phase| phase.executor.is_cargo())
            .count()
            < MAX_CARGO_PHASES
    );
    assert!(phases.iter().all(|phase| matches!(
        phase.tier,
        ValidationTier::FastUnit
            | ValidationTier::Integration
            | ValidationTier::AotNativeLink
            | ValidationTier::ControlledHost
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
        5
    );

    for tier in ValidationTier::ALL {
        let owned_by_orchestrator = phases.iter().any(|phase| phase.tier == tier);
        let owned_externally = EXTERNAL_TIER_OWNERS.iter().any(|owner| owner.tier == tier);
        assert!(
            owned_by_orchestrator || owned_externally,
            "tier {} must have an execution owner",
            tier.as_str()
        );
    }
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn cargo_capture_reaps_descendants_without_waiting_for_their_pipes() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 5 & printf '{}\\n'; exit 0"]);
    let started = Instant::now();
    assert_eq!(
        run_closed_command_captured(&mut command, Duration::from_secs(2)).unwrap(),
        b"{}\n"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn cargo_capture_output_cannot_grow_without_a_bound() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "head -c 16777217 /dev/zero"]);
    let error = run_closed_command_captured(&mut command, Duration::from_secs(5)).unwrap_err();
    assert_eq!(error.outcome, "output-limit");
}

#[test]
fn cargo_capture_selects_and_executes_a_real_compiled_harness() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "terlan-cargo-capture-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("exclusively reserve Cargo fixture");
    let root = CargoFixture(path);
    fs::write(root.0.join("Cargo.toml"), "[package]\nname = \"terlan\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"lib.rs\"\n[features]\nquality-tools=[]\neditor-lsp=[]\nbenchmark-tools=[]\n[workspace]\n").unwrap();
    fs::write(
        root.0.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"terlan\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    fs::write(
        root.0.join("lib.rs"),
        "#[test]\nfn owned_cargo_probe() { assert_eq!(2 + 2, 4); }\n",
    )
    .unwrap();
    let environment = crate::execution_environment::ExecutionEnvironment::from_entries(
        std::env::vars_os().chain([(
            "CARGO_TARGET_DIR".into(),
            root.0.join("target").into_os_string(),
        )]),
        &root.0,
    )
    .unwrap();
    let mut launches = 0;
    let harness = crate::prepare_terlan_harness(
        Path::new(&cargo_program()),
        &environment,
        terlan_process_owner::ProcessControl::new(Duration::from_secs(30)),
        &mut |_| {
            launches += 1;
            Ok(())
        },
    )
    .expect("exact declared harness selected")
    .executable;
    assert_eq!(launches, 1);
    assert!(harness.starts_with(root.0.join("target")));
    let output = run_closed_command_captured(
        Command::new(harness).args(["owned_cargo_probe", "--exact"]),
        Duration::from_secs(5),
    )
    .expect("run the compiled test, not merely select a path");
    assert!(String::from_utf8(output)
        .unwrap()
        .contains("1 passed; 0 failed; 0 ignored;"));
}

pub(super) struct CargoFixture(pub(super) std::path::PathBuf);

pub(super) fn temporary_fixture(label: &str) -> CargoFixture {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("terlan-{label}-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).expect("exclusively reserve fixture");
    CargoFixture(path)
}

impl Drop for CargoFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("clean only the owned terminal Cargo fixture");
    }
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
