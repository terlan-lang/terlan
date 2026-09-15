use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use crate::workspace_native_storage::Registry;

#[test]
fn undeclared_helper_arguments_fail_before_waiting_for_cargo_or_creating_launch_records() {
    assert!(execute(Vec::new()).is_err());
    let fixture = temporary_fixture("workspace-runner-arguments");
    let environment = ExecutionEnvironment::from_entries([], &fixture.0).unwrap();
    let registry = Registry::create(&environment).unwrap();
    write_new(
        &registry.path().join("context.json"),
        &json!({"schema":"terlan.workspace-native-context.v1", "threads":1, "timeout_seconds":30}),
    )
    .unwrap();
    let error = execute(vec![
        registry.path().as_os_str().to_owned(),
        "nonexistent-executable".into(),
        "--ignored".into(),
    ])
    .unwrap_err();
    assert!(error.detail.contains("undeclared test selectors"));
    assert_eq!(std::fs::read_dir(registry.path()).unwrap().count(), 2);
    registry.close().unwrap();
}
