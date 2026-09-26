use super::*;
use crate::phase_plan::{ignored_std_collection_phase, terlan_integration_phase};
use crate::test_orchestrator_test::temporary_fixture;
use std::fs;

fn names(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn normal_phases() -> Vec<TestPhase> {
    vec![terlan_library_phase(), terlan_integration_phase()]
}

#[test]
fn inventory_parser_rejects_malformed_or_duplicate_records() {
    for output in [
        b"probe: test\nprobe: test\n".as_slice(),
        b"probe: benchmark\n",
        b": test\n",
        b"white space: test\n",
        b"test result: ok. 0 passed\n",
        b"\xff: test\n",
        b"\n",
    ] {
        assert!(parse(output).is_err(), "unexpectedly accepted {output:?}");
    }
    assert_eq!(
        parse(b"one: test\r\ntwo: test\r\n").unwrap(),
        names(&["one", "two"])
    );
    assert!(parse(b"").unwrap().is_empty());
}

#[test]
fn selection_matches_exact_filters_or_filters_and_skips() {
    let inventory = names(&["a", "ab", "b", "quality::probe"]);
    assert_eq!(
        select(&inventory, &["a", "b", "--skip", "ab"]).unwrap(),
        vec![
            &"a".to_string(),
            &"b".to_string(),
            &"quality::probe".to_string()
        ]
    );
    assert_eq!(
        select(&inventory, &["a", "--exact"]).unwrap(),
        vec![&"a".to_string()]
    );
    assert_eq!(
        select(&inventory, &["--exact", "--skip", "a"])
            .unwrap()
            .len(),
        3
    );
    for invalid in [&["--skip"][..], &["--include-ignored"][..], &["--list"][..]] {
        assert!(select(&inventory, invalid).is_err());
    }
}

#[test]
fn phase_expectations_include_selected_ignores_but_exclude_filtered_names() {
    let all = names(&["unit", "future", "quality::probe", "quality::future"]);
    let ignored = names(&["future", "quality::future"]);
    let tiers = "selector\ttier\towner\tisolation\nfuture\tintegration\texternal\tfixture\nquality::future\tintegration\texternal\tfixture\n";
    let expected = validate(&all, &ignored, &normal_phases(), false, tiers).unwrap();
    assert_eq!(expected["Terlan library"].passed, names(&["unit"]));
    assert_eq!(expected["Terlan library"].ignored, names(&["future"]));
    assert_eq!(expected["Terlan library"].filtered, 2);
    assert_eq!(
        expected["Terlan union-feature integration"].passed,
        names(&["quality::probe"])
    );
    assert_eq!(
        expected["Terlan union-feature integration"].ignored,
        names(&["quality::future"])
    );
    assert_eq!(expected["Terlan union-feature integration"].filtered, 2);
}

#[test]
fn compiled_inventory_requires_complete_nonoverlapping_nonempty_phases() {
    let all = names(&["unit", "quality::probe", "lsp::probe"]);
    let ignored = BTreeSet::new();
    validate(&all, &ignored, &normal_phases(), false, TIER_INVENTORY).unwrap();
    validate(
        &all,
        &ignored,
        &[terlan_integration_phase()],
        true,
        TIER_INVENTORY,
    )
    .unwrap();
    assert!(validate(
        &all,
        &ignored,
        &[terlan_integration_phase()],
        false,
        TIER_INVENTORY
    )
    .is_err());
    assert!(validate(&all, &ignored, &normal_phases(), true, TIER_INVENTORY).is_err());
    assert!(validate(&BTreeSet::new(), &ignored, &[], false, TIER_INVENTORY).is_err());
    assert!(validate(&all, &names(&["missing"]), &[], false, TIER_INVENTORY).is_err());
    let mut duplicate = normal_phases();
    duplicate.push(terlan_integration_phase());
    assert!(validate(&all, &ignored, &duplicate, false, TIER_INVENTORY).is_err());
    let mut stale = normal_phases();
    stale.push(ignored_std_collection_phase());
    assert!(validate(&all, &ignored, &stale, false, TIER_INVENTORY)
        .unwrap_err()
        .detail
        .contains("zero runnable tests"));
}

#[test]
fn ignored_ownership_must_agree_with_the_declared_tier() {
    let phase = ignored_std_collection_phase();
    let selector = phase.args[0];
    let all = names(&["unit", "quality::probe", selector]);
    let ignored = names(&[selector]);
    let mut phases = normal_phases();
    phases.push(phase);
    validate(&all, &ignored, &phases, false, TIER_INVENTORY).unwrap();
    assert!(validate(&all, &ignored, &normal_phases(), false, TIER_INVENTORY).is_err());
    assert!(validate(&all, &BTreeSet::new(), &phases, false, TIER_INVENTORY).is_err());
    for row in [
        format!("{selector}\tintegration\tother-owner\tisolation"),
        format!("{selector}\tfast-unit\tterlan-test-orchestrator\tisolation"),
        format!("{selector}\tintegration\tterlan-test-orchestrator"),
        String::new(),
    ] {
        let tiers = format!("selector\ttier\towner\tisolation\n{row}\n");
        assert!(validate(&all, &ignored, &phases, false, &tiers).is_err());
    }
}

#[test]
fn external_ignored_tests_are_declared_but_not_replayed() {
    let selector = "commands::serve::handler_cache::multicore_performance_test::multicore_runtime_width_matrix_records_workloads_and_owner_overlap";
    validate(
        &names(&["unit", "quality::probe", selector]),
        &names(&[selector]),
        &normal_phases(),
        false,
        TIER_INVENTORY,
    )
    .unwrap();
    let unknown = "undeclared_ignored_test";
    assert!(validate(
        &names(&["unit", "quality::probe", unknown]),
        &names(&[unknown]),
        &normal_phases(),
        false,
        TIER_INVENTORY
    )
    .is_err());
}

#[test]
fn real_compiled_inventory_checks_selectors_without_executing_tests() {
    let fixture = temporary_fixture("test-inventory");
    fs::write(fixture.0.join("Cargo.toml"), "[package]\nname = \"terlan\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"lib.rs\"\n[workspace]\n").unwrap();
    fs::write(
        fixture.0.join("lib.rs"),
        concat!(
            "#[test] fn unit() { panic!(\"inventory must not execute tests\"); }\n",
            "mod quality { #[test] fn probe() { panic!(\"not an execution phase\"); } }\n",
            "#[test] #[ignore] fn separate_owner() { panic!(\"ignored\"); }\n",
        ),
    )
    .unwrap();
    let output = run_closed_command_captured(
        Command::new(cargo_program())
            .current_dir(&fixture.0)
            .args([
                "test",
                "--offline",
                "--lib",
                "--no-run",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(fixture.0.join("target")),
        Duration::from_secs(30),
    )
    .unwrap();
    let harness = crate::cargo_harness_admission::select(
        &output,
        &fixture.0,
        ProcessControl::new(Duration::from_secs(5)),
    )
    .unwrap()
    .executable;
    let mut launches = 0;
    let mut observe = |pid| {
        assert!(pid > 0);
        launches += 1;
        Ok(())
    };
    let control = ProcessControl::new(Duration::from_secs(5));
    let environment = execution_environment::ExecutionEnvironment::capture().unwrap();
    let all = list(&harness, false, &environment, control, &mut observe).unwrap();
    let ignored = list(&harness, true, &environment, control, &mut observe).unwrap();
    assert_eq!(launches, 2);
    assert_eq!(all, names(&["unit", "quality::probe", "separate_owner"]));
    assert_eq!(ignored, names(&["separate_owner"]));
    let tiers =
        "selector\ttier\towner\tisolation\nseparate_owner\tintegration\texternal\tfixture\n";
    validate(&all, &ignored, &normal_phases(), false, tiers).unwrap();
    let mut stale = normal_phases();
    stale.push(ignored_std_collection_phase());
    assert!(validate(&all, &ignored, &stale, false, tiers).is_err());
}

#[cfg(unix)]
#[test]
fn empty_successful_executable_fails_before_test_phases() {
    let fixture = temporary_fixture("empty-inventory");
    let report = fixture.0.join("report.json");
    let mut ledger = launch_ledger::LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    let mut executables = crate::executable_binding::ExecutableBinding::capture(
        &[("fixture", Path::new("/bin/true").to_path_buf())],
        control,
    )
    .unwrap();
    executables
        .bind_harness(Path::new("/bin/true"), control)
        .unwrap();
    let error = prepare(
        &normal_phases(),
        false,
        ProcessControl::new(Duration::from_secs(5)),
        &executables,
        &execution_environment::ExecutionEnvironment::capture().unwrap(),
        &mut ledger,
    )
    .unwrap_err();
    assert_eq!(error.outcome, "test-inventory-failed");
    assert!(ledger.finish().is_err());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(value["decision"], "fail");
    assert_eq!(value["direct_process_launch_count"], 2);
    assert_eq!(value["direct_cargo_launch_count"], 0);
    assert_eq!(value["phases"][1]["outcome"], "test-inventory-failed");
}
