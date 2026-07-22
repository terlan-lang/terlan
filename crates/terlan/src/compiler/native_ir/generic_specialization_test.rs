//! Bounded generic monomorphization checks for the native application closure.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::NativeModule;

#[test]
fn private_generic_helper_is_replaced_by_concrete_native_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module generic_native.\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(value: Int): Int -> identity(value).\n",
    )
    .expect("parse generic source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("generic NativeIR");
    let names = modules[0]
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert!(!names.contains(&"identity"));
    assert!(names
        .iter()
        .any(|name| name.starts_with("$aot_generic_identity_")));
    assert!(names.contains(&"run"));
}

#[test]
fn public_generic_export_has_stable_prelink_rejection() {
    let syntax = parse_module_as_syntax_output(
        "module generic_export.\n\npub identity[T](value: T): T -> value.\n",
    )
    .expect("parse generic export");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let error = NativeModule::lower_application(&[&core]).expect_err("reject generic export");
    assert!(
        error.starts_with("error[native_ir.generic_export]"),
        "{error}"
    );
}

/// Verifies local nominal types are not inferred as undeclared type variables.
#[test]
fn public_nominal_export_is_not_misclassified_as_generic() {
    let syntax = parse_module_as_syntax_output(
        "module nominal_export.\n\n\
         pub struct Pair { left: Int, right: Int }.\n\n\
         pub identity(value: Pair): Pair -> value.\n",
    )
    .expect("parse nominal export");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower nominal export");

    assert!(modules[0]
        .functions
        .iter()
        .any(|function| function.name == "identity"));
}

#[test]
fn generic_specialization_budget_fails_before_native_linking() {
    let mut source =
        String::from("module generic_budget.\n\nidentity[T](value: T): T -> value.\n\n");
    for index in 0..=128 {
        source.push_str(&format!(
            "pub struct Value{index} {{ value: Int }}.\n\n\
             pub use_{index}(value: Value{index}): Value{index} -> identity(value).\n\n"
        ));
    }
    let syntax = parse_module_as_syntax_output(&source).expect("parse generic budget source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let error = NativeModule::lower_application(&[&core]).expect_err("reject generic explosion");

    assert!(
        error.starts_with("error[native_ir.generic_budget]"),
        "{error}"
    );
}
