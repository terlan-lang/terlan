use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use serde_json::json;
use std::time::Duration;

#[test]
fn private_records_are_single_writer_and_clean_all_registered_leaves() {
    let fixture = temporary_fixture("workspace-storage");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    let mut registry = Registry::create(&environment).unwrap();
    let key = "a".repeat(64);
    registry.register_target(&key).unwrap();
    let path = registry.path().join(format!("{key}.complete.json"));
    let value = json!({"decision":"pass"});
    write_new(&path, &value).unwrap();
    assert!(write_new(&path, &json!({"decision":"other"})).is_err());
    assert_eq!(
        read_json(&path, ProcessControl::new(Duration::from_secs(5)))
            .unwrap()
            .0,
        value
    );
    let directory = registry.path().to_path_buf();
    registry.close().unwrap();
    assert!(!directory.exists());
}

#[test]
fn unknown_siblings_survive_registry_cleanup_failure() {
    let fixture = temporary_fixture("workspace-unknown-sibling");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    let registry = Registry::create(&environment).unwrap();
    let unknown = registry.path().join("not-owned");
    fs::write(&unknown, "preserve").unwrap();
    assert!(registry.close().is_err());
    assert_eq!(fs::read_to_string(unknown).unwrap(), "preserve");
}

#[test]
fn record_and_namespace_limits_reject_without_creating_unowned_files() {
    let fixture = temporary_fixture("workspace-storage-limits");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    assert!(crate::test_result_log::reserve_directory(&environment, "../escape").is_err());
    let mut registry = Registry::create(&environment).unwrap();
    for key in ["../escape", "wrong", &"z".repeat(64)] {
        assert!(registry.register_target(key).is_err());
    }
    for name in ["", ".", "..", "../observer", "observer/name", "/observer"] {
        assert!(registry.register_observer(name).is_err());
    }
    let path = registry.path().join("context.json");
    assert!(write_new(&path, &json!("x".repeat(MAX_RECORD_BYTES as usize))).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read_dir(registry.path()).unwrap().count(), 0);
    registry.close().unwrap();
}

#[test]
fn failure_snapshots_retain_only_valid_available_launch_records_before_cleanup() {
    let fixture = temporary_fixture("workspace-failure-records");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    let mut registry = Registry::create(&environment).unwrap();
    let key = "b".repeat(64);
    registry.register_target(&key).unwrap();
    write_new(
        &registry.path().join(format!("{key}.started.json")),
        &json!({"pid":23}),
    )
    .unwrap();
    write_new(
        &registry.path().join(format!("{key}.all.json")),
        &json!({"pid":0}),
    )
    .unwrap();
    let evidence = registry.failure_evidence();
    assert_eq!(evidence["decision"], "fail");
    assert_eq!(evidence["complete"], false);
    assert_eq!(evidence["nested_process_launch_count"], 1);
    assert_eq!(evidence["launches"][0]["pid"], 23);
    assert_eq!(evidence["observation_errors"].as_array().unwrap().len(), 1);
    registry.close().unwrap();
}
