//! Optimizer-independent guard contracts from OTP `guard_no_opt_SUITE`.

use std::collections::HashMap;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{
    emit_native_application_object_with_policy, status, NativeCodegenPolicy, NativeModule,
};

fn guarded_runtime_input_module() -> (Vec<NativeModule>, HashMap<String, u64>) {
    let syntax = parse_module_as_syntax_output(
        "\
module guard_no_opt_suite_native.\n\
\n\
identity(value: Int): Int -> value.\n\
pub guarded_division(value: Int): Int ->\n\
    case identity(value) {\n\
        item where item != 0 and 84 div item > 10 -> 1;\n\
        _ -> 0\n\
    }.\n\
pub guarded_overflow(value: Int): Int ->\n\
    case identity(value) {\n\
        item where item < 9223372036854775807 and item + 1 > item -> item + 1;\n\
        _ -> value\n\
    }.\n\
pub ordered(value: Int): Int ->\n\
    case identity(value) {\n\
        item where item > 100 -> 1;\n\
        item where item > 10 -> 2;\n\
        item where item > 0 -> 3;\n\
        _ -> 4\n\
    }.\n\
pub negated_or(value: Int, enabled: Bool): Int ->\n\
    case identity(value) {\n\
        item where !enabled or item > 10 -> item;\n\
        _ -> 0\n\
    }.\n",
    )
    .expect("parse no-opt guard source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower no-opt guard module");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.clone(), function.export_id))
        .collect();
    (modules, exports)
}

fn invocations(exports: &HashMap<String, u64>) -> Vec<NativeObjectInvocation> {
    [
        ("guarded_division", vec![0], 0),
        ("guarded_division", vec![7], 1),
        ("guarded_division", vec![14], 0),
        ("guarded_overflow", vec![41], 42),
        ("guarded_overflow", vec![i64::MAX], i64::MAX),
        ("ordered", vec![101], 1),
        ("ordered", vec![11], 2),
        ("ordered", vec![1], 3),
        ("ordered", vec![0], 4),
        ("negated_or", vec![7, 0], 7),
        ("negated_or", vec![7, 1], 0),
        ("negated_or", vec![11, 1], 11),
    ]
    .into_iter()
    .map(|(function, arguments, expected)| NativeObjectInvocation {
        export_id: exports[function],
        arguments,
        expected_status: status::OK,
        expected_result: Some(expected),
    })
    .collect()
}

#[test]
fn guard_no_opt_suite_matches_release_for_runtime_dependent_conditions() {
    let (modules, exports) = guarded_runtime_input_module();
    let cases = invocations(&exports);

    for (label, policy) in [
        ("development-none", NativeCodegenPolicy::Development),
        ("release-speed", NativeCodegenPolicy::Release),
    ] {
        let object = emit_native_application_object_with_policy(
            &format!("guard_no_opt_{label}"),
            &modules,
            policy,
        )
        .unwrap_or_else(|error| panic!("emit {label} guard object: {error}"));
        assert_native_object_invocations(label, &object, &cases);
    }
}
