//! Portable typed-guard contracts retained from OTP `guard_SUITE`.

use std::collections::HashMap;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

fn native_guard_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module guard_suite_native.\n\
\n\
pub ordered(value: Int): Int ->\n\
    case value {\n\
        item where item >= -10 and item < 0 -> 1;\n\
        item where item >= 0 and item <= 10 -> 2;\n\
        _ -> 3\n\
    }.\n\
pub guarded_division(divisor: Int): Int ->\n\
    case divisor {\n\
        value where value != 0 and 84 div value > 10 -> 1;\n\
        _ -> 0\n\
    }.\n\
pub guarded_or(value: Int): Int ->\n\
    case value {\n\
        item where item == 0 or 84 div item > 10 -> 1;\n\
        _ -> 0\n\
    }.\n\
pub overflow_safe_increment(value: Int): Int ->\n\
    case value {\n\
        item where item < 9223372036854775807 and item + 1 > item -> item + 1;\n\
        _ -> value\n\
    }.\n\
pub negated(flag: Bool): Int ->\n\
    case flag {\n\
        value where !value -> 10;\n\
        _ -> 20\n\
    }.\n\
pub ordered_fallback(value: Int): Int ->\n\
    case value {\n\
        item where item > 100 -> 1;\n\
        item where item > 10 -> 2;\n\
        item where item > 0 -> 3;\n\
        _ -> 4\n\
    }.\n",
    )
    .expect("parse guard-suite source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower guarded application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn guard_suite_conditions_execute_in_order_through_linked_native_object() {
    let (modules, exports) = native_guard_module();
    let object =
        emit_native_application_object("guard_suite_native", &modules).expect("emit guard object");
    let cases = [
        ("ordered", vec![-11], 3),
        ("ordered", vec![-10], 1),
        ("ordered", vec![-1], 1),
        ("ordered", vec![0], 2),
        ("ordered", vec![10], 2),
        ("ordered", vec![11], 3),
        ("guarded_division", vec![0], 0),
        ("guarded_division", vec![7], 1),
        ("guarded_division", vec![14], 0),
        ("guarded_or", vec![0], 1),
        ("guarded_or", vec![7], 1),
        ("guarded_or", vec![14], 0),
        ("overflow_safe_increment", vec![41], 42),
        ("overflow_safe_increment", vec![i64::MAX], i64::MAX),
        ("negated", vec![0], 10),
        ("negated", vec![1], 20),
        ("ordered_fallback", vec![101], 1),
        ("ordered_fallback", vec![11], 2),
        ("ordered_fallback", vec![1], 3),
        ("ordered_fallback", vec![0], 4),
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

    assert_native_object_invocations("guard-suite-native", &object, &invocations);
}

#[test]
fn guard_suite_dynamic_type_evidence_narrows_the_selected_branch() {
    let syntax = parse_module_as_syntax_output(
        "\
module guard_suite_type_evidence.\n\
\n\
pub to_int(value: Dynamic): Int ->\n\
    case value {\n\
        item where is_type(item, Int) -> item;\n\
        _ -> 0\n\
    }.\n",
    )
    .expect("parse type-evidence guard");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
}

#[test]
fn guard_suite_rejects_non_boolean_guards_and_operands_before_native_linking() {
    let non_boolean = parse_module_as_syntax_output(
        "\
module guard_suite_non_boolean.\n\
\n\
pub non_boolean(value: Int): Int ->\n\
    case value {\n\
        item where item -> item;\n\
        _ -> 0\n\
    }.\n",
    )
    .expect("parse non-boolean guard");
    let resolved = resolve_syntax_module_output(&non_boolean).module;
    let non_boolean_diagnostics = type_check_syntax_module_output(&non_boolean, &resolved);

    assert!(
        non_boolean_diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("case guard expected Bool found Int")),
        "diagnostics: {non_boolean_diagnostics:#?}"
    );

    let invalid_operand = parse_module_as_syntax_output(
        "\
module guard_suite_invalid_operand.\n\
\n\
pub invalid_operand(value: Int): Int ->\n\
    case value {\n\
        item where true and item -> item;\n\
        _ -> 0\n\
    }.\n",
    )
    .expect("parse invalid guard operand");
    let resolved = resolve_syntax_module_output(&invalid_operand).module;
    let operand_diagnostics = type_check_syntax_module_output(&invalid_operand, &resolved);
    assert!(
        operand_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected Bool found")),
        "diagnostics: {operand_diagnostics:#?}"
    );
}
