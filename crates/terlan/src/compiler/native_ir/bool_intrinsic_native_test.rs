//! Source-pipeline coverage for direct-AOT Boolean rendering.

use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::NativeModule;

#[test]
fn bool_to_string_lowers_composed_scalar_boolean_through_native_ir() {
    let syntax = parse_module_as_syntax_output(
        r#"
module bool_intrinsic_native.

import std.core.Bool.{to_string}.

pub render_composed(): String ->
    to_string("abc".contains("b") and "abc".length() == 3).

pub empty_string_contract(): Bool -> "".is_empty().

pub nonempty_string_contract(): Bool -> not "abc".is_empty().
"#,
    )
    .expect("parse Boolean intrinsic source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let modules = NativeModule::lower_application(&[&core])
        .expect("composed Boolean rendering must lower through direct AOT");

    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "render_composed"));
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "empty_string_contract"));
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "nonempty_string_contract"));
}
