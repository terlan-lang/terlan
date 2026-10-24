//! Qualified callable values must not capture module aliases as runtime data.

use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};

fn lower(body: &str) -> CoreExpr {
    let provider = parse_module_as_syntax_output("module app.Provider.\npub apply(value: Int): Int -> value.\nprivate(value: Int): Int -> value.\npub many(): Int -> 0.\npub many(value: Int): Int -> value.").unwrap();
    let interfaces = HashMap::from([(
        "app.Provider".into(),
        syntax_module_output_to_interface(&provider),
    )]);
    let source = format!("module app.Consumer.\nimport app.Provider as provider.\n{body}");
    let syntax = parse_module_as_syntax_output(&source).unwrap();
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.functions
        .iter()
        .find(|function| function.name == "example")
        .unwrap()
        .clauses[0]
        .body
        .core_expr
        .clone()
        .unwrap()
}

#[test]
fn qualified_callable_value_retains_provider_name_and_arity() {
    assert_eq!(
        lower("pub example(): Dynamic -> provider.apply."),
        CoreExpr::RemoteFunRef {
            module: "app.Provider".into(),
            function: "apply".into(),
            arity: 1,
        }
    );
}

#[test]
fn qualified_callable_values_survive_all_receiver_chain_positions() {
    let mut expression = lower("pub example(target: Dynamic): Dynamic -> target.first(provider.apply).last(provider.apply).");
    let mut references = 0;
    crate::terlan_typeck::visit_core_expr_mut(&mut expression, &mut |expression| {
        if let CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } = expression
        {
            assert_eq!(module, "app.Provider");
            assert_eq!(function, "apply");
            assert_eq!(*arity, 1);
            references += 1;
        }
    });
    assert_eq!(references, 2);
}

#[test]
fn qualified_callable_values_do_not_guess_private_missing_or_overloaded_targets() {
    for field in ["private", "missing", "many"] {
        assert!(matches!(
            lower(&format!("pub example(): Dynamic -> provider.{field}.")),
            CoreExpr::FieldAccess { .. }
        ));
    }
}

#[test]
fn qualified_callable_values_preserve_lexical_shadowing() {
    assert!(matches!(
        lower("pub example(provider: Dynamic): Dynamic -> provider.apply."),
        CoreExpr::FieldAccess { .. }
    ));
    let CoreExpr::Let { body, .. } =
        lower("pub example(): Dynamic -> let provider = 1; provider.apply.")
    else {
        panic!("let body");
    };
    assert!(matches!(*body, CoreExpr::FieldAccess { .. }));
    let CoreExpr::Lam { body, .. } =
        lower("pub example(): Dynamic -> (provider) -> provider.apply.")
    else {
        panic!("lambda body");
    };
    assert!(matches!(*body, CoreExpr::FieldAccess { .. }));
}
