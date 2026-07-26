//! Portable typed contracts from OTP `list_bif_SUITE`.

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

fn native_list_bif_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module list_bif_suite_native.\n\
\n\
import std.collections.List.{first as list_first, is_empty as list_is_empty, length as list_length, rest as list_rest}.\n\
import std.core.Float.{from_string as float_from_string}.\n\
import std.core.Int.{from_string as int_from_string, from_string_base as int_from_string_base, to_string as int_to_string, to_string_base as int_to_string_base}.\n\
import std.core.Option.{None, Some}.\n\
\n\
import type std.core.Option.\n\
\n\
pub values(): List[Int] -> [104, 101, 106, 115, 97, 110].\n\
pub empty(): List[Int] -> [].\n\
pub singleton(): List[Int] -> [1].\n\
\n\
pub list_length_contract(): Bool -> list_length(values()) == 6.\n\
pub list_head_contract(): Bool ->\n\
    case list_first(values()) {\n\
        Some(value) -> value == 104;\n\
        None -> false\n\
    }.\n\
pub list_tail_contract(): Bool ->\n\
    case list_rest(values()) {\n\
        Some(tail) -> list_length(tail) == 5;\n\
        None -> false\n\
    }.\n\
pub empty_first_contract(): Bool ->\n\
    case list_first(empty()) {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub empty_rest_contract(): Bool ->\n\
    case list_rest(empty()) {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub empty_is_empty_contract(): Bool -> list_is_empty(empty()).\n\
pub singleton_rest_contract(): Bool ->\n\
    case list_rest(singleton()) {\n\
        Some(tail) -> list_is_empty(tail);\n\
        None -> false\n\
    }.\n\
\n\
pub decimal_parse_contract(): Bool ->\n\
    case int_from_string(\"12373\") {\n\
        Some(value) -> value == 12373;\n\
        None -> false\n\
    }.\n\
pub negative_parse_contract(): Bool ->\n\
    case int_from_string(\"-12373\") {\n\
        Some(value) -> value == -12373;\n\
        None -> false\n\
    }.\n\
pub positive_sign_parse_contract(): Bool ->\n\
    case int_from_string(\"+12373\") {\n\
        Some(value) -> value == 12373;\n\
        None -> false\n\
    }.\n\
pub trailing_decimal_contract(): Bool ->\n\
    case int_from_string(\"12373ABC\") {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub empty_decimal_contract(): Bool ->\n\
    case int_from_string(\"\") {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub overflow_contract(): Bool ->\n\
    case int_from_string(\"9223372036854775808\") {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub radix_parse_contract(): Bool ->\n\
    case int_from_string_base(\"ff\", 16) {\n\
        Some(value) -> value == 255;\n\
        None -> false\n\
    }.\n\
pub invalid_radix_contract(): Bool ->\n\
    case int_from_string_base(\"2\", 2) {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub invalid_base_contract(): Bool ->\n\
    case int_from_string_base(\"10\", 1) {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n\
pub decimal_render_contract(): Bool ->\n\
    case int_from_string(int_to_string(-12373)) {\n\
        Some(value) -> value == -12373;\n\
        None -> false\n\
    }.\n\
pub radix_render_value(): Option[String] -> int_to_string_base(255, 16).\n\
\n\
pub float_parse_contract(): Bool ->\n\
    case float_from_string(\"5.89898\") {\n\
        Some(value) -> value == 5.89898;\n\
        None -> false\n\
    }.\n\
pub invalid_float_contract(): Bool ->\n\
    case float_from_string(\"5.89tail\") {\n\
        Some(_) -> false;\n\
        None -> true\n\
    }.\n",
    )
    .expect("parse list-BIF native source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower list-BIF application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn list_bif_portable_contracts_execute_through_linked_native_object() {
    let (modules, exports) = native_list_bif_module();
    let object =
        emit_native_application_object("list_bif_suite_native", &modules).expect("native object");
    let functions = [
        "list_length_contract",
        "list_head_contract",
        "list_tail_contract",
        "empty_first_contract",
        "empty_rest_contract",
        "empty_is_empty_contract",
        "singleton_rest_contract",
        "decimal_parse_contract",
        "negative_parse_contract",
        "positive_sign_parse_contract",
        "trailing_decimal_contract",
        "empty_decimal_contract",
        "overflow_contract",
        "radix_parse_contract",
        "invalid_radix_contract",
        "invalid_base_contract",
        "decimal_render_contract",
        "float_parse_contract",
        "invalid_float_contract",
    ];
    let invocations = functions
        .into_iter()
        .map(|function| NativeObjectInvocation {
            export_id: exports[function],
            arguments: Vec::new(),
            expected_status: status::OK,
            expected_result: Some(1),
        })
        .collect::<Vec<_>>();
    assert_managed_native_object_invocations(
        "list-bif-suite-native",
        &modules,
        &object,
        &invocations,
    );
}
