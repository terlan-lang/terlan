use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::{
    cargo_program, failed_closeout_control, phase_timeout_from, prebuilt_binary_from,
    prebuilt_binary_override_name, run_closed_command, run_closed_command_captured, test_phases,
    validate_tier_inventory, write_report, PhaseExecutor, PhaseResult, ValidationTier,
    DEFAULT_PHASE_TIMEOUT_SECONDS, EXTERNAL_TIER_OWNERS, FAST_UNIT_MAX_TIMEOUT_SECONDS,
    INTEGRATION_FILTERS, MAX_CARGO_PHASES, TIER_INVENTORY,
};
use terlan_process_owner::ProcessControl;

#[test]
fn failed_suite_closeout_uses_fresh_bounded_control() {
    let cancelled = AtomicBool::new(true);
    let cancelled_control =
        ProcessControl::new(Duration::from_secs(5)).with_cancellation(&cancelled);
    assert!(cancelled_control
        .run(&mut version_probe_command(), |_| Ok(()))
        .is_err());
    assert!(failed_closeout_control()
        .run(&mut version_probe_command(), |_| Ok(()))
        .is_ok());
}

#[test]
fn suite_program_overrides_are_explicit_and_fall_back_to_debug_outputs() {
    assert_eq!(
        prebuilt_binary_override_name("terlc"),
        Some("TERLAN_RUST_SUITE_TERLC")
    );
    assert_eq!(
        prebuilt_binary_override_name("terlan-vm"),
        Some("TERLAN_RUST_SUITE_VM")
    );
    assert_eq!(
        prebuilt_binary_override_name("terlan-native-worker"),
        Some("TERLAN_RUST_SUITE_NATIVE_WORKER")
    );
    assert_eq!(prebuilt_binary_override_name("unknown"), None);
    assert_eq!(
        prebuilt_binary_from("terlc", Some("/immutable/terlc".into())),
        std::path::PathBuf::from("/immutable/terlc")
    );
    assert_eq!(
        prebuilt_binary_from("terlc", None),
        std::path::PathBuf::from("target/debug")
            .join(format!("terlc{}", std::env::consts::EXE_SUFFIX))
    );
}

#[test]
fn compact_fixture_plan_preserves_owners_without_production_shards() {
    let phases = super::phase_plan::compact_test_phases(false);

    assert_eq!(
        phases
            .iter()
            .filter(|phase| phase.name == "Terlan library")
            .count(),
        1
    );
    assert!(!phases
        .iter()
        .any(|phase| phase.name.starts_with("Terlan build ")));
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
fn orchestrator_runs_shared_runtime_tests_in_the_library_harness() {
    let phases = test_phases(false);
    let integration = phases.first().expect("Terlan integration phase");
    assert_eq!(integration.name, "Terlan union-feature integration");
    let library = phases
        .iter()
        .find(|phase| phase.name == "Terlan build arguments")
        .expect("Terlan library phase");

    assert_eq!(library.name, "Terlan build arguments");
    assert_eq!(library.executor, PhaseExecutor::TerlanHarness);
    for filter in INTEGRATION_FILTERS {
        assert!(library
            .args
            .windows(2)
            .any(|arguments| arguments == ["--skip", filter]));
    }
}

#[test]
fn isolated_serve_lanes_declare_bounded_parallel_execution() {
    let phases = test_phases(false);
    for name in [
        "Terlan serve cache invocation",
        "Terlan serve response policy",
        "Terlan serve request routing",
        "Terlan Hyper JSON bodies",
        "Terlan Hyper SSE source",
    ] {
        let phase = phases.iter().find(|phase| phase.name == name).unwrap();
        assert_eq!(phase.test_threads, Some(4), "{name}");
    }
    assert_eq!(phases[0].test_threads, None);
}

#[test]
fn every_broad_compiler_family_has_thirty_two_bounded_shards() {
    let phases = test_phases(false);
    for prefix in [
        "Terlan command remainder shard ",
        "Terlan native IR shard ",
        "Terlan type checker shard ",
        "Terlan compiler shard ",
        "Terlan runtime shard ",
        "Terlan library remainder shard ",
    ] {
        let selected = phases
            .iter()
            .filter(|phase| phase.name.starts_with(prefix))
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 32, "{prefix}");
        assert!(selected.iter().all(|phase| phase.name.ends_with("/32")));
    }
}

#[test]
fn every_candidate_contract_precedes_broad_and_platform_validation() {
    let phases = test_phases(false);
    assert_eq!(
        phases
            .iter()
            .take(2)
            .map(|phase| phase.name)
            .collect::<Vec<_>>(),
        [
            "Terlan union-feature integration",
            "ignored std collection contract",
        ]
    );
    let broad = phases
        .iter()
        .position(|phase| phase.name == "Terlan build arguments")
        .expect("broad compiler suite");
    let workspace = phases
        .iter()
        .position(|phase| phase.name == "workspace support unit targets")
        .expect("workspace support tier");
    let platform = phases
        .iter()
        .position(|phase| phase.name == "generated C++ package evidence")
        .expect("platform evidence");
    assert_eq!(broad, 2);
    assert!(workspace > broad);
    assert!(platform > broad);
    assert!(phases[platform..].iter().all(|phase| matches!(
        phase.tier,
        ValidationTier::AotNativeLink | ValidationTier::ControlledHost
    )));
}

#[test]
fn workspace_execution_separates_owned_units_from_integration_targets() {
    let phases = test_phases(false);
    let support = phases
        .iter()
        .find(|phase| phase.name == "workspace support unit targets")
        .unwrap();
    let integration = phases
        .iter()
        .find(|phase| phase.name == "workspace integration targets")
        .unwrap();
    assert_eq!(
        support.args,
        super::phase_plan::workspace_support_arguments()
    );
    assert!(support.args.contains(&"--workspace"));
    assert!(support.args.contains(&"--exclude"));
    assert!(support.args.contains(&"--lib"));
    assert!(support.args.contains(&"--bins"));
    assert!(!support.args.contains(&"--tests"));
    assert_eq!(
        integration.args,
        super::phase_plan::workspace_integration_arguments()
    );
    assert!(integration.args.contains(&"--workspace"));
    assert!(integration
        .args
        .windows(2)
        .any(|args| args == ["--test", "*"]));
    assert!(!integration.args.contains(&"--lib"));
    assert!(!integration.args.contains(&"--bins"));
    assert!(!integration.args.contains(&"--features"));
    assert!(integration
        .args
        .windows(2)
        .any(|args| args == ["--profile", "dev"]));
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
fn harness_preparation_builds_only_the_library_before_workspace_targets() {
    let harness = super::phase_plan::terlan_harness_arguments();
    let workspace = super::phase_plan::workspace_integration_arguments();

    assert!(harness.windows(2).any(|args| args == ["-p", "terlan"]));
    assert!(harness.contains(&"--lib"));
    assert!(!harness.contains(&"--workspace"));
    assert!(!harness.contains(&"--tests"));
    assert!(workspace.contains(&"--workspace"));
    assert!(workspace.windows(2).any(|args| args == ["--test", "*"]));
    assert!(!workspace.contains(&"--lib"));
    assert_eq!(
        harness
            .iter()
            .filter(|argument| workspace.contains(argument))
            .copied()
            .collect::<Vec<_>>(),
        super::phase_plan::cargo_test_arguments(&[])
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
    assert_eq!(
        terlan_library_phases.count(),
        super::phase_plan::terlan_library_partitions().len()
            + 8
            + 2 * usize::from(cfg!(target_os = "linux"))
    );
}

#[test]
fn orchestrator_runs_ignored_contract_once_in_the_library() {
    let phases = test_phases(false);
    let ignored: Vec<_> = phases
        .iter()
        .filter(|phase| phase.args.contains(&"--ignored"))
        .collect();

    assert_eq!(
        ignored.len(),
        7 + 2 * usize::from(cfg!(target_os = "linux"))
    );
    assert_eq!(
        ignored
            .iter()
            .filter(|phase| phase.name == "public durable storage AOT restart")
            .count(),
        usize::from(cfg!(target_os = "linux"))
    );
    assert_eq!(
        ignored
            .iter()
            .filter(|phase| phase.name == "capability worker durable storage")
            .count(),
        usize::from(cfg!(target_os = "linux"))
    );
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
    let public_storage = ignored
        .iter()
        .find(|phase| phase.name == "public durable storage AOT restart");
    if cfg!(target_os = "linux") {
        let public_storage = public_storage.unwrap();
        for route in [
            "TERLAN_TEST_COMPILER",
            "TERLAN_TEST_VM",
            "TERLAN_TEST_CAPABILITY_WORKER",
        ] {
            assert!(public_storage
                .environment
                .iter()
                .any(|(name, _)| *name == route));
        }
    }
}

#[test]
fn release_orchestrator_leaves_only_normal_library_tests_to_coverage() {
    let phases = test_phases(true);

    assert_eq!(phases[0].name, "Terlan union-feature integration");
    assert_eq!(phases[1].name, "ignored std collection contract");
    assert_eq!(phases[2].name, "workspace support unit targets");
    assert_eq!(phases[3].name, "workspace integration targets");
    assert!(!phases.iter().any(|phase| phase.name == "Terlan library"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "workspace support unit targets"));
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
fn fast_unit_phases_allow_load_variance_but_remain_bounded() {
    assert_eq!(
        ValidationTier::FastUnit.maximum_timeout(),
        Some(Duration::from_secs(FAST_UNIT_MAX_TIMEOUT_SECONDS))
    );
    assert_eq!(ValidationTier::Integration.maximum_timeout(), None);
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
        reused: false,
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
    assert!(report.contains("\"direct_cargo_launch_maximum\": 4"));
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
        super::phase_plan::terlan_library_partitions().len()
    );
    assert_eq!(
        phases
            .iter()
            .filter(|phase| phase.tier == ValidationTier::AotNativeLink)
            .count(),
        5 + 2 * usize::from(cfg!(target_os = "linux"))
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
    fs::write(root.0.join("Cargo.toml"), "[package]\nname = \"terlan\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"lib.rs\"\n[features]\nquality-tools=[]\neditor-lsp=[]\nbenchmark-tools=[]\nserve-runtime-bin=[]\nvalidation-union=[\"quality-tools\",\"editor-lsp\",\"benchmark-tools\",\"serve-runtime-bin\"]\n[[bin]]\nname = \"terlan-serve-runtime\"\npath = \"runtime.rs\"\nrequired-features = [\"serve-runtime-bin\"]\n[[test]]\nname = \"paired\"\npath = \"paired.rs\"\n[workspace]\n").unwrap();
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
    fs::write(
        root.0.join("runtime.rs"),
        "fn main() { println!(\"paired runtime\"); }\n",
    )
    .unwrap();
    fs::write(
        root.0.join("paired.rs"),
        "#[test] fn paired_runtime() { assert!(std::path::Path::new(env!(\"CARGO_BIN_EXE_terlan-serve-runtime\")).is_file()); }\n",
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
    let runtime = root.0.join("target/debug").join(format!(
        "terlan-serve-runtime{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        !runtime.exists(),
        "library harness preparation must not build unrelated binaries"
    );
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

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn temporary_fixture(label: &str) -> CargoFixture {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    reserve_temporary_fixture(label, stamp)
}

fn reserve_temporary_fixture(label: &str, stamp: u128) -> CargoFixture {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "terlan-{label}-{}-{stamp}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("exclusively reserve fixture");
    CargoFixture(path)
}

#[test]
fn fixture_reservations_remain_unique_at_one_clock_tick() {
    let first = reserve_temporary_fixture("shared-clock", 1);
    let second = reserve_temporary_fixture("shared-clock", 1);

    assert_ne!(first.0, second.0);
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

fn version_probe_command() -> Command {
    let mut command = Command::new("rustc");
    command.arg("--version");
    command
}
