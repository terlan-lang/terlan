//! Portable direct-AOT contracts from OTP `map_SUITE` and `map_no_opt_SUITE`.

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

fn native_map_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    lower_source(
        "\
module map_suite_native.\n\
\n\
import std.collections.{List, Map}.\n\
import std.collections.Iterator.{next}.\n\
import std.collections.Map.{contains_key as map_contains, size as map_size}.\n\
import std.core.Option.{None, Some}.\n\
\n\
pub values(): Map[String, Int] ->\n\
    Map.from_entries([{\"first\", 1}, {\"second\", 2}, {\"first\", 3}]).\n\
pub empty(): Map[String, Int] -> Map.new[String, Int]().\n\
\n\
pub size_contract(): Bool -> values().size() == 2.\n\
pub empty_contract(): Bool -> empty().is_empty() and empty().size() == 0.\n\
pub duplicate_contract(): Bool ->\n\
    case values().get(\"first\") {\n\
        Some(value) -> value == 3;\n\
        None -> false\n\
    }.\n\
pub missing_contract(): Bool ->\n\
    case values().get(\"missing\") {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub contains_contract(): Bool ->\n\
    values().contains_key(\"second\") and values().contains_key(\"missing\") == false.\n\
pub pattern_contract(): Bool ->\n\
    case Map.from_entries([{\"first\", 3}]) {\n\
        {first: found} -> found == 3;\n\
        _ -> false\n\
    }.\n\
pub guarded_pattern_contract(): Bool ->\n\
    case Map.from_entries([{\"first\", 3}]) {\n\
        {first: found} where found == 3 -> true;\n\
        _ -> false\n\
    }.\n\
pub from_entries_contract(): Bool ->\n\
    map_size(Map.from_entries([{\"only\", 9}])) == 1.\n\
pub take_contract(): Bool ->\n\
    case Map.take(values(), \"first\") {\n\
        {Some(value), remaining} ->\n\
            value == 3 and values().contains_key(\"first\") and map_contains(remaining, \"first\") == false and map_size(remaining) == 1;\n\
        {None, _} -> false\n\
    }.\n\
pub take_missing_contract(): Bool ->\n\
    case Map.take(values(), \"missing\") {\n\
        {None, remaining} -> map_size(remaining) == 2;\n\
        {Some(_), _} -> false\n\
    }.\n\
pub mutation_contract(): Bool ->\n\
    let users = Map.new[String, Int]();\n\
    users.put(\"first\", 1);\n\
    users.put(\"second\", 2);\n\
    users.put(\"first\", 3);\n\
    users.remove(\"second\");\n\
    users.size() == 1 and users.contains_key(\"second\") == false.\n\
pub iterator_contract(): Bool ->\n\
    case next(Map.from_entries([{1, 10}, {2, 20}]).iterator()) {\n\
        Some({value: {key, value}, next: _rest}) -> key == 1 and value == 10;\n\
        None -> false\n\
    }.\n\
pub clear_contract(): Bool ->\n\
    let users = values();\n\
    users.clear();\n\
    users.is_empty() and users.size() == 0.\n",
    )
}

fn lower_source(source: &str) -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(source).expect("parse map-suite native source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower map application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn map_portable_contracts_execute_through_linked_native_object() {
    let (modules, exports) = native_map_module();
    let object = emit_native_application_object("map_suite_native", &modules).expect("object");
    let invocations = [
        "size_contract",
        "empty_contract",
        "duplicate_contract",
        "missing_contract",
        "contains_contract",
        "pattern_contract",
        "guarded_pattern_contract",
        "from_entries_contract",
        "take_contract",
        "take_missing_contract",
        "mutation_contract",
        "clear_contract",
        "iterator_contract",
    ]
    .into_iter()
    .map(|function| NativeObjectInvocation {
        export_id: exports[function],
        arguments: Vec::new(),
        expected_status: status::OK,
        expected_result: Some(1),
    })
    .collect::<Vec<_>>();
    assert_managed_native_object_invocations("map-suite-native", &modules, &object, &invocations);
}

#[test]
fn large_map_crosses_the_indexed_threshold_through_linked_native_object() {
    let entries = (0..160)
        .map(|value| format!("{{{value}, {}}}", value * 3))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "module map_suite_large_native.\n\n\
         import std.collections.Map.\n\
         import std.core.Option.{{None, Some}}.\n\n\
         pub values(): Map[Int, Int] -> Map.from_entries([{entries}]).\n\
         pub contract(): Bool ->\n\
             case values().get(159) {{\n\
                 Some(value) -> values().size() == 160 and value == 477;\n\
                 None -> false\n\
             }}.\n"
    );
    let (modules, exports) = lower_source(&source);
    let object =
        emit_native_application_object("map_suite_large_native", &modules).expect("large object");
    assert_managed_native_object_invocations(
        "map-suite-large-native",
        &modules,
        &object,
        &[NativeObjectInvocation {
            export_id: exports["contract"],
            arguments: Vec::new(),
            expected_status: status::OK,
            expected_result: Some(1),
        }],
    );
}

#[test]
fn map_no_opt_variant_has_one_deterministic_direct_aot_image() {
    let (modules, _) = native_map_module();
    let first = emit_native_application_object("map_suite_no_opt", &modules).expect("first object");
    let second =
        emit_native_application_object("map_suite_no_opt", &modules).expect("second object");
    assert_eq!(first, second);
}
