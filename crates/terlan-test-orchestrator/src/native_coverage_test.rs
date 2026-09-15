use super::*;

fn report() -> Value {
    let names = json!(["integration::first", "integration::second"]);
    let identity = ExpectedTests::native_names(&names).unwrap().identity();
    let binding = json!({"verified":true,"before":[{"role":"workspace-harness","bytes":10}],"after":[{"role":"workspace-harness","bytes":10}]});
    let completion = json!({"schema":"terlan.workspace-native-completion.v1","decision":"pass",
        "certificate_identity":"certificate","context_identity":"context","launches":[{"role":"run","pid":5}],
        "passed_names":names,"test_execution":{"scope":"admitted-libtest-records-v1","passed":2,"ignored":0,"filtered":0,"selection_identity_sha256":identity}});
    json!({"phases":[{"executor":"cargo-native-harnesses","outcome":"passed","test_execution":{
        "scope":"cargo-native-libtest-records-v1","targets":[{"delegated_main":false,
            "target":{"package":"terlan","kind":"test","target":"integration"},"executable_binding":binding,"completion":completion}]}}]})
}

fn selection(selectors: &[&str]) -> Selection {
    Selection {
        package: "terlan".into(),
        kind: "test",
        target: Some("integration".into()),
        selectors: selectors.iter().map(|value| (*value).into()).collect(),
    }
}

#[test]
fn native_names_cover_only_the_matching_target_and_actual_nonempty_selectors() {
    let targets = inventory(&report()).unwrap();
    let target = &targets[0];
    assert!(target.matches(&selection(&[])));
    for selectors in [
        vec![],
        vec!["integration"],
        vec!["integration::first", "--exact"],
        vec!["--skip", "second"],
    ] {
        assert_eq!(
            target.coverage(&selection(&selectors)).unwrap()["covered"],
            true
        );
    }
    for selectors in [
        vec!["absent"],
        vec!["integration", "--exact"],
        vec!["--ignored"],
        vec!["--skip", "integration"],
    ] {
        assert_eq!(
            target.coverage(&selection(&selectors)).unwrap()["covered"],
            false
        );
    }
    let mut other = selection(&[]);
    other.kind = "lib";
    assert!(!target.matches(&other));
    other = selection(&[]);
    other.package = "support".into();
    assert!(!target.matches(&other));
}

#[test]
fn native_coverage_rejects_missing_duplicate_changed_or_forged_name_evidence() {
    for names in [
        Value::Null,
        json!(["integration::first", "integration::first"]),
        json!(["integration::first"]),
        json!(["integration::first", "wrong"]),
        json!(["bad name"]),
    ] {
        let mut changed = report();
        changed["phases"][0]["test_execution"]["targets"][0]["completion"]["passed_names"] = names;
        assert!(inventory(&changed).is_err());
    }
    let mut changed = report();
    changed["phases"][0]["test_execution"]["targets"][0]["executable_binding"]["verified"] =
        json!(false);
    assert!(inventory(&changed).is_err());
    let mut changed = report();
    changed["phases"][0]["outcome"] = json!("failed");
    assert!(inventory(&changed).is_err());
    let mut changed = report();
    let targets = changed["phases"][0]["test_execution"]["targets"]
        .as_array_mut()
        .unwrap();
    targets.push(targets[0].clone());
    assert!(inventory(&changed).is_err());
}
