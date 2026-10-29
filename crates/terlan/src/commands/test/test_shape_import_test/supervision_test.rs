use super::*;

/// Compiles and executes the public Terlan lifecycle and durable replay API.
#[test]
fn project_vm_tests_execute_source_supervision_and_durable_recovery() {
    let exit = run_project_vm_test(
        "terlan_source_supervision",
        "supervision-app",
        "src/app/Main.terl",
        "module app.Main.\npub ready(): Bool -> true.\n",
        "tests/language/VmSupervisorRuntimeTest.terl",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language/VmSupervisorRuntimeTest.terl"
        )),
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}
