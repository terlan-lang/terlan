//! Tests for bounded private higher-order helper specialization.

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{lower_syntax_module_output_to_core, CoreModule},
};

use super::{higher_order_specialization::specialize_higher_order_helpers, NativeModule};

/// Lowers canonical source into CoreIR for specialization tests.
fn core(source: &str) -> CoreModule {
    let module = parse_module_as_syntax_output(source).expect("parse higher-order source");
    let resolved = resolve_syntax_module_output(&module).module;
    lower_syntax_module_output_to_core(&module, &resolved)
}

/// Verifies a private helper accepting a captured lambda disappears before
/// NativeIR admission.
#[test]
fn private_lambda_helper_is_specialized_and_removed() {
    let core = core(
        "module higher_order_lambda.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n\
         pub answer(): Int ->\n\
             let offset = 2;\n\
             apply(40, ((value: Int) -> value + offset)).\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("specialized native module");

    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "answer"));
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .all(|function| function.name != "apply"));
}

/// Verifies a named local function value becomes a direct native call after
/// private helper specialization.
#[test]
fn private_helper_accepts_named_local_function() {
    let core = core(
        "module higher_order_named.\n\n\
         double(value: Int): Int -> value * 2.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n\
         pub answer(): Int -> apply(21, double).\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("named callback module");

    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "double"));
}

/// Verifies an externally reachable function-value ABI survives specialization
/// and lowers through the runtime-owned closure resolver.
#[test]
fn public_higher_order_export_uses_owned_closure_abi() {
    let core = core(
        "module higher_order_public.\n\n\
         pub apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n",
    );
    let modules = NativeModule::lower_application(&[&core]).expect("public closure ABI");
    let apply = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "apply")
        .expect("public apply entry");
    assert!(matches!(
        apply.body,
        super::NativeExpr::InvokeClosure { .. }
    ));
}

/// Verifies recursive template expansion is rejected instead of exhausting
/// compiler stack or code size.
#[test]
fn recursive_higher_order_specialization_is_rejected() {
    let mut core = core(
        "module higher_order_recursive.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> apply(value, callback).\n\n\
         pub answer(): Int -> apply(1, ((value: Int) -> value)).\n",
    );

    assert_eq!(
        specialize_higher_order_helpers(&mut core).unwrap_err(),
        "error[native_ir.higher_order_recursion]: `apply/2` recursively requires higher-order specialization"
    );
}

#[test]
fn higher_order_specialization_budget_fails_before_native_linking() {
    let mut source = String::from(
        "module higher_order_budget.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n",
    );
    for index in 0..=128 {
        source.push_str(&format!(
            "pub run_{index}(value: Int): Int ->\n\
                 apply(value, ((item: Int) -> item + {index})).\n\n"
        ));
    }
    let mut core = core(&source);
    let error =
        specialize_higher_order_helpers(&mut core).expect_err("reject specialization explosion");

    assert_eq!(
        error,
        "error[native_ir.specialization_limit]: higher-order specialization exceeds 128 calls"
    );
}

#[test]
fn recursive_higher_order_helpers_receive_distinct_finite_callsite_contexts() {
    let mut core = core(
        "module higher_order_context.\n\n\
         apply(value: Int, callback: (Int) -> Int): Int -> apply(value, callback).\n\n\
         pub first(): Int -> apply(1, ((value: Int) -> value + 1)).\n\n\
         pub second(): Int -> apply(2, ((value: Int) -> value + 2)).\n",
    );
    let mut budget = super::specialization_budget::SpecializationBudget::default();

    super::higher_order_context::specialize_higher_order_contexts(&mut core, &mut budget)
        .expect("specialize recursive callsite contexts");

    let entry_target = |name: &str| {
        let function = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("entry function");
        let crate::terlan_typeck::CoreExpr::Call { function, .. } = function.clauses[0]
            .body
            .core_expr
            .as_ref()
            .expect("entry body")
        else {
            panic!("entry must remain a direct call");
        };
        function.clone()
    };
    let first = entry_target("first");
    let second = entry_target("second");
    assert_ne!(first, second);
    assert!(first.starts_with("$aot_hof_context_apply_2_"));
    assert!(second.starts_with("$aot_hof_context_apply_2_"));

    for clone_name in [first, second] {
        let clone = core
            .functions
            .iter()
            .find(|function| function.name == clone_name)
            .expect("context clone");
        let crate::terlan_typeck::CoreExpr::Call { function, .. } = clone.clauses[0]
            .body
            .core_expr
            .as_ref()
            .expect("clone body")
        else {
            panic!("recursive clone body must remain a direct call");
        };
        assert_eq!(function, &clone_name);
    }
}
