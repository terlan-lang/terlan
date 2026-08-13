//! Portable direct-AOT contracts for the complete persistent `Set` surface.

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

fn native_set_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    lower_source(
        "\
module set_suite_native.\n\
\n\
import std.collections.{List, Set}.\n\
import std.collections.Iterator.{next}.\n\
import std.core.Option.{None, Some}.\n\
\n\
pub values(): Set[Int] -> Set.from_list([1, 1, 2]).\n\
pub empty(): Set[Int] -> Set.new[Int]().\n\
\n\
pub size_contract(): Bool -> values().size() == 2.\n\
pub empty_contract(): Bool -> empty().is_empty() and empty().size() == 0.\n\
pub contains_contract(): Bool ->\n\
    values().contains(2) and values().contains(9) == false.\n\
pub mutation_contract(): Bool ->\n\
    let items = empty();\n\
    items.add(1);\n\
    items.add(2);\n\
    items.add(2);\n\
    items.remove(1);\n\
    items.size() == 1 and items.contains(2) and items.contains(1) == false.\n\
pub clear_contract(): Bool ->\n\
    let items = values();\n\
    items.clear();\n\
    items.is_empty() and items.size() == 0.\n\
pub iterator_contract(): Bool ->\n\
    case next(values().iterator()) {\n\
        Some({value: value, next: _rest}) -> value == 1;\n\
        None -> false\n\
    }.\n\
pub implicit_new_contract(): Bool ->\n\
    let items = Set.new();\n\
    items.is_empty().\n\
pub positional_constructor_contract(): Bool ->\n\
    let items = Set(1, 1, 2);\n\
    items.size() == 2 and items.contains(2).\n",
    )
}

fn lower_source(source: &str) -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(source).expect("parse set-suite native source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower set application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn set_portable_contracts_execute_through_linked_native_object() {
    let (modules, exports) = native_set_module();
    let object = emit_native_application_object("set_suite_native", &modules).expect("object");
    let invocations = [
        "size_contract",
        "empty_contract",
        "contains_contract",
        "mutation_contract",
        "clear_contract",
        "iterator_contract",
        "implicit_new_contract",
        "positional_constructor_contract",
    ]
    .into_iter()
    .map(|function| NativeObjectInvocation {
        export_id: exports[function],
        arguments: Vec::new(),
        expected_status: status::OK,
        expected_result: Some(1),
    })
    .collect::<Vec<_>>();
    assert_managed_native_object_invocations("set-suite-native", &modules, &object, &invocations);
}
