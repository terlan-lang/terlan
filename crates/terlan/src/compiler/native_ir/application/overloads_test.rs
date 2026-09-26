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

/// Result constructor payloads retain their type for downstream overloads.
#[test]
fn result_pattern_payload_type_selects_nested_overloads() {
    let result = CoreType::Apply {
        constructor: "Result".to_string(),
        args: vec![
            CoreType::Named("Expr".to_string()),
            CoreType::Named("Error".to_string()),
        ],
    };
    let pattern = CorePattern::Constructor {
        name: "Ok".to_string(),
        constructor_identity: Some("std.core.Result.Ok".to_string()),
        args: vec![CorePattern::Var("expression".to_string())],
    };
    let mut environment = HashMap::new();

    bind_pattern_type(&pattern, &result, &mut environment);

    assert_eq!(
        environment.get("expression"),
        Some(&CoreType::Named("Expr".to_string()))
    );
}

/// Transparent Result lowering uses an equivalent tagged-tuple pattern.
#[test]
fn tagged_result_pattern_payload_type_is_retained() {
    let result = CoreType::Apply {
        constructor: "std.core.Result.Result".to_string(),
        args: vec![CoreType::Int, CoreType::String],
    };
    let pattern = CorePattern::Tuple(vec![
        CorePattern::Atom("ok".to_string()),
        CorePattern::Var("value".to_string()),
    ]);
    let mut environment = HashMap::new();

    bind_pattern_type(&pattern, &result, &mut environment);

    assert_eq!(environment.get("value"), Some(&CoreType::Int));
}

/// A singleton literal selects the union overload that contains its alias.
#[test]
fn atom_literal_matches_a_transparent_union_member() {
    let expected = CoreType::Named("DataType".to_string());
    let actual = CoreType::AtomLiteral("int_64".to_string());
    let aliases = HashMap::from([
        (
            "DataType".to_string(),
            CoreType::Union(vec![
                CoreType::Named("Int64".to_string()),
                CoreType::Named("Float64".to_string()),
            ]),
        ),
        (
            "Int64".to_string(),
            CoreType::AtomLiteral("int_64".to_string()),
        ),
        (
            "Float64".to_string(),
            CoreType::AtomLiteral("float_64".to_string()),
        ),
    ]);

    assert!(type_match_score(&expected, &actual, &aliases).is_some());
    assert!(type_match_score(
        &expected,
        &CoreType::AtomLiteral("utf_8".to_string()),
        &aliases
    )
    .is_none());
}
