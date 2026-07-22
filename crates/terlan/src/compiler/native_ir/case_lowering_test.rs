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

/// Verifies unsupported scalar atoms fail while structured patterns remain for
/// the managed structured-case lowering pass.
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
    assert!(lower_scalar_cases(&mut atom)
        .unwrap_err()
        .starts_with("error[native_ir.case_pattern]:"));

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
