//! Portable bounded-`Int` and tuple-index contracts from OTP `small_SUITE`.

use std::collections::HashMap;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

/// Lowers the portable small-integer surface through the real source pipeline.
fn native_small_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module small_suite_native.\n\
\n\
pub add(left: Int, right: Int): Int -> left + right.\n\
pub subtract(left: Int, right: Int): Int -> left - right.\n\
pub negate(value: Int): Int -> -value.\n\
pub multiply(left: Int, right: Int): Int -> left * right.\n\
pub mul_add(left: Int, right: Int, addend: Int): Int ->\n\
    left * right + addend.\n\
pub divide(left: Int, right: Int): Int -> left div right.\n\
pub remainder(left: Int, right: Int): Int -> left rem right.\n\
pub reconstruct(left: Int, right: Int): Int ->\n\
    (left div right) * right + (left rem right).\n\
pub square_twice(value: Int): Int -> value * value + value * value.\n\
pub bounded_increment(value: Int): Int ->\n\
    if {\n\
        value < 9223372036854775807 -> value + 1;\n\
        true -> value\n\
    }.\n\
pub tuple_first(): Int -> {10, 20, 30}[0].\n\
pub tuple_middle(): Int -> {10, 20, 30}[1].\n\
pub tuple_last(): Int -> {10, 20, 30}[2].\n\
pub tuple_sum(): Int ->\n\
    let values = {10, 20, 30};\n\
    values[0] + values[1] + values[2].\n",
    )
    .expect("parse small-suite native source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower small-suite application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

/// Adds one exact scalar success to the linked-object invocation table.
fn success(
    invocations: &mut Vec<NativeObjectInvocation>,
    exports: &HashMap<String, u64>,
    function: &str,
    arguments: &[i64],
    expected: i64,
) {
    invocations.push(NativeObjectInvocation {
        export_id: exports[function],
        arguments: arguments.to_vec(),
        expected_status: status::OK,
        expected_result: Some(expected),
    });
}

/// Adds one exact typed failure to the linked-object invocation table.
fn failure(
    invocations: &mut Vec<NativeObjectInvocation>,
    exports: &HashMap<String, u64>,
    function: &str,
    arguments: &[i64],
    expected_status: i32,
) {
    invocations.push(NativeObjectInvocation {
        export_id: exports[function],
        arguments: arguments.to_vec(),
        expected_status,
        expected_result: None,
    });
}

/// Executes all portable arithmetic families at signed 64-bit boundaries.
#[test]
fn small_suite_checked_integer_families_execute_through_linked_native_object() {
    let (modules, exports) = native_small_module();
    let object =
        emit_native_application_object("small_suite_native", &modules).expect("native object");
    let mut invocations = Vec::new();

    for (left, right, expected) in [
        (0, 0, 0),
        (20, 22, 42),
        (-20, 22, 2),
        (i64::MAX - 1, 1, i64::MAX),
        (i64::MIN + 1, -1, i64::MIN),
    ] {
        success(&mut invocations, &exports, "add", &[left, right], expected);
        success(&mut invocations, &exports, "add", &[right, left], expected);
    }
    failure(
        &mut invocations,
        &exports,
        "add",
        &[i64::MAX, 1],
        status::OVERFLOW,
    );
    failure(
        &mut invocations,
        &exports,
        "add",
        &[i64::MIN, -1],
        status::OVERFLOW,
    );

    for (left, right, expected) in [
        (42, 20, 22),
        (-20, 22, -42),
        (i64::MAX, 1, i64::MAX - 1),
        (i64::MIN + 1, 1, i64::MIN),
    ] {
        success(
            &mut invocations,
            &exports,
            "subtract",
            &[left, right],
            expected,
        );
    }
    failure(
        &mut invocations,
        &exports,
        "subtract",
        &[i64::MAX, -1],
        status::OVERFLOW,
    );
    failure(
        &mut invocations,
        &exports,
        "subtract",
        &[i64::MIN, 1],
        status::OVERFLOW,
    );

    for (value, expected) in [
        (0, 0),
        (1, -1),
        (-1, 1),
        (i64::MAX, -i64::MAX),
        (i64::MIN + 1, i64::MAX),
    ] {
        success(&mut invocations, &exports, "negate", &[value], expected);
    }
    failure(
        &mut invocations,
        &exports,
        "negate",
        &[i64::MIN],
        status::OVERFLOW,
    );

    for (left, right, expected) in [
        (0, i64::MAX, 0),
        (1, i64::MAX, i64::MAX),
        (-1, i64::MAX, -i64::MAX),
        (3_037_000_499, 3_037_000_499, 9_223_372_030_926_249_001),
        (i64::MIN, 1, i64::MIN),
    ] {
        success(
            &mut invocations,
            &exports,
            "multiply",
            &[left, right],
            expected,
        );
        success(
            &mut invocations,
            &exports,
            "multiply",
            &[right, left],
            expected,
        );
    }
    failure(
        &mut invocations,
        &exports,
        "multiply",
        &[3_037_000_500, 3_037_000_500],
        status::OVERFLOW,
    );
    failure(
        &mut invocations,
        &exports,
        "multiply",
        &[i64::MIN, -1],
        status::OVERFLOW,
    );

    success(&mut invocations, &exports, "mul_add", &[3, 4, 5], 17);
    success(
        &mut invocations,
        &exports,
        "mul_add",
        &[i64::MAX / 2, 2, 1],
        i64::MAX,
    );
    failure(
        &mut invocations,
        &exports,
        "mul_add",
        &[i64::MAX, 2, i64::MIN],
        status::OVERFLOW,
    );
    failure(
        &mut invocations,
        &exports,
        "mul_add",
        &[i64::MAX, 1, 1],
        status::OVERFLOW,
    );

    success(&mut invocations, &exports, "square_twice", &[0], 0);
    success(&mut invocations, &exports, "square_twice", &[1], 2);
    success(&mut invocations, &exports, "square_twice", &[2], 8);
    failure(
        &mut invocations,
        &exports,
        "square_twice",
        &[i64::MAX],
        status::OVERFLOW,
    );

    assert_native_object_invocations("small-suite-arithmetic", &object, &invocations);
}

/// Executes signed division, remainder, reconstruction, and failure contracts.
#[test]
fn small_suite_division_reconstructs_operands_and_reports_typed_failures() {
    let (modules, exports) = native_small_module();
    let object =
        emit_native_application_object("small_suite_native_division", &modules).expect("object");
    let mut invocations = Vec::new();

    for (left, right, quotient, remainder) in [
        (7, 3, 2, 1),
        (-7, 3, -2, -1),
        (7, -3, -2, 1),
        (-7, -3, 2, -1),
        (i64::MAX, 1, i64::MAX, 0),
        (i64::MIN, 1, i64::MIN, 0),
        (i64::MIN, 2, i64::MIN / 2, 0),
    ] {
        success(
            &mut invocations,
            &exports,
            "divide",
            &[left, right],
            quotient,
        );
        success(
            &mut invocations,
            &exports,
            "remainder",
            &[left, right],
            remainder,
        );
        success(
            &mut invocations,
            &exports,
            "reconstruct",
            &[left, right],
            left,
        );
    }
    for function in ["divide", "remainder", "reconstruct"] {
        failure(
            &mut invocations,
            &exports,
            function,
            &[42, 0],
            status::DIVISION_BY_ZERO,
        );
        failure(
            &mut invocations,
            &exports,
            function,
            &[i64::MIN, -1],
            status::OVERFLOW,
        );
    }

    assert_native_object_invocations("small-suite-division", &object, &invocations);
}

/// Executes range-sensitive arithmetic and every valid fixed tuple index.
#[test]
fn small_suite_range_and_tuple_index_contracts_execute_natively() {
    let (modules, exports) = native_small_module();
    let object =
        emit_native_application_object("small_suite_native_ranges", &modules).expect("object");
    let mut invocations = Vec::new();

    for (value, expected) in [
        (i64::MIN, i64::MIN + 1),
        (-1, 0),
        (0, 1),
        (i64::MAX - 1, i64::MAX),
        (i64::MAX, i64::MAX),
    ] {
        success(
            &mut invocations,
            &exports,
            "bounded_increment",
            &[value],
            expected,
        );
    }
    for (function, expected) in [
        ("tuple_first", 10),
        ("tuple_middle", 20),
        ("tuple_last", 30),
        ("tuple_sum", 60),
    ] {
        success(&mut invocations, &exports, function, &[], expected);
    }

    assert_native_object_invocations("small-suite-ranges", &object, &invocations);
}
