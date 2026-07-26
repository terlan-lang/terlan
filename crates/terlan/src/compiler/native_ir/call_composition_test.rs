use std::collections::HashSet;

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::{
    composed_call_region, rebase_callee_locals, ComposedCallProfile, NativeContinuation,
    NativeExpr, NativeTransitionOperation, NativeType,
};

fn suspend(continuation_id: u64) -> NativeExpr {
    NativeExpr::Suspend {
        operation: NativeTransitionOperation::Yield,
        arguments: Vec::new(),
        continuation_id,
        values: Vec::new(),
    }
}

#[test]
fn profile_follows_continuation_identities_after_storage_reordering() {
    let continuations = vec![
        NativeContinuation {
            id: 30,
            source_module: "app.Test".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            params: vec![NativeType::Bool],
            return_type: NativeType::Bool,
            body: NativeExpr::Bool(true),
        },
        NativeContinuation {
            id: 10,
            source_module: "app.Test".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            params: vec![NativeType::Int],
            return_type: NativeType::Bool,
            body: suspend(20),
        },
        NativeContinuation {
            id: 20,
            source_module: "app.Test".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            params: vec![NativeType::Float],
            return_type: NativeType::Bool,
            body: suspend(30),
        },
    ];

    let profile = ComposedCallProfile::new(&suspend(10), &continuations).expect("linear ID chain");

    assert_eq!(
        profile
            .continuations
            .iter()
            .map(|continuation| continuation.id)
            .collect::<Vec<_>>(),
        [10, 20, 30]
    );
}

#[test]
fn profile_admits_a_callee_that_can_complete_or_suspend() {
    let body = NativeExpr::If {
        clauses: vec![
            (NativeExpr::Param(0), suspend(10)),
            (NativeExpr::Bool(true), NativeExpr::Int(7)),
        ],
    };
    let continuations = vec![NativeContinuation {
        id: 10,
        source_module: "app.Test".to_string(),
        source_function: "maybe_yield".to_string(),
        source_arity: 1,
        params: Vec::new(),
        return_type: NativeType::Int,
        body: NativeExpr::Int(41),
    }];

    let profile = ComposedCallProfile::new(&body, &continuations)
        .expect("optional suspension has a closed continuation profile");

    assert_eq!(profile.continuations.len(), 1);
    assert_eq!(profile.continuations[0].id, 10);
}

#[test]
fn rebasing_reaches_managed_operation_arguments() {
    let body = NativeExpr::ManagedOperation {
        encoded: b"test".as_slice().into(),
        args: vec![NativeExpr::Param(0), NativeExpr::Param(2)],
    };

    assert_eq!(
        rebase_callee_locals(&body, 1, 3),
        NativeExpr::ManagedOperation {
            encoded: b"test".as_slice().into(),
            args: vec![NativeExpr::Param(0), NativeExpr::Param(5)],
        }
    );
}

#[test]
fn gated_call_factors_the_surrounding_suffix_into_one_join() {
    let call = CoreExpr::Call {
        function: "package.native_check".to_string(),
        args: Vec::new(),
    };
    let expr = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("valid".to_string()),
            value: CoreExpr::BinaryOp {
                operator: "and".to_string(),
                left: Box::new(CoreExpr::Atom("true".to_string())),
                right: Box::new(call),
            },
        }],
        body: Box::new(CoreExpr::Var("valid".to_string())),
    };
    let suspending = HashSet::from([("package.native_check".to_string(), 0)]);

    let region = composed_call_region(&expr, &suspending, &|_, _| true, &HashSet::new())
        .expect("suspending branch must compose");

    assert_eq!(region.gates.len(), 1);
    assert_eq!(
        region.gates[0].bypass_resume,
        CoreExpr::Atom("false".to_string())
    );
    assert!(matches!(
        region.join.expect("outer suffix must be shared").resume,
        CoreExpr::Let { .. }
    ));
}
