use object::{Object, ObjectSymbol};

use super::{
    continuation_sharing::{
        externally_resumable_continuation_ids, intern_equivalent_continuations,
        materialize_shared_continuations,
    },
    emit_native_application_object, NativeContinuation, NativeExpr, NativeFunction, NativeModule,
    NativeTransitionOperation, NativeType,
};
use crate::compiler::native_ir::NativeCallResume;

fn continuation(id: u64, body: NativeExpr) -> NativeContinuation {
    NativeContinuation {
        id,
        source_module: "app.Intern".to_string(),
        source_function: "main".to_string(),
        source_arity: 0,
        source_span: None,
        capture_names: Vec::new(),
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

/// Equivalent bodies remain distinct when one is a synchronous completion
/// target and the other is an outward suspension entry.
#[test]
fn protocol_roles_prevent_completion_and_outward_continuation_aliasing() {
    let mut modules = vec![NativeModule {
        name: "app.ProtocolRoles".to_string(),
        functions: vec![NativeFunction {
            export_id: 1,
            name: "main".to_string(),
            public: true,
            arity: 0,
            source_module: "app.ProtocolRoles".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: NativeExpr::CallThen {
                function: 0,
                args: Vec::new(),
                resumes: vec![NativeCallResume {
                    callee_continuation_id: 99,
                    callee_capture_count: 0,
                    continuation_id: 2,
                    caller_value_start: 0,
                }],
                completion_continuation_id: 1,
                completion_function: None,
                values: Vec::new(),
            },
        }],
        continuations: vec![
            continuation(1, NativeExpr::Int(7)),
            continuation(2, NativeExpr::Int(7)),
        ],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    intern_equivalent_continuations(&mut modules);

    let mut ids = modules[0]
        .continuations
        .iter()
        .map(|continuation| continuation.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2]);
    let NativeExpr::CallThen {
        resumes,
        completion_continuation_id,
        ..
    } = &modules[0].functions[0].body
    else {
        panic!("protocol fixture must retain CallThen");
    };
    assert_eq!(*completion_continuation_id, 1);
    assert_eq!(resumes[0].continuation_id, 2);
}

#[test]
fn materialization_keeps_internal_graph_but_marks_it_non_resumable() {
    let mut modules = vec![NativeModule {
        name: "app.Materialize".to_string(),
        functions: vec![NativeFunction {
            export_id: 1,
            name: "main".to_string(),
            public: true,
            arity: 0,
            source_module: "app.Materialize".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: NativeExpr::ContinuationTailCall {
                continuation_id: 7,
                args: Vec::new(),
            },
        }],
        continuations: vec![continuation(7, NativeExpr::Int(42))],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    materialize_shared_continuations(&mut modules).expect("materialize continuation graph");

    assert_eq!(modules.len(), 2);
    assert_eq!(modules[1].name, "$terlan.continuations");
    assert_eq!(modules[1].functions.len(), 1);
    assert_eq!(modules[0].continuations.len(), 1);
    assert!(externally_resumable_continuation_ids(&modules).is_empty());
    assert_eq!(
        modules[0].functions[0].body,
        NativeExpr::TailCall {
            function: 1,
            args: Vec::new(),
            yield_continuation_id: None,
        }
    );
}

#[test]
fn materialization_classifies_only_vm_resumable_continuation_adapters() {
    let mut modules = vec![NativeModule {
        name: "app.Resume".to_string(),
        functions: vec![NativeFunction {
            export_id: 1,
            name: "main".to_string(),
            public: true,
            arity: 0,
            source_module: "app.Resume".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: NativeExpr::Suspend {
                operation: NativeTransitionOperation::Yield,
                arguments: Vec::new(),
                continuation_id: 7,
                values: Vec::new(),
            },
        }],
        continuations: vec![
            continuation(
                7,
                NativeExpr::ContinuationTailCall {
                    continuation_id: 8,
                    args: Vec::new(),
                },
            ),
            continuation(8, NativeExpr::Int(42)),
        ],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    materialize_shared_continuations(&mut modules).expect("materialize continuation graph");

    assert_eq!(modules[0].continuations.len(), 2);
    assert_eq!(
        externally_resumable_continuation_ids(&modules),
        [7].into_iter().collect(),
    );
    assert!(matches!(
        modules[0].continuations[0].body,
        NativeExpr::TailCall {
            yield_continuation_id: None,
            ..
        }
    ));
    assert_eq!(modules[1].functions.len(), 2);
    let object = emit_native_application_object("resumable-continuation-filter", &modules)
        .expect("emit filtered continuation image");
    let parsed = object::File::parse(object.as_slice()).expect("parse continuation image");
    let records = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some("terlan_native_dispatch_records_v2"))
        .expect("dense dispatch records");
    assert_eq!(
        records.size(),
        2 * 24,
        "one public function and one VM-resumable continuation must be dispatchable"
    );
}
