use super::*;
use serde_json::json;

#[test]
fn current_input_comparison_requires_every_saved_binding_to_match() {
    let mut current = crate::validation_inputs::ValidationInputs::default().snapshot();
    for value in current.as_object_mut().unwrap().values_mut() {
        *value = json!({"before":"current-bytes", "after":"current-bytes", "verified":true});
    }
    let mut previous = current.clone();
    previous["decision"] = json!("pass");
    previous["wall_time_ms"] = json!(100);
    assert!(compare(&current, &previous).is_ok());
    for key in current.as_object().unwrap().keys() {
        let mut changed = previous.clone();
        changed[key]["before"] = json!("old-bytes");
        assert!(compare(&current, &changed)
            .unwrap_err()
            .detail
            .contains(key));
        changed.as_object_mut().unwrap().remove(key);
        assert!(compare(&current, &changed).is_err());
    }
    assert!(compare(&json!({}), &previous).is_err());
    assert!(!crate::validation_inputs::ValidationInputs::default().fully_verified());
}

#[test]
fn input_verification_cannot_seal_an_empty_or_unbound_observation() {
    let fixture = crate::test_orchestrator_test::temporary_fixture("empty-input-verification");
    let path = fixture.0.join("verification.json");
    let mut ledger = LaunchLedger::new(&path, 1, std::time::Duration::from_secs(5)).unwrap();
    assert!(ledger
        .finish_input_verification(&json!({}), &std::sync::atomic::AtomicBool::new(false))
        .is_err());
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(report["decision"], "fail");
    assert_eq!(report["direct_cargo_launch_count"], 0);
}

#[test]
fn admission_matches_pending_observations_without_ignoring_input_changes() {
    let mut current = crate::validation_inputs::ValidationInputs::default().snapshot();
    for value in current.as_object_mut().unwrap().values_mut() {
        *value = json!({"before":{"bytes":"same","policy":"strict"},"after":null,"verified":false});
    }
    let mut previous = current.clone();
    for value in previous.as_object_mut().unwrap().values_mut() {
        value["after"] = value["before"].clone();
        value["verified"] = json!(true);
    }
    current["cargo_tool_binding"]["paths_verified"] = json!(false);
    previous["cargo_tool_binding"]["paths_verified"] = json!(true);
    current["selected_compiler_binding"]["paths_verified"] = json!(false);
    previous["selected_compiler_binding"]["paths_verified"] = json!(true);
    assert!(compare_admission(&current, &previous).is_ok());
    assert!(compare(&current, &previous).is_err());
    for key in current.as_object().unwrap().keys() {
        for field in ["bytes", "policy"] {
            let mut changed = previous.clone();
            changed[key]["before"][field] = json!("changed");
            assert!(compare_admission(&current, &changed).is_err());
        }
        let mut missing = previous.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(compare_admission(&current, &missing).is_err());
    }
    assert!(compare_admission(&json!({}), &previous).is_err());
}
