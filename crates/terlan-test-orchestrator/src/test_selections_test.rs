use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use crate::ValidationTier;
use std::time::Duration;

fn planned(
    mut selections: BTreeMap<&'static str, ExpectedTests>,
) -> crate::test_inventory::TestPlan {
    let all = selections
        .values()
        .flat_map(|selection| selection.passed.iter().chain(&selection.ignored))
        .cloned()
        .collect::<BTreeSet<_>>();
    let ignored = selections
        .values()
        .flat_map(|selection| &selection.ignored)
        .cloned()
        .collect();
    for selection in selections.values_mut() {
        selection.filtered = all.len() - selection.passed.len() - selection.ignored.len();
    }
    crate::test_inventory::TestPlan {
        all,
        ignored,
        selections,
    }
}

fn phase(name: &'static str) -> TestPhase {
    TestPhase {
        name,
        tier: ValidationTier::FastUnit,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![],
        environment: vec![],
    }
}

fn selected(name: &str) -> ExpectedTests {
    ExpectedTests {
        passed: BTreeSet::from([name.into()]),
        ignored: BTreeSet::from(["external_ignored".into()]),
        filtered: 4,
    }
}

fn harness() -> Value {
    json!({"role":"terlan-library-harness", "identity_sha256":"a".repeat(64), "path":"/fixture/harness"})
}

fn result(name: &'static str, expected: &ExpectedTests) -> PhaseResult {
    PhaseResult {
        name,
        tier: ValidationTier::FastUnit,
        executor: "direct-terlan-harness",
        wall_time_ms: 1,
        outcome: "passed",
        child_pid: Some(12),
        test_execution: Some(json!({"scope":"admitted-libtest-records-v1",
            "passed":expected.passed.len(), "ignored":expected.ignored.len(), "filtered":expected.filtered,
            "selection_identity_sha256":expected.identity()})),
    }
}

#[test]
fn exact_selections_are_run_bound_and_require_matching_terminal_evidence() {
    let fixture = temporary_fixture("test-selection-complete");
    let control = ProcessControl::new(Duration::from_secs(5));
    let phases = [phase("owned")];
    // A phase supplied for external coverage is not silently certified by this suite.
    let expected = planned(BTreeMap::from([
        ("owned", selected("owned_test")),
        ("external", selected("external_test")),
    ]));
    let path = fixture.0.join("names.json");
    let mut inventory =
        TestSelections::create(&path, "suite-1", &phases, &expected, &harness(), control).unwrap();
    let document: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["suite_run_id"], "suite-1");
    assert_eq!(document["phases"].as_array().unwrap().len(), 1);
    assert_eq!(document["phases"][0]["passed"], json!(["owned_test"]));
    assert_eq!(inventory.json()["verified"], false);
    let completion = result("owned", &expected.selections["owned"]);
    inventory
        .verify(std::slice::from_ref(&completion), control)
        .unwrap();
    assert_eq!(inventory.json()["verified"], true);
    assert_eq!(inventory.json()["reusable"], false);
    assert!(inventory.verify(&[completion], control).is_err());
}

#[test]
fn selection_closeout_rejects_missing_failed_forged_duplicate_and_changed_records() {
    let fixture = temporary_fixture("test-selection-failure");
    let control = ProcessControl::new(Duration::from_secs(5));
    let phases = [phase("owned")];
    let expected = planned(BTreeMap::from([("owned", selected("owned_test"))]));
    let valid = result("owned", &expected.selections["owned"]);
    for mode in [
        "missing",
        "failed",
        "digest",
        "count",
        "scope",
        "pid",
        "duplicate",
        "file",
    ] {
        let path = fixture.0.join(format!("{mode}.json"));
        let mut inventory =
            TestSelections::create(&path, "suite", &phases, &expected, &harness(), control)
                .unwrap();
        let mut completions = vec![valid.clone()];
        match mode {
            "missing" => completions.clear(),
            "failed" => completions[0].outcome = "failed",
            "digest" => {
                completions[0].test_execution.as_mut().unwrap()["selection_identity_sha256"] =
                    json!("forged")
            }
            "count" => completions[0].test_execution.as_mut().unwrap()["passed"] = json!(2),
            "scope" => {
                completions[0].test_execution.as_mut().unwrap()["scope"] =
                    json!("cargo-summary-presence-v1")
            }
            "pid" => completions[0].child_pid = None,
            "duplicate" => completions.push(valid.clone()),
            "file" => std::fs::write(path, "changed").unwrap(),
            _ => unreachable!(),
        }
        assert!(inventory.verify(&completions, control).is_err(), "{mode}");
        assert_eq!(inventory.json()["verified"], false);
    }
}

#[test]
fn selection_admission_rejects_duplicate_ownership_missing_inputs_and_invalid_names() {
    let fixture = temporary_fixture("test-selection-admission");
    let control = ProcessControl::new(Duration::from_secs(5));
    let path = fixture.0.join("names.json");
    let expected = planned(BTreeMap::from([
        ("one", selected("same_test")),
        ("two", selected("same_test")),
    ]));
    assert!(TestSelections::create(
        &path,
        "suite",
        &[phase("one"), phase("two")],
        &expected,
        &harness(),
        control
    )
    .is_err());
    assert!(TestSelections::create(
        &path,
        "suite",
        &[phase("missing")],
        &expected,
        &harness(),
        control
    )
    .is_err());
    assert!(TestSelections::create(&path, "suite", &[], &expected, &harness(), control).is_err());
    assert!(
        TestSelections::create(&path, "", &[phase("one")], &expected, &harness(), control).is_err()
    );
    assert!(TestSelections::create(
        &path,
        "suite",
        &[phase("one")],
        &expected,
        &Value::Null,
        control
    )
    .is_err());
    for name in ["", "with space"] {
        let expected = planned(BTreeMap::from([("one", selected(name))]));
        assert!(TestSelections::create(
            &path,
            "suite",
            &[phase("one")],
            &expected,
            &harness(),
            control
        )
        .is_err());
    }
}

#[test]
fn broad_coverage_cannot_hide_externally_owned_or_unexecuted_ignored_tests() {
    let fixture = temporary_fixture("coverage-external-owners");
    let control = ProcessControl::new(Duration::from_secs(5));
    let expected = |name: &str| ExpectedTests {
        passed: BTreeSet::from([name.into()]),
        ignored: BTreeSet::new(),
        filtered: 3,
    };
    let plan = crate::test_inventory::TestPlan {
        all: [
            "module::owned",
            "module::delegated",
            "ignored::owned",
            "ignored::external",
        ]
        .map(String::from)
        .into(),
        ignored: ["ignored::owned", "ignored::external"]
            .map(String::from)
            .into(),
        selections: BTreeMap::from([
            ("owned-normal", expected("module::owned")),
            ("owned-ignored", expected("ignored::owned")),
            ("external-normal", expected("module::delegated")),
        ]),
    };
    let normal = phase("owned-normal");
    let mut ignored = phase("owned-ignored");
    ignored.args = vec!["--ignored", "--exact", "ignored::owned"];
    let path = fixture.0.join("selections.json");
    let mut inventory = TestSelections::create(
        &path,
        "suite",
        &[normal, ignored],
        &plan,
        &harness(),
        control,
    )
    .unwrap();
    assert!(inventory.inspect_coverage(&["module::owned"]).is_err());
    inventory
        .verify(
            &[
                result("owned-normal", &plan.selections["owned-normal"]),
                result("owned-ignored", &plan.selections["owned-ignored"]),
            ],
            control,
        )
        .unwrap();
    for selectors in [&["module::"][..], &["ignored::", "--ignored"]] {
        let coverage = inventory.inspect_coverage(selectors).unwrap();
        assert_eq!(coverage["selected"], 2);
        assert_eq!(coverage["passed"], 1);
        assert_eq!(coverage["uncovered"], 1);
        assert_eq!(coverage["covered"], false);
    }
    for selectors in [
        &["module::owned", "--exact"][..],
        &["module::", "--skip", "delegated"],
        &["ignored::owned", "--ignored", "--exact"],
    ] {
        let coverage = inventory.inspect_coverage(selectors).unwrap();
        assert_eq!(coverage["covered"], true);
        assert_eq!(coverage["selected"], 1);
        assert_eq!(coverage["reusable"], false);
    }
    assert_eq!(
        inventory.inspect_coverage(&["absent"]).unwrap()["covered"],
        false
    );
    let environments = BTreeMap::from([
        ("owned-normal".to_owned(), "base".to_owned()),
        ("owned-ignored".to_owned(), "worker-path".to_owned()),
    ]);
    assert!(inventory
        .inspect_request(&["module::owned"], "base", &environments)
        .is_ok());
    assert!(inventory
        .inspect_request(
            &["ignored::owned", "--ignored"],
            "worker-path",
            &environments
        )
        .is_ok());
    for selectors in [&["module::owned"][..], &["ignored::owned", "--ignored"]] {
        assert!(inventory
            .inspect_request(selectors, "changed", &environments)
            .is_err());
        assert!(inventory
            .inspect_request(selectors, "base", &BTreeMap::new())
            .is_err());
    }
    assert!(inventory.inspect_coverage(&["--unknown-option"]).is_err());
    let document: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(document["schema"], "terlan.rust-test-selections.v2");
    assert_eq!(document["compiled"]["all"].as_array().unwrap().len(), 4);
    assert_eq!(document["phases"].as_array().unwrap().len(), 2);
}

#[test]
fn compiled_inventory_rejects_wrong_ignore_classification_and_filtered_counts() {
    let fixture = temporary_fixture("coverage-invalid-inventory");
    let control = ProcessControl::new(Duration::from_secs(5));
    let make_plan = || planned(BTreeMap::from([("owned", selected("owned_test"))]));
    for mode in ["missing", "ignore", "filtered", "unknown", "overflow"] {
        let mut plan = make_plan();
        match mode {
            "missing" => {
                plan.all.remove("owned_test");
            }
            "ignore" => {
                plan.ignored.insert("owned_test".into());
            }
            "filtered" => plan.selections.get_mut("owned").unwrap().filtered += 1,
            "unknown" => {
                plan.ignored.insert("not_compiled".into());
            }
            "overflow" => plan.selections.get_mut("owned").unwrap().filtered = usize::MAX,
            _ => unreachable!(),
        }
        assert!(
            TestSelections::create(
                &fixture.0.join(mode),
                "suite",
                &[phase("owned")],
                &plan,
                &harness(),
                control
            )
            .is_err(),
            "{mode}"
        );
    }
}
