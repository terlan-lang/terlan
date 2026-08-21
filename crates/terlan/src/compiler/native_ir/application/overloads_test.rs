use super::*;
use crate::{
    terlan_hir::resolve_syntax_module_output, terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::lower_syntax_module_output_to_core,
};

/// Lowers one source fixture into CoreIR without entering NativeIR.
fn core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse overload fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Returns the direct call target retained by one zero-arity function.
fn direct_call_target<'a>(core: &'a CoreModule, function_name: &str) -> &'a str {
    let expression = core
        .functions
        .iter()
        .find(|function| function.name == function_name)
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("typed direct-call body");
    match expression {
        CoreExpr::Call { function, .. } => function,
        other => panic!("expected direct call, found {other:?}"),
    }
}

/// Type-distinct same-arity native overloads retain their selected ABI.
#[test]
fn typed_native_overloads_receive_distinct_internal_call_identities() {
    let mut modules = vec![core(
        "module app.Overload.\n\n\
         @compiler.native {fixture.int}\n\
         choose(_value: Int): Int -> native.\n\n\
         @compiler.native {fixture.list}\n\
         choose(_value: List[Int]): Int -> native.\n\n\
         pub scalar(): Int -> choose(1).\n\n\
         pub collection(): Int -> choose([1, 2]).\n",
    )];

    assert_eq!(
        modules[0]
            .functions
            .iter()
            .filter(|function| function.name == "choose")
            .count(),
        2
    );
    resolve_typed_overloads(&mut modules).expect("resolve typed overloads");

    let integer = modules[0]
        .functions
        .iter()
        .find(|function| function.native_operation.as_deref() == Some("fixture.int"))
        .expect("integer native overload");
    let list = modules[0]
        .functions
        .iter()
        .find(|function| function.native_operation.as_deref() == Some("fixture.list"))
        .expect("list native overload");
    assert_ne!(integer.name, list.name);
    assert_eq!(direct_call_target(&modules[0], "scalar"), integer.name);
    assert_eq!(direct_call_target(&modules[0], "collection"), list.name);
}

/// Alias, list, and Bool literals select a complete same-arity overload.
#[test]
fn typed_overloads_infer_alias_list_and_bool_literal_arguments() {
    let mut modules = vec![core(
        "module app.StructuralOverload.\n\n\
         /** Structural alias used to select the integer overload. */\n\
         pub type Mode: Int = DEFAULT = 0 | OTHER = 1.\n\n\
         choose(_value: Int, _mode: Mode, _axes: List[Int], _keep: Bool): Int -> 1.\n\n\
         choose(_value: Int, _order: Float, _axes: List[Int], _keep: Bool): Int -> 2.\n\n\
         pub selected(): Int -> choose(1, Mode.DEFAULT, [0, 1], true).\n",
    )];

    resolve_typed_overloads(&mut modules).expect("resolve structural overload literals");
    let selected = direct_call_target(&modules[0], "selected");
    let alias = modules[0]
        .functions
        .iter()
        .find(|function| {
            function
                .params
                .get(1)
                .and_then(|parameter| parameter.core_ty.as_ref())
                == Some(&CoreType::Named("Mode".to_string()))
        })
        .expect("alias overload");
    assert_eq!(selected, alias.name);
}

/// Duplicate typed declarations remain for application admission.
#[test]
fn duplicate_core_signature_is_left_for_application_admission() {
    let mut module = core(
        "module app.DuplicateOverload.\n\n\
         @compiler.native {fixture.int}\n\
         choose(_value: Int): Int -> native.\n",
    );
    module.functions.push(module.functions[0].clone());

    resolve_typed_overloads(std::slice::from_mut(&mut module))
        .expect("duplicate declarations are not typed overloads");
    assert_eq!(module.functions[0].name, "choose");
    assert_eq!(module.functions[1].name, "choose");
}
