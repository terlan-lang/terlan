//! Tests for scalar `Case` elimination before NativeIR admission.

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{
        lower_syntax_module_output_to_core, CoreCaseClause, CoreExpr, CoreModule, CorePattern,
    },
};

use super::{
    case_lowering::lower_scalar_cases, emit_native_application_object,
    native_object_test_support::assert_native_object_result, NativeModule,
};

/// Lowers canonical Terlan source into mutable CoreIR.
fn core(source: &str) -> CoreModule {
    let module = parse_module_as_syntax_output(source).expect("parse scalar case source");
    let resolved = resolve_syntax_module_output(&module).module;
    lower_syntax_module_output_to_core(&module, &resolved)
}

#[test]
fn structured_lambda_arguments_retain_aggregate_types() {
    let module = core(
        r#"
module structured_lambda_arguments.
pub type Pair = {Atom["pair"], Int, Int}.
pub answer(): Int ->
    let add = (({left, right}) -> left + right);
    let head_plus_tail = (([head | tail]) ->
        case tail { [next] -> head + next; _ -> 0 });
    let score = ((Pair(_id, size)) -> size * 10);
    let Pair(_id, bonus) = Pair(0, 2);
    add({4, 5}) + head_plus_tail([3, 4]) + score(Pair(7, 3)) + bonus.
"#,
    );
    let modules = NativeModule::lower_application(&[&module]).expect("lower aggregate lambdas");
    let object = emit_native_application_object("aggregate-lambdas", &modules)
        .expect("emit aggregate lambdas");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("lambda export")
        .export_id;
    super::native_object_test_support::assert_managed_native_object_invocations(
        "aggregate-lambdas",
        &modules,
        &object,
        &[super::native_object_test_support::NativeObjectInvocation {
            export_id,
            arguments: vec![],
            expected_status: super::status::OK,
            expected_result: Some(48),
        }],
    );
}

#[test]
fn ordered_function_heads_specialize_patterns_and_keep_guard_scope() {
    let module = core(
        "module native_function_heads.\n\
         pair_or_scalar[T](value: T): Int.\n\
         pair_or_scalar({left, right}) -> left + right;\n\
         pair_or_scalar(_) -> 0.\n\
         choose(value: Int): Int.\n\
         choose(value) where value >= 0 -> 10;\n\
         choose(1) -> 20;\n\
         choose(_) -> 30.\n\
         combine(left: Int, right: Int): Int.\n\
         combine(x, y) where x > y -> x - y;\n\
         combine(_, y) -> y.\n\
         zero(): Int.\n\
         zero() where false -> 100;\n\
         zero() -> 0.\n\
         pub answer(value: Int): Int ->\n\
             pair_or_scalar({value, 5}) + pair_or_scalar(value)\n\
             + choose(value) + combine(value, 2) + zero().\n",
    );
    let modules = NativeModule::lower_application(&[&module]).expect("lower function heads");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("answer export")
        .export_id;
    let object = emit_native_application_object("function-heads", &modules).expect("emit heads");
    let invocations = [(4, 21), (-1, 36), (1, 18)].map(|(argument, expected)| {
        super::native_object_test_support::NativeObjectInvocation {
            export_id,
            arguments: vec![argument],
            expected_status: super::status::OK,
            expected_result: Some(expected),
        }
    });
    super::native_object_test_support::assert_managed_native_object_invocations(
        "function-heads",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn function_head_clause_budget_is_checked_before_normalization() {
    let mut module = core("module heads_budget.\nf(value: Int): Int -> value.\n");
    let clause = module.functions[0].clauses[0].clone();
    module.functions[0].clauses = vec![clause; 257];
    let error = lower_scalar_cases(&mut module).expect_err("reject excessive head clauses");
    assert!(
        error.starts_with("error[native_ir.function_head_budget]"),
        "{error}"
    );
}

#[test]
fn constant_empty_list_cases_keep_guards_and_skip_unreachable_heads() {
    let module = core(
        r#"
module native_empty_case.
pub choose(guard: Bool): Int ->
    case [] {
        [head | _tail] -> head;
        [] where guard -> 1;
        [] -> 2
    }.
pub missing(): Int -> case [] { [_head] -> 3 }.
"#,
    );
    let modules = NativeModule::lower_application(&[&module]).expect("lower empty list cases");
    let object =
        emit_native_application_object("empty-list-case", &modules).expect("emit empty case");
    let mut invocations = Vec::new();
    for (name, arguments, expected_status, expected_result) in [
        ("choose", vec![1], super::status::OK, Some(1)),
        ("choose", vec![0], super::status::OK, Some(2)),
        ("missing", vec![], super::status::NO_MATCHING_BRANCH, None),
    ] {
        let export_id = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("empty-case export")
            .export_id;
        invocations.push(super::native_object_test_support::NativeObjectInvocation {
            export_id,
            arguments,
            expected_status,
            expected_result,
        });
    }
    super::native_object_test_support::assert_managed_native_object_invocations(
        "empty-list-case",
        &modules,
        &object,
        &invocations,
    );
}

/// Builds retained structured control with a scalar result for operand tests.
fn structured_operand() -> CoreExpr {
    CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Tuple(vec![CoreExpr::Int(42)])),
        clauses: vec![CoreCaseClause {
            pattern: CorePattern::Tuple(vec![CorePattern::Var("selected".into())]),
            guard: None,
            body: CoreExpr::Var("selected".into()),
        }],
    }
}

/// Verifies scalar siblings retain their order around an eager control operand.
#[test]
fn eager_case_operands_capture_scalar_siblings_in_source_order() {
    for binary in [false, true] {
        let mut module = core("module eager_case.\npub answer(): Int -> 0.\n");
        let first = CoreExpr::Call {
            function: "first".into(),
            args: vec![],
        };
        *function_body_mut(&mut module, "answer") = if binary {
            CoreExpr::BinaryOp {
                operator: "+".into(),
                left: Box::new(first),
                right: Box::new(structured_operand()),
            }
        } else {
            CoreExpr::Call {
                function: "consume".into(),
                args: vec![
                    first,
                    structured_operand(),
                    CoreExpr::Call {
                        function: "last".into(),
                        args: vec![],
                    },
                ],
            }
        };
        lower_scalar_cases(&mut module).expect("normalize eager operands");
        let CoreExpr::Let { bindings, .. } = function_body_mut(&mut module, "answer") else {
            panic!("eager operands require ordered owners");
        };
        assert_eq!(bindings.len(), if binary { 2 } else { 3 });
        assert!(
            matches!(&bindings[0].value, CoreExpr::Call { function, .. } if function == "first")
        );
        assert!(matches!(&bindings[1].value, CoreExpr::Case { .. }));
        if !binary {
            assert!(
                matches!(&bindings[2].value, CoreExpr::Call { function, .. } if function == "last")
            );
        }
    }
}

/// Verifies normalization does not evaluate a short-circuit branch eagerly.
#[test]
fn eager_case_operands_stay_inside_short_circuit_branch() {
    let mut module = core("module lazy_case.\npub answer(): Bool -> false.\n");
    *function_body_mut(&mut module, "answer") = CoreExpr::BinaryOp {
        operator: "and".into(),
        left: Box::new(CoreExpr::Atom("false".into())),
        right: Box::new(CoreExpr::Call {
            function: "consume".into(),
            args: vec![structured_operand()],
        }),
    };
    lower_scalar_cases(&mut module).expect("normalize lazy operands");
    let CoreExpr::If { clauses } = function_body_mut(&mut module, "answer") else {
        panic!("short circuit must retain branch ownership");
    };
    assert!(matches!(&clauses[0].condition, CoreExpr::Atom(value) if value == "false"));
    assert!(matches!(&clauses[0].body, CoreExpr::Let { .. }));
    assert!(matches!(&clauses[1].body, CoreExpr::Atom(value) if value == "false"));
}

/// Verifies a structured case call argument reaches linked native execution.
#[test]
fn eager_structured_case_call_argument_executes_native_object() {
    let module = core(
        "module eager_native_case.\n\
         identity(value: Int): Int -> value.\n\
         pub answer(): Int -> identity(case {40, 2} { {left, right} -> left + right }).\n",
    );
    let modules = NativeModule::lower_application(&[&module]).expect("lower eager structured case");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("answer export")
        .export_id;
    let object = emit_native_application_object("eager_native_case", &modules)
        .expect("emit eager structured case object");
    assert_native_object_result("eager-native-case", &object, export_id, &[], 42);
}

/// Verifies a later control condition is not lifted before an earlier branch.
#[test]
fn eager_case_condition_keeps_preceding_branch_lazy() {
    use crate::terlan_typeck::CoreIfClause;
    let mut module = core("module conditional_case.\npub answer(): Int -> 0.\n");
    *function_body_mut(&mut module, "answer") = CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition: CoreExpr::Atom("true".into()),
                body: CoreExpr::Int(7),
            },
            CoreIfClause {
                condition: structured_operand(),
                body: CoreExpr::Int(8),
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".into()),
                body: CoreExpr::Int(9),
            },
        ],
    };
    lower_scalar_cases(&mut module).expect("normalize conditional owners");
    let CoreExpr::If { clauses } = function_body_mut(&mut module, "answer") else {
        panic!("the preceding branch must stay outside the new owner");
    };
    assert_eq!(clauses.len(), 2);
    assert!(matches!(clauses[0].body, CoreExpr::Int(7)));
    let CoreExpr::Let { bindings, body } = &clauses[1].body else {
        panic!("the later condition belongs to the fallback branch");
    };
    assert_eq!(bindings.len(), 1);
    assert!(matches!(bindings[0].value, CoreExpr::Case { .. }));
    let CoreExpr::If { clauses } = body.as_ref() else {
        panic!("resumed condition")
    };
    assert!(matches!(clauses[0].body, CoreExpr::Int(8)));
    assert!(matches!(clauses[1].body, CoreExpr::Int(9)));
}

/// Returns the mutable body of a named single-clause function fixture.
fn function_body_mut<'a>(core: &'a mut CoreModule, name: &str) -> &'a mut CoreExpr {
    core.functions
        .iter_mut()
        .find(|function| function.name == name)
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("function body")
}

/// Counts calls to one local function in the lowered scalar expression subset.
fn count_calls(expr: &CoreExpr, expected: &str) -> usize {
    match expr {
        CoreExpr::Call { function, args } => {
            usize::from(function == expected)
                + args
                    .iter()
                    .map(|argument| count_calls(argument, expected))
                    .sum::<usize>()
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .map(|binding| count_calls(&binding.value, expected))
                .sum::<usize>()
                + count_calls(body, expected)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .map(|clause| {
                count_calls(&clause.condition, expected) + count_calls(&clause.body, expected)
            })
            .sum(),
        CoreExpr::UnaryOp { operand, .. } => count_calls(operand, expected),
        CoreExpr::BinaryOp { left, right, .. } => {
            count_calls(left, expected) + count_calls(right, expected)
        }
        _ => 0,
    }
}

/// Reports whether one expression contains a compiler-private managed call.
fn contains_managed_call(expr: &CoreExpr, expected: &str) -> bool {
    match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            (module == "$terlan.managed.http" && function == expected)
                || args
                    .iter()
                    .any(|argument| contains_managed_call(argument, expected))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| contains_managed_call(&binding.value, expected))
                || contains_managed_call(body, expected)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            contains_managed_call(&clause.condition, expected)
                || contains_managed_call(&clause.body, expected)
        }),
        CoreExpr::BinaryOp { left, right, .. } => {
            contains_managed_call(left, expected) || contains_managed_call(right, expected)
        }
        _ => false,
    }
}

/// Verifies string patterns lower to value equality instead of reference equality.
#[test]
fn string_case_patterns_lower_to_managed_value_equality() {
    let mut core = core(
        "module string_case.\n\n\
         pub matches(value: String): Bool ->\n\
             case value {\n\
                 \"route\" -> true;\n\
                 _ -> false\n\
             }.\n",
    );

    lower_scalar_cases(&mut core).expect("lower string case");

    assert!(contains_managed_call(
        function_body_mut(&mut core, "matches"),
        "string_equal"
    ));
}

/// Verifies canonical source preserves one-time scrutinee evaluation, clause
/// order, guard captures, native object emission, linking, and execution.
#[test]
fn scalar_case_source_executes_through_linked_native_object() {
    let mut core = core(
        "module scalar_case_source.\n\n\
         next(): Int -> 41.\n\n\
         pub answer(): Int ->\n\
             case next() {\n\
                 0 -> 0;\n\
                 matched where matched > 0 -> matched + 1;\n\
                 _ -> -1\n\
             }.\n",
    );
    lower_scalar_cases(&mut core).expect("lower scalar source case");

    let body = function_body_mut(&mut core, "answer");
    assert_eq!(count_calls(body, "next"), 1);
    let CoreExpr::Let { bindings, body } = body else {
        panic!("case must begin with one scrutinee binding");
    };
    assert_eq!(bindings.len(), 1);
    let CoreExpr::If { clauses } = body.as_ref() else {
        panic!("case binding must contain ordered if control");
    };
    assert_eq!(clauses.len(), 3);
    assert!(matches!(clauses[1].condition, CoreExpr::Let { .. }));
    assert!(matches!(clauses[1].body, CoreExpr::Let { .. }));

    let modules = NativeModule::lower_application(&[&core]).expect("lower scalar case module");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("answer export")
        .export_id;
    let object = emit_native_application_object("scalar_case_source", &modules)
        .expect("emit scalar case object");
    assert_native_object_result("scalar-case", &object, export_id, &[], 42);
}

/// Replaces BEAM jump-table availability introspection with executable native
/// evidence for adjacent and widely separated integer dispatch branches.
#[test]
fn dense_and_sparse_integer_dispatch_executes_through_linked_native_object() {
    let core = core(
        "module native_integer_dispatch.\n\n\
         pub dispatch(value: Int): Int ->\n\
             case value {\n\
                 0 -> 20;\n\
                 1 -> 21;\n\
                 2 -> 22;\n\
                 3 -> 23;\n\
                 1024 -> 30;\n\
                 1000000 -> 31;\n\
                 9223372036854775807 -> 32;\n\
                 _ -> -1\n\
             }.\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("lower integer dispatch module");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "dispatch")
        .expect("dispatch export")
        .export_id;
    let object = emit_native_application_object("native_integer_dispatch", &modules)
        .expect("emit integer dispatch object");

    for (input, expected) in [
        (0, 20),
        (1, 21),
        (2, 22),
        (3, 23),
        (1_024, 30),
        (1_000_000, 31),
        (i64::MAX, 32),
        (99, -1),
        (-3, -1),
    ] {
        assert_native_object_result(
            &format!("native-integer-dispatch-{input}"),
            &object,
            export_id,
            &[input],
            expected,
        );
    }
}

/// Verifies aliases and nested variable patterns bind the same scalar value in
/// both guards and selected bodies.
#[test]
fn scalar_alias_patterns_bind_guards_and_bodies() {
    let mut core = core("module scalar_alias.\n\npub answer(): Int -> 0.\n");
    *function_body_mut(&mut core, "answer") = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Int(7)),
        clauses: vec![CoreCaseClause {
            pattern: CorePattern::Alias {
                alias: "whole".to_string(),
                pattern: Box::new(CorePattern::Var("part".to_string())),
            },
            guard: Some(CoreExpr::BinaryOp {
                operator: "==".to_string(),
                left: Box::new(CoreExpr::Var("whole".to_string())),
                right: Box::new(CoreExpr::Var("part".to_string())),
            }),
            body: CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("whole".to_string())),
                right: Box::new(CoreExpr::Var("part".to_string())),
            },
        }],
    };

    let modules = NativeModule::lower_application(&[&core]).expect("lower alias case");
    let export_id = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "answer")
        .expect("alias answer export")
        .export_id;
    let object =
        emit_native_application_object("scalar_alias", &modules).expect("emit alias case object");
    assert_native_object_result("scalar-alias", &object, export_id, &[], 14);
}

/// Verifies case elimination exposes higher-order calls in branch bodies to
/// the following bounded specialization and static-callable passes.
#[test]
fn scalar_case_composes_with_higher_order_specialization() {
    let core = core(
        "module scalar_case_higher_order.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n\
         pub answer(): Int ->\n\
             case 1 {\n\
                 1 -> apply(40, ((value: Int) -> value + 2));\n\
                 _ -> 0\n\
             }.\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("lower composed case module");
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .all(|function| function.name != "apply"));
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("composed answer export")
        .export_id;
    let object = emit_native_application_object("scalar_case_higher_order", &modules)
        .expect("emit composed case object");
    assert_native_object_result("scalar-case-hofn", &object, export_id, &[], 42);
}

/// Verifies boolean and `Unit` literals use native word equality in ordered
/// scalar pattern matching.
#[test]
fn boolean_and_unit_patterns_execute() {
    let core = core(
        "module scalar_case_literals.\n\n\
         pub answer(): Int ->\n\
             case true {\n\
                 false -> 0;\n\
                 true -> case Unit { Unit -> 42; _ -> 0 };\n\
                 _ -> 0\n\
             }.\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("lower literal case module");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("literal answer export")
        .export_id;
    let object = emit_native_application_object("scalar_case_literals", &modules)
        .expect("emit literal case object");
    assert_native_object_result("scalar-case-literals", &object, export_id, &[], 42);
}

/// Verifies finite Float patterns and equality execute with numeric rather
/// than raw-bit semantics.
#[test]
fn finite_float_patterns_and_equality_execute() {
    let core = core(
        "module scalar_case_float.\n\n\
         pub answer(value: Float): Int ->\n\
             case value {\n\
                 1.5 -> 42;\n\
                 _ -> 0\n\
             }.\n\n\
         pub equal(left: Float, right: Float): Bool -> left == right.\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("lower Float case module");
    let answer = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("Float answer export")
        .export_id;
    let equal = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "equal")
        .expect("Float equality export")
        .export_id;
    let object = emit_native_application_object("scalar_case_float", &modules)
        .expect("emit Float case object");
    assert_native_object_result(
        "scalar-case-float",
        &object,
        answer,
        &[1.5_f64.to_bits() as i64],
        42,
    );
    assert_native_object_result(
        "scalar-float-zero-equality",
        &object,
        equal,
        &[0.0_f64.to_bits() as i64, (-0.0_f64).to_bits() as i64],
        1,
    );
}

/// Verifies non-finite Float patterns fail before NativeIR construction.
#[test]
fn non_finite_float_pattern_fails_closed() {
    let mut core = core("module bad_float_case.\n\npub answer(): Int -> 0.\n");
    *function_body_mut(&mut core, "answer") = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Float("1.0".to_string())),
        clauses: vec![CoreCaseClause {
            pattern: CorePattern::Float("NaN".to_string()),
            guard: None,
            body: CoreExpr::Int(1),
        }],
    };
    assert_eq!(
        lower_scalar_cases(&mut core).unwrap_err(),
        "error[native_ir.case_float]: invalid Float pattern `NaN`: value must be finite"
    );
}

/// Verifies nested cases are recursively eliminated rather than surviving into
/// scalar candidate admission.
#[test]
fn nested_scalar_cases_are_eliminated() {
    let mut core = core(
        "module nested_scalar_case.\n\n\
         pub answer(): Int ->\n\
             case 1 {\n\
                 1 -> case 2 { 2 -> 42; _ -> 0 };\n\
                 _ -> 0\n\
             }.\n",
    );
    lower_scalar_cases(&mut core).expect("lower nested scalar cases");
    let body = function_body_mut(&mut core, "answer");
    assert!(!format!("{body:?}").contains("Case"));
}

/// Verifies scalar atom constructors lower directly while structured patterns
/// remain for the managed structured-case lowering pass.
#[test]
fn unsupported_case_patterns_fail_closed() {
    let mut atom = core("module bad_scalar_case.\n\npub answer(): Int -> 0.\n");
    *function_body_mut(&mut atom, "answer") = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Int(1)),
        clauses: vec![CoreCaseClause {
            pattern: CorePattern::Atom("ready".to_string()),
            guard: None,
            body: CoreExpr::Int(1),
        }],
    };
    lower_scalar_cases(&mut atom).expect("lower arbitrary scalar atom pattern");
    assert!(!format!("{:?}", function_body_mut(&mut atom, "answer")).contains("Case"));

    for pattern in [
        CorePattern::Tuple(vec![CorePattern::Int(1)]),
        CorePattern::List(vec![CorePattern::Int(1)]),
    ] {
        let mut core = core("module bad_scalar_case.\n\npub answer(): Int -> 0.\n");
        *function_body_mut(&mut core, "answer") = CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Int(1)),
            clauses: vec![CoreCaseClause {
                pattern,
                guard: None,
                body: CoreExpr::Int(1),
            }],
        };
        lower_scalar_cases(&mut core).expect("defer structured case");
        assert!(matches!(
            function_body_mut(&mut core, "answer"),
            CoreExpr::Case { .. }
        ));
    }
}

/// Verifies malformed, oversized, and excessively nested cases are bounded
/// before NativeIR construction.
#[test]
fn scalar_case_shape_limits_are_enforced() {
    let mut empty = core("module empty_case.\n\npub answer(): Int -> 0.\n");
    *function_body_mut(&mut empty, "answer") = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Int(1)),
        clauses: Vec::new(),
    };
    assert_eq!(
        lower_scalar_cases(&mut empty).unwrap_err(),
        "error[native_ir.case_empty]: scalar case has no clauses"
    );

    let mut wide = core("module wide_case.\n\npub answer(): Int -> 0.\n");
    *function_body_mut(&mut wide, "answer") = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Int(1)),
        clauses: (0..257)
            .map(|value| CoreCaseClause {
                pattern: CorePattern::Int(value),
                guard: None,
                body: CoreExpr::Int(value),
            })
            .collect(),
    };
    assert!(lower_scalar_cases(&mut wide)
        .unwrap_err()
        .starts_with("error[native_ir.case_clause_limit]:"));

    let mut deep = core("module deep_case.\n\npub answer(): Int -> 0.\n");
    let mut body = CoreExpr::Int(0);
    for _ in 0..65 {
        body = CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Int(1)),
            clauses: vec![CoreCaseClause {
                pattern: CorePattern::Wildcard,
                guard: None,
                body,
            }],
        };
    }
    *function_body_mut(&mut deep, "answer") = body;
    assert!(lower_scalar_cases(&mut deep)
        .unwrap_err()
        .starts_with("error[native_ir.case_depth_limit]:"));
}
