//! Portable callable contracts from OTP `fun_SUITE`.

use std::collections::HashMap;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

fn native_fun_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module fun_suite_native.\n\
\n\
apply0(callback: () -> Int): Int -> callback().\n\
apply1(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\
apply2(left: Int, right: Int, callback: (Int, Int) -> Int): Int -> callback(left, right).\n\
compose(value: Int, first: (Int) -> Int, second: (Int) -> Int): Int -> second(first(value)).\n\
double(value: Int): Int -> value * 2.\n\
increment(value: Int): Int -> value + 1.\n\
\n\
pub zero_arity(): Int -> apply0(() -> 42).\n\
pub identity(value: Int): Int -> apply1(value, (item: Int) -> item).\n\
pub captured(value: Int, offset: Int): Int ->\n\
    apply1(value, (item: Int) -> item + offset).\n\
pub two_arguments(left: Int, right: Int): Int ->\n\
    apply2(left, right, (first: Int, second: Int) -> first + second).\n\
pub named(value: Int): Int -> apply1(value, double).\n\
pub composed(value: Int): Int -> compose(value, double, increment).\n\
pub repeated(value: Int, offset: Int): Int ->\n\
    let callback = ((item: Int) -> item + offset);\n\
    callback(value) + callback(value + 1).\n\
pub selected(forward: Bool, value: Int, seed: Int): Int ->\n\
    case forward {\n\
        true -> apply1(value, (item: Int) -> item + seed);\n\
        false -> apply1(value, (item: Int) -> item - seed)\n\
    }.\n",
    )
    .expect("parse fun-suite source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower callable application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

#[test]
fn fun_suite_callbacks_execute_through_linked_native_object() {
    let (modules, exports) = native_fun_module();
    let object = emit_native_application_object("fun_suite_native", &modules)
        .expect("emit callable native object");
    let cases = [
        ("zero_arity", Vec::new(), 42),
        ("identity", vec![-5], -5),
        ("captured", vec![40, 2], 42),
        ("captured", vec![40, 3], 43),
        ("two_arguments", vec![20, 22], 42),
        ("named", vec![21], 42),
        ("composed", vec![20], 41),
        ("repeated", vec![20, 1], 43),
        ("selected", vec![1, 40, 2], 42),
        ("selected", vec![0, 44, 2], 42),
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

    assert_native_object_invocations("fun-suite-native", &object, &invocations);
}

#[test]
fn fun_suite_wrong_callback_arity_is_rejected_before_native_linking() {
    let syntax = parse_module_as_syntax_output(
        "\
module fun_suite_wrong_arity.\n\
\n\
apply(value: Int, callback: (Int) -> Int): Int -> callback().\n\
pub answer(): Int -> apply(42, (item: Int) -> item).\n",
    )
    .expect("parse wrong-arity source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("function arity mismatch: expected 1 args, found 0")
        }),
        "diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn fun_suite_non_callable_argument_is_rejected_by_static_types() {
    let syntax = parse_module_as_syntax_output(
        "\
module fun_suite_non_callable.\n\
\n\
apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\
pub answer(): Int -> apply(40, 2).\n",
    )
    .expect("parse non-callable source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("expected (Int) -> Int")
                || diagnostic.message.contains("cannot unify Int with")
        }),
        "diagnostics: {diagnostics:#?}"
    );
}
