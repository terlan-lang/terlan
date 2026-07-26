//! Terlan-native direct-AOT replacements for portable `match_spec_SUITE` behavior.

use std::collections::HashMap;

use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status, NativeModule};

fn native_match_selection_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let source = "\
module match_spec_suite_native.\n\
\n\
pub repeated_binding(left: Int, right: Int): Bool ->\n\
    left == right.\n\
pub guarded_selection(value: Int): Int ->\n\
    case value {\n\
        item where item > 0 and item < 10 -> item;\n\
        _ -> -1\n\
    }.\n\
pub short_circuit_or(value: Int): Bool ->\n\
    value == 0 or 84 div value > 10.\n\
pub short_circuit_and(value: Int): Bool ->\n\
    value != 0 and 84 div value > 10.\n\
pub unary_minus(value: Int): Int -> -value.\n\
pub moved_boolean_targets(left: Bool, right: Bool): Bool ->\n\
    (left and right) == false and (left or right) == true.\n";
    lower_source(source, "match-spec replacement")
}

fn native_collection_selection_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let source = "\
module match_spec_collection_native.\n\
\n\
pub select_values(values: List[Int]): List[Int] ->\n\
    [value + 10 | value <- values, value rem 2 == 0].\n";
    lower_source(source, "typed collection selection")
}

fn lower_source(source: &str, label: &str) -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(source)
        .unwrap_or_else(|error| panic!("parse {label}: {error:?}"));
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .unwrap_or_else(|error| panic!("lower {label}: {error}"));
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn portable_match_selection_executes_through_one_linked_native_object() {
    let (modules, exports) = native_match_selection_module();
    let object = emit_native_application_object("match_spec_suite_native", &modules)
        .expect("emit match-spec replacement object");
    let cases = [
        ("repeated_binding", vec![5, 5], 1),
        ("repeated_binding", vec![5, 6], 0),
        ("guarded_selection", vec![5], 5),
        ("guarded_selection", vec![10], -1),
        ("short_circuit_or", vec![0], 1),
        ("short_circuit_or", vec![14], 0),
        ("short_circuit_and", vec![0], 0),
        ("short_circuit_and", vec![7], 1),
        ("unary_minus", vec![5], -5),
        ("moved_boolean_targets", vec![1, 0], 1),
    ];
    let invocations = cases
        .into_iter()
        .map(|(function, arguments, expected)| NativeObjectInvocation {
            export_id: exports[function],
            arguments,
            expected_status: status::OK,
            expected_result: Some(expected),
        })
        .collect::<Vec<_>>();

    assert_managed_native_object_invocations(
        "match-spec-suite-native",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn typed_collection_selection_is_compiled_into_the_native_image() {
    let (modules, _) = native_collection_selection_module();
    let object = emit_native_application_object("match_spec_collection_native", &modules)
        .expect("emit typed collection selection object");
    assert!(!object.is_empty());
    assert!(
        modules
            .iter()
            .flat_map(|module| &module.functions)
            .any(|function| function.name.starts_with("$aot_comprehension_")),
        "typed selection must lower to an image-private AOT helper"
    );
}

#[test]
fn dynamic_match_spec_language_is_not_admitted_as_an_aot_runtime_path() {
    let source = "\
module match_spec_dynamic_rejected.\n\
\n\
pub run(spec: Dynamic, value: Dynamic): Dynamic ->\n\
    match_spec_run(spec, value).\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse rejected dynamic match spec");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let error = NativeModule::lower_application(&[&core])
        .expect_err("dynamic match-spec call must fail before native linking");

    assert!(
        error.contains("match_spec_run"),
        "unexpected native admission error: {error}"
    );
}
