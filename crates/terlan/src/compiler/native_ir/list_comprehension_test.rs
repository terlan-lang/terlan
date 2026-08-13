use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

use super::list_comprehension::{
    completed_effect_list_type, lower_completed_effect_guards, lower_completed_guard_results,
};

#[test]
fn completed_effect_guards_unwrap_succeed_and_preserve_pure_filters() {
    let pure_filter = CoreExpr::BinaryOp {
        operator: ">".to_string(),
        left: Box::new(CoreExpr::Var("value".to_string())),
        right: Box::new(CoreExpr::Int(0)),
    };
    let mut guards = vec![
        CoreExpr::Call {
            function: "std.core.Effect.succeed".to_string(),
            args: vec![CoreExpr::Atom("true".to_string())],
        },
        pure_filter.clone(),
    ];

    lower_completed_effect_guards(&mut guards).expect("completed effect guards");

    assert_eq!(
        guards,
        vec![CoreExpr::Atom("true".to_string()), pure_filter]
    );
}

#[test]
fn deferred_effect_guard_is_a_loud_native_boundary() {
    let mut guards = vec![CoreExpr::Call {
        function: "std.core.Effect.flat_map".to_string(),
        args: Vec::new(),
    }];

    let error = lower_completed_effect_guards(&mut guards).expect_err("deferred effect guard");

    assert_eq!(
        error.to_string(),
        "error[native_ir.comprehension_effect]: deferred effect guard `std.core.Effect.flat_map` requires scheduler continuation lowering"
    );
}

#[test]
fn failed_and_cancelled_guards_keep_distinct_vm_diagnostics() {
    for (function, expected) in [
        (
            "std.core.Effect.fail",
            "error[vm_comprehension_guard_failed]",
        ),
        (
            "std.core.Effect.cancelled",
            "error[vm_comprehension_guard_cancelled]",
        ),
    ] {
        let mut guards = vec![CoreExpr::Call {
            function: function.to_string(),
            args: Vec::new(),
        }];

        let error = lower_completed_effect_guards(&mut guards).expect_err("deferred guard exit");

        assert!(error.to_string().starts_with(expected), "{error}");
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
            function: "std.core.GuardResult.from_bool".to_string(),
            args: vec![decision.clone()],
        },
        CoreExpr::Call {
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
        function: "std.core.GuardResult.both".to_string(),
        args: vec![
            CoreExpr::Call {
                function: "std.core.GuardResult.accept".to_string(),
                args: Vec::new(),
            },
            CoreExpr::Call {
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
