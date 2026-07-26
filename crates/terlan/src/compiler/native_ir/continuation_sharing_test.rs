use super::{
    continuation_sharing::intern_equivalent_continuations, NativeContinuation, NativeExpr,
    NativeFunction, NativeModule, NativeType,
};

fn continuation(id: u64, body: NativeExpr) -> NativeContinuation {
    NativeContinuation {
        id,
        source_module: "app.Intern".to_string(),
        source_function: "main".to_string(),
        source_arity: 0,
        params: Vec::new(),
        return_type: NativeType::Int,
        body,
    }
}

#[test]
fn identical_suffixes_and_their_parents_collapse_transitively() {
    let call = |continuation_id| NativeExpr::ContinuationTailCall {
        continuation_id,
        args: Vec::new(),
    };
    let mut modules = vec![NativeModule {
        name: "app.Intern".to_string(),
        functions: vec![NativeFunction {
            export_id: 1,
            name: "main".to_string(),
            public: true,
            arity: 0,
            source_module: "app.Intern".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: call(1),
        }],
        continuations: vec![
            continuation(1, call(2)),
            continuation(2, NativeExpr::Int(7)),
            continuation(3, NativeExpr::Int(7)),
            continuation(4, call(3)),
        ],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    intern_equivalent_continuations(&mut modules);

    assert_eq!(modules[0].continuations.len(), 2);
    assert_eq!(modules[0].functions[0].body, call(4));
    assert_eq!(
        modules[0]
            .continuations
            .iter()
            .find(|continuation| continuation.id == 4)
            .expect("canonical parent")
            .body,
        call(3)
    );
}
