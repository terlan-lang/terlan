#[test]
fn constructor_debug_spans_use_source_callable_identity() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("app/Cell.terl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = "module app.Cell. pub struct Cell { value: Int }. pub constructor Cell { (value: Int): Cell -> Cell { value: value } }.";
    std::fs::write(&path, source).unwrap();
    let compiled = super::super::compile::compile_vm_module(
        path.to_str().unwrap(),
        &crate::CliState::default(),
    )
    .unwrap();
    let debug = compiled.native_debug;
    assert!(debug.function_spans.contains_key(&("Cell".into(), 1)));
}
