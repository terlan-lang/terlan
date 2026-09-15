use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::parse_module_as_syntax_output;

fn lower(source: &str) -> CoreModule {
    let provider = parse_module_as_syntax_output("module provider.\npub type NotFound.\n")
        .expect("provider source");
    let interfaces = HashMap::from([(
        "provider".to_string(),
        syntax_module_output_to_interface(&provider),
    )]);
    let syntax = parse_module_as_syntax_output(source).expect("consumer source");
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Renaming a singleton type must preserve its canonical value and pattern.
#[test]
fn imported_singleton_alias_retains_atom_value_and_case_pattern() {
    let core = lower("module consumer.\nimport provider.{NotFound as Missing}.\npub value(): Atom -> Missing.\npub matches(value: Atom): Bool -> case value { Missing -> true; _ -> false }.\n");
    let value = core
        .functions
        .iter()
        .find(|function| function.name == "value")
        .unwrap();
    assert_eq!(
        value.clauses[0].body.core_expr,
        Some(CoreExpr::Atom("not_found".to_string()))
    );
    let matches = core
        .functions
        .iter()
        .find(|function| function.name == "matches")
        .unwrap();
    let Some(CoreExpr::Case { clauses, .. }) = &matches.clauses[0].body.core_expr else {
        panic!("case expression");
    };
    assert_eq!(
        clauses[0].pattern,
        CorePattern::Atom("not_found".to_string())
    );
}

/// An imported type name does not replace a same-spelled lexical parameter.
#[test]
fn imported_singleton_alias_does_not_capture_local_parameter() {
    let core = lower("module consumer.\nimport provider.{NotFound as Missing}.\npub shadow(Missing: Int): Int -> Missing.\n");
    assert_eq!(
        core.functions[0].clauses[0].body.core_expr,
        Some(CoreExpr::Var("Missing".to_string()))
    );
    core.binding_identities
        .validate()
        .expect("preserved binding evidence");
}

/// The aliased value must also survive expression lowering inside a case arm.
#[test]
fn imported_singleton_alias_retains_identity_in_comparison() {
    let core = lower("module consumer.\nimport provider.{NotFound as Missing}.\npub matches(value: Atom): Bool -> case value { _ -> value == Missing }.\n");
    let Some(CoreExpr::Case { clauses, .. }) = &core.functions[0].clauses[0].body.core_expr else {
        panic!("case expression");
    };
    assert_eq!(
        clauses[0].body,
        CoreExpr::BinaryOp {
            operator: "==".into(),
            left: Box::new(CoreExpr::Var("value".into())),
            right: Box::new(CoreExpr::Atom("not_found".into())),
        }
    );
}

#[test]
fn imported_singleton_alias_retains_identity_in_lambda_pattern() {
    let core = lower("module consumer.\nimport provider.{NotFound as Missing}.\npub value(): Atom -> let _identity = ((Missing) -> Missing); Missing.\n");
    let Some(CoreExpr::Let { bindings, body }) = &core.functions[0].clauses[0].body.core_expr
    else {
        panic!("let expression");
    };
    let CoreExpr::Lam {
        params,
        body: lambda_body,
        ..
    } = &bindings[0].value
    else {
        panic!("lambda")
    };
    assert_eq!(params, &[CorePattern::Atom("not_found".into())]);
    assert_eq!(**lambda_body, CoreExpr::Atom("not_found".into()));
    assert_eq!(**body, CoreExpr::Atom("not_found".into()));
}
