//! Portable finite-binary64 contracts from OTP `float_SUITE`.

use std::collections::HashMap;

use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

/// Lowers the portable Float surface through source, typechecking, and NativeIR.
fn native_float_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module float_suite_native.\n\
\n\
import std.core.Float.{ceil, floor, from_string, log, pi, tau, to_string}.\n\
import type std.core.Option.\n\
\n\
pub add(left: Float, right: Float): Float -> left + right.\n\
pub subtract(left: Float, right: Float): Float -> left - right.\n\
pub negate(value: Float): Float -> -value.\n\
pub multiply(left: Float, right: Float): Float -> left * right.\n\
pub divide(left: Float, right: Float): Float -> left / right.\n\
pub mixed_multiply(left: Int, right: Float): Float -> left * right.\n\
pub equal(left: Float, right: Float): Bool -> left == right.\n\
pub not_equal(left: Float, right: Float): Bool -> left != right.\n\
pub less(left: Float, right: Float): Bool -> left < right.\n\
pub less_equal(left: Float, right: Float): Bool -> left <= right.\n\
pub greater(left: Float, right: Float): Bool -> left > right.\n\
pub greater_equal(left: Float, right: Float): Bool -> left >= right.\n\
pub mixed_less(left: Int, right: Float): Bool -> left < right.\n\
pub mixed_greater(left: Float, right: Int): Bool -> left > right.\n\
pub classify(value: Float): Int ->\n\
    case value {\n\
        1.0 -> 1;\n\
        2.0 -> 2;\n\
        1000.0 -> 3;\n\
        _ -> 0\n\
    }.\n\
pub hidden_multiply_overflow(left: Float, right: Float, mask: Float): Float ->\n\
    (left * right) * mask.\n\
pub hidden_add_overflow(left: Float, right: Float, mask: Float): Float ->\n\
    (left + right) * mask.\n\
pub hidden_division_failure(left: Float, zero: Float, mask: Float): Float ->\n\
    (left / zero) * mask.\n\
pub repeated_mul_add(value: Float, addend: Float): Float ->\n\
    ((((((0.0 * value + addend) * value + addend) * value + addend)\n\
        * value + addend) * value + addend) * value + addend).\n\
pub floor_value(value: Float): Float -> floor(value).\n\
pub ceil_value(value: Float): Float -> ceil(value).\n\
pub log_value(value: Float): Float -> log(value).\n\
pub pi_value(): Float -> pi().\n\
pub tau_value(): Float -> tau().\n\
pub renders_float(): Bool -> to_string(-2.25) == \"-2.25\".\n\
pub parse_float(value: String): Option[Float] -> from_string(value).\n",
    )
    .expect("parse float-suite native source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower Float application");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

/// Adds one exact Float-bit success to the invocation table.
fn float_success(
    invocations: &mut Vec<NativeObjectInvocation>,
    exports: &HashMap<String, u64>,
    function: &str,
    arguments: &[i64],
    expected: f64,
) {
    invocations.push(NativeObjectInvocation {
        export_id: exports[function],
        arguments: arguments.to_vec(),
        expected_status: status::OK,
        expected_result: Some(expected.to_bits() as i64),
    });
}

/// Adds one exact Bool or Int success to the invocation table.
fn scalar_success(
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

/// Adds one typed Float failure to the invocation table.
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

/// Encodes one finite Float as the direct-AOT scalar boundary word.
fn bits(value: f64) -> i64 {
    value.to_bits() as i64
}

/// Executes finite arithmetic, signed zero, subnormals, and mixed promotion.
#[test]
fn float_suite_finite_arithmetic_executes_through_linked_native_object() {
    let (modules, exports) = native_float_module();
    let object =
        emit_native_application_object("float_suite_arithmetic", &modules).expect("native object");
    let mut invocations = Vec::new();

    for (left, right) in [
        (0.0, 0.0),
        (1.5, 2.25),
        (-1.5, 2.25),
        (f64::MIN_POSITIVE, f64::MIN_POSITIVE),
        (f64::from_bits(1), 0.0),
        (f64::MAX / 2.0, f64::MAX / 2.0),
    ] {
        float_success(
            &mut invocations,
            &exports,
            "add",
            &[bits(left), bits(right)],
            left + right,
        );
        float_success(
            &mut invocations,
            &exports,
            "subtract",
            &[bits(left), bits(right)],
            left - right,
        );
    }
    for (left, right) in [
        (0.0, f64::MAX),
        (-1.0, 0.0),
        (-1.0, -0.0),
        (1.5, 2.25),
        (f64::from_bits(1), 1.0),
    ] {
        float_success(
            &mut invocations,
            &exports,
            "multiply",
            &[bits(left), bits(right)],
            left * right,
        );
    }
    for (left, right) in [
        (7.5, 2.5),
        (-7.5, 2.5),
        (f64::MIN_POSITIVE, 2.0),
        (f64::from_bits(1), 2.0),
    ] {
        float_success(
            &mut invocations,
            &exports,
            "divide",
            &[bits(left), bits(right)],
            left / right,
        );
    }
    for value in [0.0, -0.0, 1.5, -1.5, f64::from_bits(1), f64::MAX] {
        float_success(&mut invocations, &exports, "negate", &[bits(value)], -value);
    }
    for (integer, float) in [(-1, 0.0), (-1, -0.0), (2, 1.5), (i64::MAX, 0.0)] {
        float_success(
            &mut invocations,
            &exports,
            "mixed_multiply",
            &[integer, bits(float)],
            integer as f64 * float,
        );
    }
    for (function, argument, expected) in
        [("floor_value", -42.1, -43.0), ("ceil_value", -42.1, -42.0)]
    {
        float_success(
            &mut invocations,
            &exports,
            function,
            &[bits(argument)],
            expected,
        );
    }
    float_success(
        &mut invocations,
        &exports,
        "pi_value",
        &[],
        std::f64::consts::PI,
    );
    float_success(
        &mut invocations,
        &exports,
        "tau_value",
        &[],
        std::f64::consts::TAU,
    );
    assert_native_object_invocations("float-suite-arithmetic", &object, &invocations);
}

/// Executes ordered comparisons, numeric Float patterns, and fallback behavior.
#[test]
fn float_suite_comparison_and_patterns_execute_natively() {
    let (modules, exports) = native_float_module();
    let object =
        emit_native_application_object("float_suite_comparison", &modules).expect("native object");
    let mut invocations = Vec::new();

    for (function, left, right, expected) in [
        ("equal", 0.0, -0.0, 1),
        ("equal", 1.5, 1.5, 1),
        ("equal", 1.5, 2.5, 0),
        ("not_equal", 1.5, 2.5, 1),
        ("less", f64::from_bits(1), 0.0, 0),
        ("less", 0.0, f64::from_bits(1), 1),
        ("less_equal", 2.0, 2.0, 1),
        ("greater", f64::MAX, f64::MAX / 2.0, 1),
        ("greater_equal", -0.0, 0.0, 1),
    ] {
        scalar_success(
            &mut invocations,
            &exports,
            function,
            &[bits(left), bits(right)],
            expected,
        );
    }
    scalar_success(&mut invocations, &exports, "mixed_less", &[1, bits(1.5)], 1);
    scalar_success(
        &mut invocations,
        &exports,
        "mixed_less",
        &[i64::MAX, bits(2_f64.powi(63))],
        0,
    );
    scalar_success(
        &mut invocations,
        &exports,
        "mixed_greater",
        &[bits(2.5), 2],
        1,
    );
    for (value, expected) in [(1.0, 1), (2.0, 2), (1000.0, 3), (0.5, 0)] {
        scalar_success(
            &mut invocations,
            &exports,
            "classify",
            &[bits(value)],
            expected,
        );
    }

    assert_native_object_invocations("float-suite-comparison", &object, &invocations);
}

/// Proves every non-finite intermediate fails and later valid calls recover.
#[test]
fn float_suite_non_finite_intermediates_fail_without_poisoning_later_calls() {
    let (modules, exports) = native_float_module();
    let object =
        emit_native_application_object("float_suite_failures", &modules).expect("native object");
    let mut invocations = Vec::new();

    failure(
        &mut invocations,
        &exports,
        "multiply",
        &[bits(f64::MAX), bits(2.0)],
        status::FLOAT_OVERFLOW,
    );
    float_success(
        &mut invocations,
        &exports,
        "add",
        &[bits(1.0), bits(2.0)],
        3.0,
    );
    failure(
        &mut invocations,
        &exports,
        "divide",
        &[bits(5.0), bits(0.0)],
        status::FLOAT_DIVISION_BY_ZERO,
    );
    failure(
        &mut invocations,
        &exports,
        "divide",
        &[bits(5.0), bits(-0.0)],
        status::FLOAT_DIVISION_BY_ZERO,
    );
    float_success(
        &mut invocations,
        &exports,
        "multiply",
        &[bits(3.0), bits(4.0)],
        12.0,
    );
    for function in [
        "hidden_multiply_overflow",
        "hidden_add_overflow",
        "hidden_division_failure",
    ] {
        let arguments = if function == "hidden_add_overflow" {
            [bits(f64::MAX), bits(f64::MAX), bits(0.0)]
        } else if function == "hidden_division_failure" {
            [bits(1.0), bits(0.0), bits(0.0)]
        } else {
            [bits(f64::MAX), bits(2.0), bits(0.0)]
        };
        failure(
            &mut invocations,
            &exports,
            function,
            &arguments,
            if function == "hidden_division_failure" {
                status::FLOAT_DIVISION_BY_ZERO
            } else {
                status::FLOAT_OVERFLOW
            },
        );
    }
    float_success(
        &mut invocations,
        &exports,
        "repeated_mul_add",
        &[bits(2.03), bits(1.3)],
        87.06260151458997,
    );

    assert_native_object_invocations("float-suite-failures", &object, &invocations);
}
