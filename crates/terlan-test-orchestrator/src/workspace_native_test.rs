use super::*;

#[test]
fn only_the_exact_main_library_can_delegate_to_direct_phases() {
    let main = json!({"scope":"declared-cargo-libtest-target-v1", "package":"terlan", "kind":"lib", "executable":"/main", "manifest_identity_sha256":"same"});
    assert!(main_owner(&main, &main).unwrap());
    for (key, value) in [
        ("executable", json!("/rebuilt")),
        ("manifest_identity_sha256", json!("changed")),
    ] {
        let mut changed = main.clone();
        changed[key] = value;
        assert!(main_owner(&changed, &main).is_err());
    }
    let other = json!({"scope":"declared-cargo-libtest-target-v1", "package":"terlan", "kind":"test", "executable":"/integration"});
    assert!(!main_owner(&other, &main).unwrap());
    assert!(main_owner(&other, &Value::Null).is_err());
    let mut alias = other;
    alias["executable"] = main["executable"].clone();
    assert!(main_owner(&alias, &main).is_err());
}

#[test]
fn delegation_is_not_passing_test_evidence_and_cannot_launch_the_main_harness_again() {
    let launches = vec![json!({"role":"started", "pid":1})];
    let valid = json!({"schema":"terlan.workspace-native-delegation.v1", "decision":"delegated", "owner":"main-library-phases", "certificate_identity":"certificate", "context_identity":"context", "launches":launches});
    validate_delegated_completion(&valid, "certificate", "context", &launches).unwrap();
    assert!(validate_completion(&valid, "certificate", "context", &launches).is_err());
    for (key, value) in [
        ("decision", json!("pass")),
        ("owner", json!("unknown")),
        ("certificate_identity", json!("changed")),
        ("context_identity", json!("changed")),
        ("test_execution", json!({"passed":1})),
    ] {
        let mut changed = valid.clone();
        changed[key] = value;
        assert!(
            validate_delegated_completion(&changed, "certificate", "context", &launches).is_err()
        );
    }
    let replay = vec![launches[0].clone(), json!({"role":"run", "pid":2})];
    let mut changed = valid;
    changed["launches"] = json!(replay);
    assert!(validate_delegated_completion(&changed, "certificate", "context", &replay).is_err());
}

#[test]
fn completion_requires_exact_input_scope_and_launch_records() {
    let launches = vec![json!({"role":"started", "pid":1})];
    let names = json!(["first", "second", "third"]);
    let identity = crate::test_execution::ExpectedTests::native_names(&names)
        .unwrap()
        .identity();
    let valid = json!({"schema":"terlan.workspace-native-completion.v1", "decision":"pass", "certificate_identity":"certificate", "context_identity":"context", "launches":launches,
        "passed_names":names, "test_execution":{"scope":"admitted-libtest-records-v1", "passed":3, "ignored":0, "filtered":0,"selection_identity_sha256":identity}});
    assert_eq!(
        validate_completion(&valid, "certificate", "context", &launches).unwrap(),
        3
    );
    for field in [
        "schema",
        "decision",
        "certificate_identity",
        "context_identity",
        "launches",
        "test_execution",
        "passed_names",
    ] {
        let mut invalid = valid.clone();
        invalid[field] = Value::Null;
        assert!(validate_completion(&invalid, "certificate", "context", &launches).is_err());
    }
    for (field, value) in [
        ("scope", json!("cargo-summary-presence-v1")),
        ("passed", json!(-1)),
        ("ignored", json!(1)),
        ("filtered", json!(1)),
        ("selection_identity_sha256", json!("changed")),
    ] {
        let mut invalid = valid.clone();
        invalid["test_execution"][field] = value;
        assert!(validate_completion(&invalid, "certificate", "context", &launches).is_err());
    }
    assert!(validate_completion(&valid, "other-certificate", "context", &launches).is_err());
    assert!(validate_completion(&valid, "certificate", "other-context", &launches).is_err());
    assert!(validate_completion(&valid, "certificate", "context", &[]).is_err());
}
