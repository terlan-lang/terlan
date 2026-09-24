use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

use super::list_comprehension::{
    completed_effect_list_type, lower_completed_guard_results, lower_guards,
};

#[test]
fn completed_guard_lowering_preserves_unrelated_function_identities() {
    for function in [
        "from_bool",
        "app.Decision.from_bool",
        "app.GuardResult.accept",
    ] {
        let guard = CoreExpr::Call {
            function: function.to_string(),
            type_args: Vec::new(),
            args: vec![CoreExpr::Atom("true".to_string())],
        };
        let mut guards = vec![guard.clone()];
        lower_completed_guard_results(&mut guards);
        assert_eq!(guards, vec![guard]);
    }
}

#[test]
fn comprehension_executes_user_predicates_named_like_standard_helpers() {
    super::source_constructor_test::check_sources(&[r#"
module comprehension_predicate_identity.
from_bool(value: Bool): Bool -> not value.
pub check(): Bool ->
    let values = [1, 2];
    let filtered = [value | value <- values, from_bool(value > 1)];
    filtered == [1].
"#]);
}

#[test]
fn comprehension_executes_checked_range_membership_filters() {
    super::source_constructor_test::check_sources(&[r#"
module comprehension_checked_range.
pub check(): Bool ->
    let values = [0, 1, 2, 3, 4];
    let ascending = [value | value <- values, value in 1 .. 3];
    let descending = [value | value <- values, value in 3 .. 1];
    ascending == [1, 2, 3] and descending == [1, 2, 3].
"#]);
}

#[test]
fn effect_guards_retain_execution_boundaries_and_preserve_pure_filters() {
    let pure_filter = CoreExpr::BinaryOp {
        operator: ">".to_string(),
        left: Box::new(CoreExpr::Var("value".to_string())),
        right: Box::new(CoreExpr::Int(0)),
    };
    let mut guards = vec![
        CoreExpr::Cast {
            expr: Box::new(CoreExpr::Call {
                type_args: Vec::new(),
                function: "std.core.Effect.succeed".to_string(),
                args: vec![CoreExpr::Atom("true".to_string())],
            }),
            target_type: CoreType::Apply {
                constructor: "std.core.Effect.Effect".to_string(),
                args: vec![CoreType::Bool],
            },
        },
        pure_filter.clone(),
    ];

    lower_guards(&mut guards);

    assert!(matches!(&guards[0], CoreExpr::Intrinsic(call) if call.return_type == CoreType::Bool));
    assert_eq!(guards[1], pure_filter);
}

#[test]
fn untyped_calls_are_not_mistaken_for_effect_guards() {
    let untyped = CoreExpr::Call {
        type_args: Vec::new(),
        function: "std.core.Effect.flat_map".to_string(),
        args: Vec::new(),
    };
    let mut guards = vec![untyped.clone()];
    lower_guards(&mut guards);
    assert_eq!(guards, vec![untyped]);
}

#[test]
fn failed_and_cancelled_guards_are_deferred_to_vm_execution() {
    for function in ["std.core.Effect.fail", "std.core.Effect.cancelled"] {
        let mut guards = vec![CoreExpr::Cast {
            expr: Box::new(CoreExpr::Call {
                type_args: Vec::new(),
                function: function.to_string(),
                args: Vec::new(),
            }),
            target_type: CoreType::Apply {
                constructor: "std.core.Effect.Effect".to_string(),
                args: vec![CoreType::Bool],
            },
        }];
        lower_guards(&mut guards);
        assert!(
            matches!(&guards[0], CoreExpr::Intrinsic(call) if call.id == crate::terlan_typeck::CoreIntrinsicId::Primitive(crate::terlan_typeck::CorePrimitiveIntrinsic::VmEffectRun))
        );
    }
}

#[test]
fn completed_effect_list_type_finds_transparent_union_payload() {
    let expected = CoreType::List(Box::new(CoreType::Int));
    let expanded_effect = CoreType::Union(vec![
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("pure".to_string())),
            CoreTupleTypeElem::Field {
                name: "value".to_string(),
                ty: expected.clone(),
            },
        ]),
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("failed".to_string())),
            CoreTupleTypeElem::Field {
                name: "error".to_string(),
                ty: CoreType::Dynamic,
            },
        ]),
    ]);

    assert_eq!(
        completed_effect_list_type(&expanded_effect).expect("completed list payload"),
        expected
    );
}

#[test]
fn completed_guard_results_lower_to_native_boolean_decisions() {
    let decision = CoreExpr::BinaryOp {
        operator: ">".to_string(),
        left: Box::new(CoreExpr::Var("value".to_string())),
        right: Box::new(CoreExpr::Int(0)),
    };
    let mut guards = vec![
        CoreExpr::Call {
            type_args: Vec::new(),
            function: "std.core.GuardResult.from_bool".to_string(),
            args: vec![decision.clone()],
        },
        CoreExpr::Call {
            type_args: Vec::new(),
            function: "std.core.GuardResult.reject".to_string(),
            args: Vec::new(),
        },
    ];

    lower_completed_guard_results(&mut guards);

    assert_eq!(guards, vec![decision, CoreExpr::Atom("false".to_string())]);
}

#[test]
fn completed_guard_result_combinators_preserve_boolean_structure() {
    let mut guards = vec![CoreExpr::Call {
        type_args: Vec::new(),
        function: "std.core.GuardResult.both".to_string(),
        args: vec![
            CoreExpr::Call {
                type_args: Vec::new(),
                function: "std.core.GuardResult.accept".to_string(),
                args: Vec::new(),
            },
            CoreExpr::Call {
                type_args: Vec::new(),
                function: "std.core.GuardResult.reject".to_string(),
                args: Vec::new(),
            },
        ],
    }];

    lower_completed_guard_results(&mut guards);

    assert!(matches!(
        &guards[0],
        CoreExpr::BinaryOp { operator, left, right }
            if operator == "and"
                && matches!(left.as_ref(), CoreExpr::Atom(value) if value == "true")
                && matches!(right.as_ref(), CoreExpr::Atom(value) if value == "false")
    ));
}
