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
            }],
            control,
        )
        .unwrap();
    let report = json!({"schema":"terlan.rust-test-suite.v4","decision":"pass","run_id":"completed-suite",
        "test_selection_binding":inventory.json(), "executable_binding":{"before":[harness.clone()],"after":[harness],"verified":true},
        "phases":[{"name":"normal","outcome":"passed","executor":"direct-terlan-harness","child_pid":17,"test_execution":completion}]});
    let document = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    (report, document)
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
