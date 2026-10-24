use super::*;

fn fixture(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("app/Values.terl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    (root, path)
}

#[test]
fn source_exports_preserve_order_types_and_projection_failures() {
    let (root, path) = fixture("module app.Values.\npub number(): Int -> 40 + 2.\npub text(): String -> \"computed\" + \" value\".");
    with_source_exports(
        &path,
        &root.path().join("build"),
        &["text", "number", "text"],
        |module, values, exports| {
            assert_eq!(module, "app.Values");
            assert!(matches!(&values[0], ReplValue::String(value) if value == "computed value"));
            assert!(matches!(&values[1], ReplValue::Int(42)));
            assert_eq!(values[0], values[2]);
            assert!(!exports.is_empty());
            Ok(())
        },
    )
    .unwrap();
    let error = with_source_exports(&path, &root.path().join("build"), &["number"], |_, _, _| {
        Err::<(), _>("projection rejected".into())
    })
    .unwrap_err();
    assert_eq!(error, "projection rejected");
}

#[test]
fn source_exports_validate_all_entrypoints_before_execution() {
    let (root, path) = fixture(
        r#"module app.Values.
import std.vm.Process.
pub wait(): Unit -> Process.sleep(Process.timer(1)).
"#,
    );
    let error = with_source_exports(
        &path,
        &root.path().join("build"),
        &["wait", "missing"],
        |_, _, _| Ok(()),
    )
    .unwrap_err();
    assert!(
        error.contains("missing public `app.Values.missing/0`"),
        "{error}"
    );
    let error = with_source_exports(&path, &root.path().join("build"), &["wait"], |_, _, _| {
        Ok(())
    })
    .unwrap_err();
    assert!(
        error.contains("source_eval.external_wait") && error.contains("Timer"),
        "{error}"
    );
}

#[test]
fn source_exports_resume_yields_and_cancel_when_transition_budget_is_exhausted() {
    let (root, path) = fixture(
        r#"module app.Values.
import std.vm.Process.
repeat(count: Int): String ->
    if {
        count == 0 -> "done";
        true -> (Process.yield_now(); repeat(count - 1))
    }.
pub finite(): String -> repeat(2).
pub excessive(): String -> repeat(1025).
"#,
    );
    with_source_exports(
        &path,
        &root.path().join("build"),
        &["finite"],
        |_, values, _| {
            assert!(matches!(&values[0], ReplValue::String(value) if value == "done"));
            Ok(())
        },
    )
    .unwrap();
    let error = with_source_exports(
        &path,
        &root.path().join("build"),
        &["excessive"],
        |_, _, _| Ok(()),
    )
    .unwrap_err();
    assert!(
        error.contains("source_eval.budget") && error.contains("app.Values.excessive/0"),
        "{error}"
    );
    with_source_exports(&path, &root.path().join("build"), &["finite"], |_, _, _| {
        Ok(())
    })
    .unwrap();
}
