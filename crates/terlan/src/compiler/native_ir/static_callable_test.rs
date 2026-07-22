//! Tests for bounded static function-value elimination.

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};
use crate::{
    terlan_hir::resolve_syntax_module_output, terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::lower_syntax_module_output_to_core,
};

use super::{
    emit_native_application_object, native_object_test_support::assert_native_object_result,
    static_callable::normalize_static_callables, NativeModule,
};

/// Creates one variable binding for a static-callable fixture.
fn binding(name: &str, value: CoreExpr) -> CoreLetBinding {
    CoreLetBinding {
        pattern: CorePattern::Var(name.to_string()),
        value,
    }
}

/// Creates one single-parameter integer lambda fixture.
fn increment_lambda(capture: &str) -> CoreExpr {
    CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::Var("value".to_string())),
            right: Box::new(CoreExpr::Var(capture.to_string())),
        }),
    }
}

/// Verifies a lambda binding snapshots its lexical value and lowers invocation
/// into ordinary sequential CoreIR.
#[test]
fn captured_lambda_binding_is_erased_into_ordinary_let_control() {
    let source = CoreExpr::Let {
        bindings: vec![
            binding("offset", CoreExpr::Int(2)),
            binding("increment", increment_lambda("offset")),
        ],
        body: Box::new(CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var("increment".to_string())),
            args: vec![CoreExpr::Int(40)],
        }),
    };

    let lowered = normalize_static_callables(&source).expect("static closure lowering");
    let text = format!("{lowered:?}");
    assert!(!text.contains("Lam"), "lowered expression: {text}");
    assert!(!text.contains("FunctionCall"), "lowered expression: {text}");
    assert!(text.contains("$native_closure_capture_0_offset"));
    assert!(text.contains("Int(40)"));
}

/// Verifies immediate lambda invocation lowers without allocating a closure.
#[test]
fn immediately_invoked_lambda_is_beta_lowered() {
    let source = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Lam {
            params: vec![CorePattern::Var("value".to_string())],
            body: Box::new(CoreExpr::Var("value".to_string())),
        }),
        args: vec![CoreExpr::Int(42)],
    };

    assert_eq!(
        normalize_static_callables(&source).expect("immediate lambda lowering"),
        CoreExpr::Let {
            bindings: vec![binding("value", CoreExpr::Int(42))],
            body: Box::new(CoreExpr::Var("value".to_string())),
        }
    );
}

/// Verifies normalization reaches immediate callables nested inside managed
/// aggregate values instead of leaving dynamic closure dispatch behind.
#[test]
fn immediate_lambda_nested_in_tuple_is_beta_lowered() {
    let source = CoreExpr::Tuple(vec![CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Lam {
            params: vec![CorePattern::Var("value".to_string())],
            body: Box::new(CoreExpr::Var("value".to_string())),
        }),
        args: vec![CoreExpr::Int(42)],
    }]);

    let lowered = normalize_static_callables(&source).expect("nested immediate lambda lowering");
    let text = format!("{lowered:?}");
    assert!(!text.contains("Lam"), "lowered expression: {text}");
    assert!(!text.contains("FunctionCall"), "lowered expression: {text}");
    assert!(text.contains("Int(42)"), "lowered expression: {text}");
}

/// Verifies backend function references become qualified direct calls.
#[test]
fn remote_function_reference_invocation_becomes_direct_call() {
    let source = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::RemoteFunRef {
            module: "app.Math".to_string(),
            function: "double".to_string(),
            arity: 1,
        }),
        args: vec![CoreExpr::Int(21)],
    };

    assert_eq!(
        normalize_static_callables(&source).expect("remote function lowering"),
        CoreExpr::Call {
            function: "app.Math.double".to_string(),
            args: vec![CoreExpr::Int(21)],
        }
    );
}

/// Verifies an escaping named reference survives for owned-closure lowering.
#[test]
fn escaping_remote_function_reference_reaches_native_closure_conversion() {
    let reference = CoreExpr::RemoteFunRef {
        module: "app.Math".to_string(),
        function: "double".to_string(),
        arity: 1,
    };

    assert_eq!(
        normalize_static_callables(&reference).expect("escaping named reference"),
        reference
    );
}

/// Verifies dynamic function values survive for typed owned-closure dispatch.
#[test]
fn unresolved_dynamic_call_reaches_owned_closure_lowering() {
    let source = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("callback".to_string())),
        args: vec![CoreExpr::Int(1)],
    };

    assert_eq!(
        normalize_static_callables(&source).expect("retain dynamic closure call"),
        source
    );
}

/// Verifies whole-result lambdas survive for owned-closure conversion.
#[test]
fn escaping_lambda_reaches_native_closure_conversion() {
    let source = CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::Var("value".to_string())),
    };

    assert_eq!(
        normalize_static_callables(&source).expect("escaping lambda"),
        source
    );
}

/// Verifies a terminal callable binding is retained for owned closure lowering.
#[test]
fn terminal_bound_callable_reaches_native_closure_conversion() {
    let source = CoreExpr::Let {
        bindings: vec![binding(
            "identity",
            CoreExpr::Lam {
                params: vec![CorePattern::Var("value".to_string())],
                body: Box::new(CoreExpr::Var("value".to_string())),
            },
        )],
        body: Box::new(CoreExpr::Var("identity".to_string())),
    };

    assert_eq!(
        normalize_static_callables(&source).expect("terminal escaping callable"),
        CoreExpr::Lam {
            params: vec![CorePattern::Var("value".to_string())],
            body: Box::new(CoreExpr::Var("value".to_string())),
        }
    );
}

/// Verifies a terminal named-function alias preserves its qualified identity.
#[test]
fn terminal_named_alias_reaches_native_closure_conversion() {
    let source = CoreExpr::Let {
        bindings: vec![binding(
            "callback",
            CoreExpr::RemoteFunRef {
                module: "app.Math".to_string(),
                function: "double".to_string(),
                arity: 1,
            },
        )],
        body: Box::new(CoreExpr::Var("callback".to_string())),
    };

    assert_eq!(
        normalize_static_callables(&source).expect("terminal named alias"),
        CoreExpr::RemoteFunRef {
            module: "app.Math".to_string(),
            function: "double".to_string(),
            arity: 1,
        }
    );
}

/// Verifies non-terminal callable escape remains rejected until general
/// closure-valued local lowering exists.
#[test]
fn nonterminal_bound_callable_value_escape_is_rejected() {
    let source = CoreExpr::Let {
        bindings: vec![
            binding(
                "identity",
                CoreExpr::Lam {
                    params: vec![CorePattern::Var("value".to_string())],
                    body: Box::new(CoreExpr::Var("value".to_string())),
                },
            ),
            binding("after", CoreExpr::Int(1)),
        ],
        body: Box::new(CoreExpr::Var("identity".to_string())),
    };

    assert_eq!(
        normalize_static_callables(&source).unwrap_err(),
        "error[native_ir.function_value_escape]: `identity` escapes static native lowering"
    );
}

/// Verifies malformed backend call arity fails with a stable diagnostic.
#[test]
fn static_function_reference_arity_is_checked() {
    let source = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::RemoteFunRef {
            module: "app.Math".to_string(),
            function: "double".to_string(),
            arity: 1,
        }),
        args: Vec::new(),
    };

    assert_eq!(
        normalize_static_callables(&source).unwrap_err(),
        "error[native_ir.function_value_arity]: expected 1 arguments but received 0"
    );
}

/// Verifies statically expanding function values is bounded before native
/// object generation can amplify malformed CoreIR without limit.
#[test]
fn static_callable_specialization_explosion_is_rejected() {
    let mut source = CoreExpr::Int(1);
    for _ in 0..129 {
        source = CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Lam {
                params: vec![CorePattern::Var("value".to_string())],
                body: Box::new(CoreExpr::Var("value".to_string())),
            }),
            args: vec![source],
        };
    }

    assert_eq!(
        normalize_static_callables(&source).unwrap_err(),
        "error[native_ir.specialization_limit]: static function-value expansion exceeds 128 calls"
    );
}

/// Verifies the iterative preflight traverses aggregate children before the
/// recursive normalizer can encounter an adversarial immediate-call chain.
#[test]
fn nested_static_callable_specialization_explosion_is_rejected() {
    let mut nested = CoreExpr::Int(1);
    for _ in 0..129 {
        nested = CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Lam {
                params: vec![CorePattern::Var("value".to_string())],
                body: Box::new(CoreExpr::Var("value".to_string())),
            }),
            args: vec![nested],
        };
    }
    let source = CoreExpr::Tuple(vec![nested]);

    assert_eq!(
        normalize_static_callables(&source).unwrap_err(),
        "error[native_ir.specialization_limit]: static function-value expansion exceeds 128 calls"
    );
}

#[test]
fn static_callable_capture_budget_has_stable_prelink_rejection() {
    let mut bindings = (0..65)
        .map(|index| binding(&format!("capture_{index}"), CoreExpr::Int(index)))
        .collect::<Vec<_>>();
    bindings.push(binding(
        "callback",
        CoreExpr::Lam {
            params: Vec::new(),
            body: Box::new(CoreExpr::Tuple(
                (0..65)
                    .map(|index| CoreExpr::Var(format!("capture_{index}")))
                    .collect(),
            )),
        },
    ));
    let source = CoreExpr::Let {
        bindings,
        body: Box::new(CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var("callback".to_string())),
            args: Vec::new(),
        }),
    };

    assert_eq!(
        normalize_static_callables(&source).unwrap_err(),
        "error[native_ir.closure_capture_limit]: static closure captures 65 values; maximum is 64"
    );
}

#[test]
fn static_callable_renames_captures_inside_managed_shapes() {
    let source = CoreExpr::Let {
        bindings: vec![
            binding("left", CoreExpr::Int(20)),
            binding("right", CoreExpr::Int(22)),
            binding(
                "callback",
                CoreExpr::Lam {
                    params: Vec::new(),
                    body: Box::new(CoreExpr::Tuple(vec![
                        CoreExpr::Var("left".to_string()),
                        CoreExpr::Var("right".to_string()),
                    ])),
                },
            ),
        ],
        body: Box::new(CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var("callback".to_string())),
            args: Vec::new(),
        }),
    };

    let lowered = normalize_static_callables(&source).expect("rename managed-shape captures");
    let text = format!("{lowered:?}");
    assert!(text.contains("$native_closure_capture_0_left"), "{text}");
    assert!(text.contains("$native_closure_capture_1_right"), "{text}");
    assert!(
        !text.contains("Tuple([Var(\"left\"), Var(\"right\")])"),
        "{text}"
    );
}

/// Verifies canonical Terlan source reaches application-wide NativeIR after
/// static closure elimination.
#[test]
fn source_captured_lambda_lowers_into_native_application() {
    let module = parse_module_as_syntax_output(
        "module static_closure.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n\
         pub captured(): Int ->\n\
             let offset = 2;\n\
             let increment = ((value: Int) -> value + offset);\n\
             increment(40).\n\n\
         pub specialized(): Int ->\n\
             let offset = 2;\n\
             apply(40, ((value: Int) -> value + offset)).\n\n\
         pub main(): Bool -> specialized() == 42.\n",
    )
    .expect("parse static closure source");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let modules = NativeModule::lower_application(&[&core]).unwrap_or_else(|error| {
        let captured = core
            .functions
            .iter()
            .find(|function| function.name == "captured")
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .map(|body| normalize_static_callables(body))
            .transpose()
            .expect("normalize source closure");
        panic!("{error}; normalized body: {captured:#?}")
    });
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "specialized")
        .expect("specialized native export")
        .export_id;
    let object = emit_native_application_object("static_closure", &modules)
        .expect("emit static closure native object");
    assert_native_object_result("static-closure", &object, export_id, &[], 42);
}
