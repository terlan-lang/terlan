//! Portable one-image bring-up contract retained from OTP `hello_SUITE`.

use std::collections::HashMap;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

fn native_hello_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module hello_suite_native.\n\
\n\
factorial(value: Int, acc: Int): Int ->\n\
    if {\n\
        value == 0 -> acc;\n\
        true -> factorial(value - 1, acc * value)\n\
    }.\n\
\n\
apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\
\n\
pub recursion(value: Int): Int -> factorial(value, 1).\n\
\n\
pub tuple_sum(): Int -> {10, 20, 12}[0] + {10, 20, 12}[1] + {10, 20, 12}[2].\n\
\n\
pub captured(value: Int, offset: Int): Int ->\n\
    apply(value, (item: Int) -> item + offset).\n\
\n\
pub recovered(divisor: Int): Int ->\n\
    try 84 div divisor {\n\
        result -> result\n\
    catch\n\
        _reason -> 5\n\
    after\n\
        0 -> 7\n\
    }.\n\
\n\
pub classify(value: Int): Int ->\n\
    case value {\n\
        item where item < 0 -> -1;\n\
        item where item <= 10 -> 0;\n\
        _ -> 1\n\
    }.\n",
    )
    .expect("parse hello-suite source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower hello-suite application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn hello_suite_portable_families_execute_together_in_one_linked_aot_image() {
    let (modules, exports) = native_hello_module();
    let object =
        emit_native_application_object("hello_suite_native", &modules).expect("native object");
    let cases = [
        ("recursion", vec![0], 1),
        ("recursion", vec![5], 120),
        ("tuple_sum", vec![], 42),
        ("captured", vec![40, 2], 42),
        ("captured", vec![-5, 47], 42),
        ("recovered", vec![2], 42),
        ("recovered", vec![0], 5),
        ("classify", vec![-1], -1),
        ("classify", vec![0], 0),
        ("classify", vec![10], 0),
        ("classify", vec![11], 1),
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

    assert_native_object_invocations("hello-suite-native", &object, &invocations);
}
