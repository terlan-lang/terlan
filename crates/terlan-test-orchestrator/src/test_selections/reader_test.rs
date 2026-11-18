use super::*;
use crate::test_inventory::TestPlan;
use crate::test_orchestrator_test::temporary_fixture;
use crate::ValidationTier;

fn evidence(path: &Path) -> (Value, Value) {
    let control = ProcessControl::new(Duration::from_secs(5));
    let selection = ExpectedTests {
        passed: BTreeSet::from(["module::owned".into()]),
        ignored: BTreeSet::from(["module::ignored".into()]),
        filtered: 1,
    };
    let plan = TestPlan {
        all: ["module::owned", "module::delegated", "module::ignored"]
            .map(String::from)
            .into(),
        ignored: ["module::ignored"].map(String::from).into(),
        selections: BTreeMap::from([("normal", selection.clone())]),
    };
    let harness = json!({"role":"terlan-library-harness", "identity_sha256":"a".repeat(64)});
    let phase = TestPhase {
        name: "normal",
        tier: ValidationTier::FastUnit,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![],
        environment: vec![],
        shard: None,
        test_threads: None,
    };
    let mut inventory =
        TestSelections::create(path, "completed-suite", &[phase], &plan, &harness, control)
            .unwrap();
    let completion = json!({"scope":"admitted-libtest-records-v1", "selection_identity_sha256":selection.identity(), "passed":1,"ignored":1,"filtered":1});
    inventory
        .verify(
            &[PhaseResult {
                name: "normal",
                tier: ValidationTier::FastUnit,
                executor: "direct-terlan-harness",
                wall_time_ms: 1,
                outcome: "passed",
                child_pid: Some(17),
                test_execution: Some(completion.clone()),
                reused: false,
            }],
            control,
        )
        .unwrap();
    let report = json!({"schema":"terlan.rust-test-suite.v4","decision":"pass","run_id":"completed-suite",
        "test_selection_binding":inventory.json(), "executable_binding":{"before":[harness.clone()],"after":[harness],"verified":true},
        "phases":[{"name":"normal","outcome":"passed","executor":"direct-terlan-harness","child_pid":17,"reused":false,"test_execution":completion}]});
    let document = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    (report, document)
}

#[test]
fn historical_reader_accepts_explicit_exact_receipt_completion() {
    let fixture = temporary_fixture("coverage-reader-receipt");
    let (mut report, document) = evidence(&fixture.0.join("names.json"));
    report["phases"][0]["child_pid"] = Value::Null;
    report["phases"][0]["reused"] = json!(true);
    assert!(restored(&report, &document).is_ok());

    report["phases"][0]["child_pid"] = json!(17);
    assert!(restored(&report, &document).is_err());
}

fn restored(report: &Value, document: &Value) -> Result<TestSelections, PhaseFailure> {
    restore(
        report,
        document,
        report["test_selection_binding"]["sha256"].as_str().unwrap(),
        Path::new("archived.json.selections.json"),
    )
}

#[test]
fn historical_reader_reconciles_production_evidence_and_preserves_missing_coverage() {
    let fixture = temporary_fixture("coverage-reader-roundtrip");
    let (report, document) = evidence(&fixture.0.join("names.json"));
    let inventory = restored(&report, &document).unwrap();
    assert_eq!(inventory.inspect_coverage(&[]).unwrap()["uncovered"], 1);
    assert_eq!(
        inventory
            .inspect_coverage(&["module::owned", "--exact"])
            .unwrap()["covered"],
        true
    );
    assert_eq!(
        inventory
            .inspect_coverage(&["--skip", "delegated"])
            .unwrap()["covered"],
        true
    );
    assert_eq!(
        inventory.inspect_coverage(&["--ignored"]).unwrap()["covered"],
        false
    );
    assert_eq!(
        inventory.inspect_coverage(&["absent"]).unwrap()["covered"],
        false
    );
    assert_eq!(inventory.json()["reusable"], false);
}

#[test]
fn historical_reader_bounds_phases_by_compiled_tests_not_a_magic_number() {
    let phase_count = 257_usize;
    let names = (0..phase_count)
        .map(|index| format!("module::owned_{index}"))
        .collect::<BTreeSet<_>>();
    let phases = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let selected = ExpectedTests {
                passed: BTreeSet::from([name.clone()]),
                ignored: BTreeSet::new(),
                filtered: phase_count - 1,
            };
            json!({"phase":format!("phase-{index}"), "passed":selected.passed,
                "ignored":selected.ignored, "filtered":selected.filtered,
                "selection_identity_sha256":selected.identity()})
        })
        .collect::<Vec<_>>();
    let completions = phases
        .iter()
        .map(|phase| {
            json!({"name":phase["phase"], "outcome":"passed",
                "executor":"direct-terlan-harness", "child_pid":17, "reused":false,
                "test_execution":{"scope":"admitted-libtest-records-v1",
                    "selection_identity_sha256":phase["selection_identity_sha256"],
                    "passed":1, "ignored":0, "filtered":phase_count - 1}})
        })
        .collect::<Vec<_>>();
    let harness = json!({"role":"terlan-library-harness", "identity_sha256":"a".repeat(64)});
    let coverage = json!({
        "normal":{"scope":"completed-main-harness-name-coverage-v1", "selected":phase_count,
            "passed":phase_count, "uncovered":0, "covered":true, "reusable":false},
        "ignored":{"scope":"completed-main-harness-name-coverage-v1", "selected":0,
            "passed":0, "uncovered":0, "covered":false, "reusable":false}
    });
    let digest = "b".repeat(64);
    let document = json!({"schema":"terlan.rust-test-selections.v2",
        "suite_run_id":"completed-suite", "scope":"admitted-main-harness-selections-v1",
        "reusable":false, "package":"terlan", "target":"lib",
        "features":crate::VALIDATION_FEATURES, "harness":harness,
        "compiled":{"all":names, "ignored":[]}, "phases":phases});
    let report = json!({"schema":"terlan.rust-test-suite.v4", "decision":"pass",
        "run_id":"completed-suite", "executable_binding":{"before":[harness.clone()],
            "after":[harness], "verified":true}, "phases":completions,
        "test_selection_binding":{"suite_run_id":"completed-suite",
            "scope":"admitted-main-harness-selections-v1", "sha256":digest,
            "verified":true, "reusable":false, "phases":phase_count,
            "tests":phase_count, "compiled_tests":phase_count,
            "originally_ignored":0, "coverage":coverage}});

    assert!(restore(
        &report,
        &document,
        report["test_selection_binding"]["sha256"].as_str().unwrap(),
        Path::new("large-suite.json.selections.json"),
    )
    .is_ok());
}

#[test]
fn historical_reader_rejects_failed_incomplete_cross_run_and_forged_completions() {
    let fixture = temporary_fixture("coverage-reader-report-rejection");
    let (valid, document) = evidence(&fixture.0.join("names.json"));
    for (pointer, value) in [
        ("/schema", json!("terlan.rust-test-suite.v3")),
        ("/decision", json!("running")),
        ("/run_id", json!("another-run")),
        ("/test_selection_binding/verified", json!(false)),
        ("/test_selection_binding/reusable", json!(true)),
        ("/test_selection_binding/tests", json!(2)),
        ("/test_selection_binding/compiled_tests", json!(2)),
        (
            "/test_selection_binding/coverage/normal/covered",
            json!(true),
        ),
        ("/executable_binding/verified", json!(false)),
        (
            "/executable_binding/after/0/identity_sha256",
            json!("b".repeat(64)),
        ),
        ("/phases/0/outcome", json!("failed")),
        ("/phases/0/child_pid", json!(0)),
        ("/phases/0/test_execution/passed", json!(2)),
        (
            "/phases/0/test_execution/scope",
            json!("cargo-summary-presence-v1"),
        ),
        (
            "/phases/0/test_execution/selection_identity_sha256",
            json!("b".repeat(64)),
        ),
        ("/phases", json!([])),
    ] {
        let mut report = valid.clone();
        *report.pointer_mut(pointer).unwrap() = value;
        assert!(restored(&report, &document).is_err(), "{pointer}");
    }
    let mut duplicate = valid.clone();
    duplicate["phases"]
        .as_array_mut()
        .unwrap()
        .push(valid["phases"][0].clone());
    assert!(restored(&duplicate, &document).is_err());
}

#[test]
fn historical_reader_rejects_corrupt_inventory_even_with_matching_document_digest() {
    let fixture = temporary_fixture("coverage-reader-inventory-rejection");
    let (report, valid) = evidence(&fixture.0.join("names.json"));
    for (pointer, value) in [
        ("/schema", json!("terlan.rust-test-selections.v1")),
        ("/features", json!("quality-tools")),
        ("/target", json!("bin")),
        ("/harness/identity_sha256", json!("b".repeat(64))),
        ("/compiled/all", json!([])),
        ("/compiled/all", json!(["module::owned", "module::owned"])),
        ("/compiled/ignored", json!(["absent"])),
        (
            "/phases/0/passed",
            json!(["module::owned", "module::owned"]),
        ),
        ("/phases/0/ignored", json!(["module::owned"])),
        ("/phases/0/filtered", json!(u64::MAX)),
        ("/phases/0/selection_identity_sha256", json!("forged")),
        ("/phases", json!([])),
    ] {
        let mut document = valid.clone();
        *document.pointer_mut(pointer).unwrap() = value;
        assert!(restored(&report, &document).is_err(), "{pointer}");
    }
    let mut duplicate = valid.clone();
    duplicate["phases"]
        .as_array_mut()
        .unwrap()
        .push(valid["phases"][0].clone());
    assert!(restored(&report, &duplicate).is_err());
}

#[test]
fn historical_reader_uses_only_fixed_sibling_and_checks_its_actual_bytes() {
    let fixture = temporary_fixture("coverage-reader-files");
    let path = fixture.0.join("suite.json");
    let companion = fixture.0.join("suite.json.selections.json");
    let (mut report, _) = evidence(&companion);
    // Archived pairs remain inspectable after old executable and output paths disappear.
    report["test_selection_binding"]["path"] = json!("/unavailable/original/names.json");
    std::fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    assert!(read(&path, control).is_ok());
    std::fs::write(&companion, "{}").unwrap();
    assert!(read(&path, control).is_err());
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(1024 * 1024 + 1).unwrap();
    assert!(read(&path, control).is_err());
    let mut budget = NameBudget {
        count: MAX_NAMES,
        bytes: 0,
    };
    assert!(budget.read(&json!(["one"])).is_err());
    let mut budget = NameBudget {
        count: 0,
        bytes: MAX_BYTES as usize / 2,
    };
    assert!(budget.read(&json!(["one"])).is_err());
}
