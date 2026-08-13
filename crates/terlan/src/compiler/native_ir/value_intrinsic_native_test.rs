//! Source-pipeline coverage for polymorphic `String(value)` AOT specialization.

use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::NativeModule;

#[test]
fn value_to_string_specializes_supported_scalar_representations() {
    let syntax = parse_module_as_syntax_output(
        r#"
module value_intrinsic_native.

pub int_value(): Bool -> String(123) == "123".

pub float_value(): Bool -> String(1.5) == "1.5".

pub bool_value(): Bool -> String(true) == "true".

pub string_value(): Bool -> String("ready") == "ready".
"#,
    )
    .expect("parse value-to-string source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let modules = NativeModule::lower_application(&[&core])
        .expect("value-to-string scalar variants must lower through direct AOT");
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();

    for expected in ["int_value", "float_value", "bool_value", "string_value"] {
        assert!(functions.contains(&expected), "missing `{expected}`");
    }
}
