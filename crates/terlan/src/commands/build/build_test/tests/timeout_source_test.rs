use crate::commands::build::vm_artifact::source_exports::with_source_exports;
use crate::runtime::vm::ReplValue;
use std::fs;
use std::path::Path;

const TIMEOUT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../std/vm/Timeout.terl"
));
const TESTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../std/vm/TimeoutTest.terl"
));

fn fixture(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("std/vm");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("Timeout.terl"), TIMEOUT).unwrap();
    let path = directory.join("TimeoutTest.terl");
    fs::write(&path, source).unwrap();
    (root, path)
}

fn evaluate(path: &Path, functions: &[&str]) -> Result<Vec<ReplValue>, String> {
    with_source_exports(
        path,
        &path.parent().unwrap().join("build"),
        functions,
        |_, values, _| Ok(values.to_vec()),
    )
}

#[test]
fn timeout_source_executes_boundaries_without_runtime_services() {
    let (_root, path) = fixture(TESTS);
    let values = evaluate(
        &path,
        &[
            "finite_timeout_becomes_due_at_the_deadline",
            "zero_timeout_is_immediately_due",
            "unbounded_timeout_never_becomes_due",
            "largest_timeout_does_not_overflow",
            "values_remain_independent",
        ],
    )
    .unwrap();
    assert_eq!(values, vec![ReplValue::Bool(true); 5]);
}

#[test]
fn timeout_deadline_behavior_is_owned_by_editable_terlan_source() {
    let (_root, path) = fixture(TESTS);
    let functions = ["finite_timeout_becomes_due_at_the_deadline"];
    assert_eq!(
        evaluate(&path, &functions).unwrap(),
        [ReplValue::Bool(true)]
    );
    let changed = TIMEOUT.replace("elapsed >= deadline", "elapsed > deadline");
    assert_ne!(changed, TIMEOUT);
    fs::write(path.parent().unwrap().join("Timeout.terl"), changed).unwrap();
    assert_eq!(
        evaluate(&path, &functions).unwrap(),
        [ReplValue::Bool(false)]
    );
}

#[test]
fn timeout_source_rejects_negative_constructor_and_elapsed_values() {
    for expression in [
        "Timeout.wakes_after(Timeout.milliseconds(-1), 0)",
        "Timeout.wakes_after(Timeout.milliseconds(-9223372036854775807), 0)",
        "Timeout.wakes_after(Timeout.milliseconds(0), -1)",
        "Timeout.wakes_after(Timeout.forever(), -1)",
    ] {
        let (_root, path) = fixture(&format!(
            "module std.vm.TimeoutTest.\nimport std.vm.Timeout.\npub probe(): Bool -> {expression}.\n"
        ));
        let error = evaluate(&path, &["probe"]).unwrap_err();
        assert!(error.contains("match"), "{expression}: {error}");
        assert!(
            !error.contains("compilation failed"),
            "{expression}: {error}"
        );
    }
}
