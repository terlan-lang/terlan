//! Tests for fail-closed concrete type recovery at dynamic compiler boundaries.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern, CoreTupleTypeElem, CoreType};

use super::dynamic_return::infer_expression_type;

/// Structural recovery admits homogeneous managed lists and nested tuples.
#[test]
fn dynamic_return_recovers_concrete_managed_shapes() {
    let expression = CoreExpr::Tuple(vec![
        CoreExpr::List(vec![CoreExpr::Int(1), CoreExpr::Int(2)]),
        CoreExpr::Binary("native".to_string()),
    ]);

    assert_eq!(
        infer_expression_type(&expression, &HashMap::new()),
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::List(Box::new(CoreType::Int))),
            CoreTupleTypeElem::Type(CoreType::String),
        ]))
    );
}

/// Ambiguous and heterogeneous collection boundaries remain unsupported.
#[test]
fn dynamic_return_rejects_ambiguous_managed_shapes() {
    assert_eq!(
        infer_expression_type(&CoreExpr::List(Vec::new()), &HashMap::new()),
        None
    );
    assert_eq!(
        infer_expression_type(
            &CoreExpr::List(vec![CoreExpr::Int(1), CoreExpr::Binary("two".to_string())]),
            &HashMap::new(),
        ),
        None
    );
}

/// Effectful sequence temporaries do not obscure an independent final value.
#[test]
fn dynamic_return_ignores_unreferenced_unknown_sequence_results() {
    let expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("answer".to_string()),
                value: CoreExpr::Int(42),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("_script_effect".to_string()),
                value: CoreExpr::RemoteCall {
                    type_args: Vec::new(),
                    module: "std.vm.Process".to_string(),
                    function: "fail".to_string(),
                    args: vec![CoreExpr::Int(1)],
                },
            },
        ],
        body: Box::new(CoreExpr::Var("answer".to_string())),
    };

    assert_eq!(
        infer_expression_type(&expression, &HashMap::new()),
        Some(CoreType::Int)
    );
}

/// Unknown shadowing remains unknown instead of leaking the outer type.
#[test]
fn dynamic_return_rejects_unknown_shadowed_result() {
    let variables = HashMap::from([("answer".to_string(), CoreType::Int)]);
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("answer".to_string()),
            value: CoreExpr::RemoteCall {
                type_args: Vec::new(),
                module: "unknown.Module".to_string(),
                function: "value".to_string(),
                args: Vec::new(),
            },
        }],
        body: Box::new(CoreExpr::Var("answer".to_string())),
    };

    assert_eq!(infer_expression_type(&expression, &variables), None);
    assert_eq!(
        super::structured_case::core_expr_type(&expression, &variables, &HashMap::new()),
        None,
        "call-aware recovery must not retain a shadowed outer type"
    );
}

fn checked_core(source: &str) -> crate::terlan_typeck::CoreModule {
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source).unwrap();
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&syntax).module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved)
}

fn dynamic_main(core: &mut crate::terlan_typeck::CoreModule) {
    let function = core
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    function.return_type = "Dynamic".into();
    function.core_return_type = Some(CoreType::Dynamic);
}

/// An actual script's final call must use its checked helper's Boolean ABI.
#[test]
fn dynamic_return_script_direct_call_reaches_native_object() {
    let syntax = crate::terlan_syntax::parse_script_as_syntax_output(
        "answer(): Bool -> true.\nanswer().\n",
        "script.DirectCall",
    )
    .unwrap();
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&syntax).module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = super::NativeModule::lower_application(&[&core]).unwrap();
    let main = modules[0]
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    assert_eq!(main.return_type, super::NativeType::Bool);
    assert_eq!(
        core.functions.last().unwrap().core_return_type,
        Some(CoreType::Dynamic)
    );
    super::emit_native_application_object("script_direct_call", &modules).unwrap();
}

/// Qualified identity and arity select the callee, not another module's short name.
#[test]
fn dynamic_return_resolves_qualified_managed_calls_without_rewriting_body() {
    let mut caller = checked_core("module app.Entry. pub main(): String -> \"placeholder\".");
    dynamic_main(&mut caller);
    caller.imports.push(crate::terlan_typeck::CoreImport {
        module: "app.Library".into(),
        kind: crate::terlan_typeck::CoreImportKind::Module,
    });
    let body = caller.functions[0].clauses[0]
        .body
        .core_expr
        .as_mut()
        .unwrap();
    *body = CoreExpr::Call {
        type_args: Vec::new(),
        function: "app.Library.answer".into(),
        args: vec![],
    };
    let library = checked_core("module app.Library. pub answer(): String -> \"native\".");
    let unrelated = checked_core("module app.Other. pub answer(): Int -> 7.");
    let mut cores = vec![caller, library, unrelated];
    let original_body = cores[0].functions[0].clauses[0].body.clone();
    super::dynamic_return::close_application_returns(&mut cores);
    assert_eq!(
        cores[0].functions[0].core_return_type,
        Some(CoreType::String)
    );
    assert_eq!(cores[0].functions[0].return_type, "String");
    assert_eq!(cores[0].functions[0].clauses[0].body, original_body);
    let once = cores[0].functions[0].clauses[0].body.clone();
    super::dynamic_return::close_application_returns(&mut cores);
    assert_eq!(
        cores[0].functions[0].clauses[0].body, once,
        "boundary annotation must be idempotent"
    );
    // Also exercise the source-IR entry, including its explicit import policy.
    dynamic_main(&mut cores[0]);
    cores[0].functions[0].clauses[0].body.core_expr = Some(CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: "app.Library".into(),
        function: "answer".into(),
        args: vec![],
    });
    let modules =
        super::NativeModule::lower_application(&cores.iter().collect::<Vec<_>>()).unwrap();
    super::emit_native_application_object("script_managed_call", &modules).unwrap();
}

/// Incorrect arity, unknown calls, and ungrounded cycles remain unresolved.
#[test]
fn dynamic_return_does_not_guess_unknown_or_cyclic_call_results() {
    for (name, args) in [
        ("answer", vec![CoreExpr::Int(1)]),
        ("missing", vec![]),
        ("main", vec![]),
    ] {
        let mut core = checked_core(
            "module app.Unknown. pub answer(): Bool -> true. pub main(): Bool -> answer().",
        );
        dynamic_main(&mut core);
        let main = core
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        main.clauses[0].body.core_expr = Some(CoreExpr::Call {
            type_args: Vec::new(),
            function: name.into(),
            args,
        });
        let mut cores = vec![core];
        super::dynamic_return::close_application_returns(&mut cores);
        let main = cores[0]
            .functions
            .iter()
            .find(|f| f.name == "main")
            .unwrap();
        assert_eq!(main.core_return_type, Some(CoreType::Dynamic));
        assert!(super::NativeModule::lower_application(&[&cores[0]]).is_err());
    }
}

/// A finite chain of internal boundaries can close only from a known typed leaf.
#[test]
fn dynamic_return_closes_forward_chains_from_declared_leaf_results() {
    let mut core = checked_core("module app.Chain. pub leaf(): Bool -> true. pub middle(): Bool -> leaf(). pub main(): Bool -> middle().");
    for function in &mut core.functions {
        if function.name != "leaf" {
            function.core_return_type = Some(CoreType::Dynamic);
            function.return_type = "Dynamic".into();
        }
    }
    let mut cores = vec![core];
    super::dynamic_return::close_application_returns(&mut cores);
    for function in &cores[0].functions {
        if function.name != "leaf" {
            assert_eq!(function.core_return_type, Some(CoreType::Bool));
        }
    }
}

/// A branch join cannot turn incompatible scalar results into a guessed ABI.
#[test]
fn dynamic_return_rejects_incompatible_call_branches() {
    let mut core = checked_core(
        "module app.Branches. pub answer(): Bool -> true. pub main(): Bool -> answer().",
    );
    dynamic_main(&mut core);
    let main = core
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    main.clauses[0].body.core_expr = Some(CoreExpr::If {
        clauses: vec![
            crate::terlan_typeck::CoreIfClause {
                condition: CoreExpr::Var("true".into()),
                body: CoreExpr::Call {
                    type_args: Vec::new(),
                    function: "answer".into(),
                    args: vec![],
                },
            },
            crate::terlan_typeck::CoreIfClause {
                condition: CoreExpr::Var("true".into()),
                body: CoreExpr::Int(9),
            },
        ],
    });
    let mut cores = vec![core];
    super::dynamic_return::close_application_returns(&mut cores);
    let main = cores[0]
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    assert_eq!(main.core_return_type, Some(CoreType::Dynamic));
}

/// Duplicate identity with conflicting results must not select the last entry.
#[test]
fn dynamic_return_rejects_conflicting_callee_declarations() {
    let mut core = checked_core(
        "module app.Ambiguous. pub answer(): Bool -> true. pub main(): Bool -> answer().",
    );
    dynamic_main(&mut core);
    let mut conflicting = core
        .functions
        .iter()
        .find(|f| f.name == "answer")
        .unwrap()
        .clone();
    conflicting.core_return_type = Some(CoreType::Int);
    conflicting.return_type = "Int".into();
    core.functions.push(conflicting);
    let mut cores = vec![core];
    super::dynamic_return::close_application_returns(&mut cores);
    let main = cores[0]
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    assert_eq!(main.core_return_type, Some(CoreType::Dynamic));
}

/// Result recovery must leave a suspending grouped guard visible to case lowering.
#[test]
fn dynamic_return_script_grouped_guard_and_final_call_keep_native_control() {
    let syntax = crate::terlan_syntax::parse_script_as_syntax_output(
        r#"
pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
@compiler.native {probe.read}
read(): Option[String] -> native.
accepted(value: String): Bool -> value == "ok".
let Some(value) <- read() else { _ -> false };
accepted(value).
"#,
        "script.GroupedCall",
    )
    .unwrap();
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&syntax).module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = super::NativeModule::lower_application(&[&core]).unwrap();
    let main = modules[0]
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    assert_eq!(main.return_type, super::NativeType::Bool);
    assert!(
        !modules[0].continuations.is_empty(),
        "native read must suspend"
    );
    assert_eq!(
        core.functions
            .iter()
            .find(|f| f.name == "main")
            .unwrap()
            .core_return_type,
        Some(CoreType::Dynamic)
    );
    super::emit_native_application_object("script_grouped_call", &modules).unwrap();
}
