//! Focused checks for bounded escaping-lambda closure conversion.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreIfClause, CorePattern, CoreType};

use super::{
    closure_conversion::{
        lower_escaping_closure_with_yields, lower_escaping_lambda, ClosureLexicalScope,
        ClosureLoweringEnvironment, ClosureOwner, ClosureYieldState, NativeCallableShape,
    },
    NativeBinaryOperator, NativeConstructorLayouts, NativeExpr, NativeType,
};

fn arrow(arity: usize) -> CoreType {
    CoreType::Arrow {
        params: vec![CoreType::Int; arity],
        return_type: Box::new(CoreType::Int),
    }
}

/// Tests the conversion's explicit no-continuation context without production scaffolding.
fn lower_escaping_closure(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    scope: ClosureLexicalScope<'_>,
    environment: &ClosureLoweringEnvironment<'_>,
    owner: ClosureOwner<'_>,
) -> Result<Option<(NativeExpr, Vec<super::NativeFunction>)>, String> {
    lower_escaping_closure_with_yields(
        body,
        expected,
        scope,
        environment,
        owner,
        &mut ClosureYieldState {
            environment: None,
            stable_ids: &mut HashSet::new(),
            continuations: Vec::new(),
            lifted_ordinal: 0,
        },
    )
}

/// Lifted lambdas preserve captures and intermediate values across real yield/resume edges.
#[test]
fn escaping_callbacks_resume_non_tail_calls_and_direct_yields() {
    check_callbacks(
        r#"
module closure_resume.
import std.vm.Process.
import type std.collections.List.
park(value: Int): Int -> let _parked = Process.yield_now(); value.
park_values(value: Int): List[Int] -> let _parked = Process.yield_now(); [value].
consume(values: List[Int], offset: Int): Int ->
    let _parked = Process.yield_now();
    case values { [first] -> first + offset; _ -> 0 }.
make(seed: Int): ((Int) -> Int) -> (value) -> park(seed + value) + park(2).
make_nested(seed: Int): ((Int) -> Int) -> (value) -> park(park(seed + value) + 1).
make_list(seed: Int): ((Int) -> Int) -> (value) -> consume(park_values(seed + value), park(2)).
make_direct(seed: Int): (() -> Int) -> () -> let _parked = Process.yield_now(); seed + 3.
choose(flag: Bool, seed: Int): ((Int) -> Int) -> if {
    flag -> ((value) -> park(seed + value) + 1);
    true -> ((value) -> park(seed + value) + 1)
}.
pub composed(): Int -> let callback = make(10); callback(5).
pub direct(): Int -> let callback = make_direct(20); callback().
pub left_branch(): Int -> let callback = choose(true, 30); callback(2).
pub right_branch(): Int -> let callback = choose(false, 40); callback(2).
pub nested_tail(): Int -> let callback = make_nested(30); callback(4).
pub list_tail(): Int -> let callback = make_list(30); callback(5).
"#,
        &[
            ("composed", 17),
            ("direct", 23),
            ("left_branch", 33),
            ("right_branch", 43),
            ("nested_tail", 35),
            ("list_tail", 37),
        ],
    );
}

#[test]
fn escaping_callbacks_preserve_captured_callable_contracts() {
    check_callbacks(
        r#"
module closure_captured_callable.
import std.vm.Process.
pub negate(property: (Int) -> Bool): ((Int) -> Bool) ->
    (value) -> property(value) == false.
pub alias(property: (Int) -> Bool): ((Int) -> Bool) ->
    let saved = property;
    (value) -> saved(value) == false.
pub choose(flag: Bool, property: (Int) -> Bool): ((Int) -> Bool) -> if {
    flag -> ((value) -> property(value) == false);
    true -> ((value) -> property(value))
}.
pub shadow(property: (Int) -> Bool): ((Int) -> Bool) ->
    (property) -> property == 7.
negative(value: Int): Bool -> let _parked = Process.yield_now(); value < 0.
pub direct(): Bool -> let check = negate(negative); check(0) and check(-1) == false.
pub saved(): Bool -> let check = alias(negative); check(0) and check(-1) == false.
pub branches(): Bool ->
    let left = choose(true, negative);
    let right = choose(false, negative);
    left(0) and right(-1).
pub shadowed(): Bool -> let check = shadow(negative); check(7).
"#,
        &[
            ("direct", 1),
            ("saved", 1),
            ("branches", 1),
            ("shadowed", 1),
        ],
    );
}

fn check_callbacks(source: &str, cases: &[(&str, i64)]) {
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source)
        .expect("parse suspending callback source");
    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &interfaces)
            .module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        super::NativeModule::lower_application(&[&core]).expect("lower resumable lambdas");
    assert!(modules
        .iter()
        .any(|module| !module.continuations.is_empty()));
    let object = super::emit_native_application_object("closure-resume", &modules)
        .expect("emit resumable lambdas");
    let invocations = cases
        .iter()
        .map(|&(name, expected)| {
            let function = modules
                .iter()
                .flat_map(|module| &module.functions)
                .find(|function| function.name == name)
                .expect("source callable export");
            super::native_object_test_support::NativeObjectInvocation {
                export_id: function.export_id,
                arguments: vec![],
                expected_status: super::status::OK,
                expected_result: Some(expected),
            }
        })
        .collect::<Vec<_>>();
    super::native_object_test_support::assert_managed_native_object_invocations(
        "closure-resume",
        &modules,
        &object,
        &invocations,
    );
}

fn lower(
    body: &CoreExpr,
    expected: &CoreType,
    outer_params: &HashMap<String, usize>,
    outer_types: &HashMap<String, NativeType>,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    suspending: &HashSet<(String, usize)>,
) -> Result<Option<(NativeExpr, super::NativeFunction)>, String> {
    lower_escaping_lambda(
        body,
        Some(expected),
        ClosureLexicalScope {
            available: outer_params,
            available_types: outer_types,
            available_core_types: &HashMap::new(),
        },
        &ClosureLoweringEnvironment {
            identities,
            function_types,
            constructors: &NativeConstructorLayouts::new(),
            suspending,
            callable_shapes: &std::collections::HashMap::new(),
        },
        ClosureOwner {
            module: "closure_test",
            name: "make",
            arity: outer_params.len(),
        },
    )
}

#[test]
fn captured_parameters_are_snapshotted_in_stable_name_order() {
    let lambda = CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("value".to_string())),
                right: Box::new(CoreExpr::Var("zebra".to_string())),
            }),
            right: Box::new(CoreExpr::Var("alpha".to_string())),
        }),
    };
    let outer_params = HashMap::from([("zebra".to_string(), 0), ("alpha".to_string(), 1)]);
    let outer_types = HashMap::from([
        ("zebra".to_string(), NativeType::Int),
        ("alpha".to_string(), NativeType::Int),
    ]);

    let (maker, lifted) = lower(
        &lambda,
        &arrow(1),
        &outer_params,
        &outer_types,
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
    )
    .expect("closure conversion")
    .expect("escaping lambda");

    let NativeExpr::MakeClosure { captures, .. } = maker else {
        panic!("expected closure allocation");
    };
    assert_eq!(captures, vec![NativeExpr::Param(1), NativeExpr::Param(0)]);
    assert_eq!(
        lifted.callable_captures,
        vec![NativeType::Int, NativeType::Int]
    );
    assert_eq!(
        lifted.params,
        vec![NativeType::Int, NativeType::Int, NativeType::Int]
    );
}

#[test]
fn scalar_lexical_prefix_is_evaluated_before_local_capture_snapshot() {
    let expression = CoreExpr::Let {
        bindings: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var("offset".to_string()),
            value: CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("seed".to_string())),
                right: Box::new(CoreExpr::Int(1)),
            },
        }],
        body: Box::new(CoreExpr::Lam {
            parameter_types: Vec::new(),
            params: vec![CorePattern::Var("value".to_string())],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("value".to_string())),
                right: Box::new(CoreExpr::Var("offset".to_string())),
            }),
        }),
    };
    let available = HashMap::from([("seed".to_string(), 0)]);
    let available_types = HashMap::from([("seed".to_string(), NativeType::Int)]);

    let (maker, lifted) = lower_escaping_closure(
        &expression,
        Some(&arrow(1)),
        ClosureLexicalScope {
            available: &available,
            available_types: &available_types,
            available_core_types: &HashMap::new(),
        },
        &ClosureLoweringEnvironment {
            identities: &HashMap::new(),
            function_types: &HashMap::new(),
            constructors: &NativeConstructorLayouts::new(),
            suspending: &HashSet::new(),
            callable_shapes: &HashMap::new(),
        },
        ClosureOwner {
            module: "closure_test",
            name: "make",
            arity: 1,
        },
    )
    .expect("closure conversion")
    .expect("escaping closure");

    let NativeExpr::Let { bindings, body } = maker else {
        panic!("expected lexical prefix");
    };
    assert_eq!(bindings.len(), 1);
    assert!(matches!(
        &bindings[0],
        NativeExpr::Binary {
            operator: NativeBinaryOperator::Add,
            left,
            right,
            ..
        } if **left == NativeExpr::Param(0) && **right == NativeExpr::Int(1)
    ));
    assert!(matches!(
        body.as_ref(),
        NativeExpr::MakeClosure { captures, .. }
            if captures == &vec![NativeExpr::Param(1)]
    ));
    assert_eq!(lifted.len(), 1);
    assert_eq!(lifted[0].callable_captures, vec![NativeType::Int]);
}

#[test]
fn non_closure_let_bypasses_closure_prefix_validation() {
    let expression = CoreExpr::Let {
        bindings: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var("value".to_string()),
            value: CoreExpr::Call {
                type_args: Vec::new(),
                function: "pause".to_string(),
                args: Vec::new(),
            },
        }],
        body: Box::new(CoreExpr::Var("value".to_string())),
    };
    let suspending = HashSet::from([("pause".to_string(), 0)]);

    assert_eq!(
        lower_escaping_closure(
            &expression,
            Some(&CoreType::Int),
            ClosureLexicalScope {
                available: &HashMap::new(),
                available_types: &HashMap::new(),
                available_core_types: &HashMap::new()
            },
            &ClosureLoweringEnvironment {
                identities: &HashMap::new(),
                function_types: &HashMap::new(),
                constructors: &NativeConstructorLayouts::new(),
                suspending: &suspending,
                callable_shapes: &HashMap::new()
            },
            ClosureOwner {
                module: "closure_test",
                name: "ordinary",
                arity: 0
            }
        )
        .expect("non-closure expression"),
        None
    );
}

#[test]
fn closure_branches_receive_distinct_ordered_lifted_identities() {
    let lambda = |operator: &str| CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: operator.to_string(),
            left: Box::new(CoreExpr::Var("value".to_string())),
            right: Box::new(CoreExpr::Var("seed".to_string())),
        }),
    };
    let expression = CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition: CoreExpr::Var("forward".to_string()),
                body: lambda("+"),
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: lambda("-"),
            },
        ],
    };
    let available = HashMap::from([("forward".to_string(), 0), ("seed".to_string(), 1)]);
    let available_types = HashMap::from([
        ("forward".to_string(), NativeType::Bool),
        ("seed".to_string(), NativeType::Int),
    ]);

    let (maker, lifted) = lower_escaping_closure(
        &expression,
        Some(&arrow(1)),
        ClosureLexicalScope {
            available: &available,
            available_types: &available_types,
            available_core_types: &HashMap::new(),
        },
        &ClosureLoweringEnvironment {
            identities: &HashMap::new(),
            function_types: &HashMap::new(),
            constructors: &NativeConstructorLayouts::new(),
            suspending: &HashSet::new(),
            callable_shapes: &HashMap::new(),
        },
        ClosureOwner {
            module: "closure_test",
            name: "choose",
            arity: 2,
        },
    )
    .expect("branch closure conversion")
    .expect("escaping branch closures");

    let NativeExpr::If { clauses } = maker else {
        panic!("expected native closure branch");
    };
    assert_eq!(clauses.len(), 2);
    assert_eq!(lifted.len(), 2);
    assert_eq!(lifted[0].name, "$closure_choose_2_0");
    assert_eq!(lifted[1].name, "$closure_choose_2_1");
    assert_ne!(lifted[0].export_id, lifted[1].export_id);
    assert!(lifted
        .iter()
        .all(|function| function.callable_captures == vec![NativeType::Int]));
}

#[test]
fn closure_branch_rejects_a_non_callable_arm() {
    let expression = CoreExpr::If {
        clauses: vec![CoreIfClause {
            condition: CoreExpr::Atom("true".to_string()),
            body: CoreExpr::Int(1),
        }],
    };

    assert_eq!(
        lower_escaping_closure(&expression, Some(&arrow(1)), ClosureLexicalScope { available: &HashMap::new(), available_types: &HashMap::new(), available_core_types: &HashMap::new() }, &ClosureLoweringEnvironment { identities: &HashMap::new(), function_types: &HashMap::new(), constructors: &NativeConstructorLayouts::new(), suspending: &HashSet::new(), callable_shapes: &HashMap::new() }, ClosureOwner { module: "closure_test", name: "choose", arity: 0 })
        .unwrap_err(),
        "error[native_ir.closure_branch]: every escaping closure branch must produce a callable value"
    );
}

#[test]
fn closure_branch_rejects_a_suspending_condition() {
    let expression = CoreExpr::If {
        clauses: vec![CoreIfClause {
            condition: CoreExpr::Call {
                type_args: Vec::new(),
                function: "pause".to_string(),
                args: Vec::new(),
            },
            body: CoreExpr::Lam {
                parameter_types: Vec::new(),
                params: vec![CorePattern::Var("value".to_string())],
                body: Box::new(CoreExpr::Var("value".to_string())),
            },
        }],
    };

    assert_eq!(
        lower_escaping_closure(&expression, Some(&arrow(1)), ClosureLexicalScope { available: &HashMap::new(), available_types: &HashMap::new(), available_core_types: &HashMap::new() }, &ClosureLoweringEnvironment { identities: &HashMap::new(), function_types: &HashMap::new(), constructors: &NativeConstructorLayouts::new(), suspending: &HashSet::from([("pause".to_string(), 0)]), callable_shapes: &HashMap::new() }, ClosureOwner { module: "closure_test", name: "choose", arity: 0 })
        .unwrap_err(),
        "error[native_ir.closure_branch_suspension]: escaping closure branch condition cannot suspend"
    );
}

#[test]
fn closure_branch_budget_rejects_more_than_sixty_four_clauses() {
    let expression = CoreExpr::If {
        clauses: (0..65)
            .map(|_| CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: CoreExpr::Lam {
                    parameter_types: Vec::new(),
                    params: vec![CorePattern::Var("value".to_string())],
                    body: Box::new(CoreExpr::Var("value".to_string())),
                },
            })
            .collect(),
    };

    assert_eq!(
        lower_escaping_closure(&expression, Some(&arrow(1)), ClosureLexicalScope { available: &HashMap::new(), available_types: &HashMap::new(), available_core_types: &HashMap::new() }, &ClosureLoweringEnvironment { identities: &HashMap::new(), function_types: &HashMap::new(), constructors: &NativeConstructorLayouts::new(), suspending: &HashSet::new(), callable_shapes: &HashMap::new() }, ClosureOwner { module: "closure_test", name: "choose", arity: 0 })
        .unwrap_err(),
        "error[native_ir.closure_branch_limit]: escaping closure branch has 65 clauses; limit is 64"
    );
}

#[test]
fn closure_branch_can_mix_named_and_lifted_targets() {
    let expression = CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition: CoreExpr::Var("named".to_string()),
                body: CoreExpr::RemoteFunRef {
                    module: "app.Math".to_string(),
                    function: "double".to_string(),
                    arity: 1,
                },
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: CoreExpr::Lam {
                    parameter_types: Vec::new(),
                    params: vec![CorePattern::Var("value".to_string())],
                    body: Box::new(CoreExpr::Var("value".to_string())),
                },
            },
        ],
    };
    let available = HashMap::from([("named".to_string(), 0)]);
    let available_types = HashMap::from([("named".to_string(), NativeType::Bool)]);
    let callable_shapes = HashMap::from([(
        ("app.Math.double".to_string(), 1),
        NativeCallableShape {
            id: 88,
            parameters: vec![NativeType::Int],
            result: NativeType::Int,
        },
    )]);

    let (maker, lifted) = lower_escaping_closure(
        &expression,
        Some(&arrow(1)),
        ClosureLexicalScope {
            available: &available,
            available_types: &available_types,
            available_core_types: &HashMap::new(),
        },
        &ClosureLoweringEnvironment {
            identities: &HashMap::new(),
            function_types: &HashMap::new(),
            constructors: &NativeConstructorLayouts::new(),
            suspending: &HashSet::new(),
            callable_shapes: &callable_shapes,
        },
        ClosureOwner {
            module: "closure_test",
            name: "choose",
            arity: 1,
        },
    )
    .expect("mixed branch conversion")
    .expect("mixed escaping closures");

    assert_eq!(lifted.len(), 1);
    assert_eq!(lifted[0].name, "$closure_choose_1_0");
    let NativeExpr::If { clauses } = maker else {
        panic!("expected mixed closure branch");
    };
    let NativeExpr::MakeClosure { encoded, captures } = &clauses[0].1 else {
        panic!("named branch did not allocate closure");
    };
    assert!(captures.is_empty());
    assert_eq!(
        u64::from_le_bytes(encoded[16..24].try_into().expect("callable identity")),
        88
    );
}

#[test]
fn escaping_lambda_rejects_non_variable_parameters() {
    let lambda = CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: vec![CorePattern::Wildcard],
        body: Box::new(CoreExpr::Int(1)),
    };

    assert_eq!(
        lower(
            &lambda,
            &arrow(1),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_parameter]: escaping lambda parameters must be variables"
    );
}

#[test]
fn escaping_lambda_rejects_declared_arity_drift() {
    let lambda = CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: Vec::new(),
        body: Box::new(CoreExpr::Int(1)),
    };

    assert_eq!(
        lower(
            &lambda,
            &arrow(1),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_arity]: escaping lambda declares 0 parameters but its type requires 1"
    );
}

#[test]
fn escaping_lambda_rejects_untyped_non_parameter_captures() {
    let lambda = CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::Var("value".to_string())),
            right: Box::new(CoreExpr::Var("local".to_string())),
        }),
    };

    assert_eq!(
        lower(
            &lambda,
            &arrow(1),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_capture]: `local` is not a typed lexical value"
    );
}

#[test]
fn escaping_lambda_capture_budget_has_stable_prelink_rejection() {
    let lambda = CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: Vec::new(),
        body: Box::new(CoreExpr::Tuple(
            (0..65)
                .map(|index| CoreExpr::Var(format!("capture_{index}")))
                .collect(),
        )),
    };
    let outer_params = (0..65)
        .map(|index| (format!("capture_{index}"), index))
        .collect::<HashMap<_, _>>();
    let outer_types = outer_params
        .keys()
        .cloned()
        .map(|name| (name, NativeType::Int))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        lower(
            &lambda,
            &arrow(0),
            &outer_params,
            &outer_types,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_capture_limit]: escaping lambda captures 65 values; limit is 64"
    );
}

#[test]
fn escaping_lambda_tail_calls_one_admitted_suspending_target() {
    let lambda = CoreExpr::Lam {
        parameter_types: Vec::new(),
        params: Vec::new(),
        body: Box::new(CoreExpr::Call {
            type_args: Vec::new(),
            function: "pause".to_string(),
            args: Vec::new(),
        }),
    };
    let identities = HashMap::from([(("pause".to_string(), 0), 0)]);
    let function_types = HashMap::from([(("pause".to_string(), 0), NativeType::Int)]);
    let suspending = HashSet::from([("pause".to_string(), 0)]);

    let (_, lifted) = lower(
        &lambda,
        &arrow(0),
        &HashMap::new(),
        &HashMap::new(),
        &identities,
        &function_types,
        &suspending,
    )
    .expect("suspending closure lowering")
    .expect("escaping closure");
    assert_eq!(
        lifted.body,
        NativeExpr::TailCall {
            function: 0,
            args: vec![],
            yield_continuation_id: None,
        }
    );
}
