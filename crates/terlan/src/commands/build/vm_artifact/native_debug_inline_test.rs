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

#[test]
fn native_debug_reuse_identity_covers_source_paths_and_callable_spans() {
    let make = || super::NativeDebugInput {
        source_path: "app/Main.terl".into(),
        source_text: "module app.Main. pub main(): Int -> 7.".into(),
        module: "app.Main".into(),
        core_schema: "core-v1".into(),
        proof_readiness: "ready".into(),
        function_spans: [(("main".into(), 0), (17, 38))].into(),
    };
    let original = make().cache_identity().unwrap();
    assert_eq!(original, make().cache_identity().unwrap());
    let mutations: [fn(&mut super::NativeDebugInput); 7] = [
        |input| input.source_path.push_str(".other"),
        |input| input.source_text.push(' '),
        |input| input.module.push_str("Other"),
        |input| input.core_schema.push('2'),
        |input| input.proof_readiness.push_str("-changed"),
        |input| {
            input.function_spans.insert(("main".into(), 0), (18, 38));
        },
        |input| {
            input.function_spans.insert(("main".into(), 1), (17, 38));
        },
    ];
    for mutate in mutations {
        let mut changed = make();
        mutate(&mut changed);
        assert_ne!(original, changed.cache_identity().unwrap());
    }
}
