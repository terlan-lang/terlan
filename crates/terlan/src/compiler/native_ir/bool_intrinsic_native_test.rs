//! Source-pipeline coverage for the direct-AOT Boolean primitive family.

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

/// Boolean comparisons and both parsing spellings execute with their exact ABIs.
#[test]
fn bool_intrinsics_execute_truth_tables_and_strict_parsing() {
    let syntax = parse_module_as_syntax_output(
        r#"
module bool_primitive_family.
import std.core.Bool.{compare, equal, from_string, to_string}.
import std.core.Ordering.{Eq, Gt, Lt}.
import std.core.Option.{None, Some}.

pub equality(left: Bool, right: Bool): Bool -> equal(left, right).
pub comparison(left: Bool, right: Bool): Bool ->
    compare(left, right) == if {
        left == right -> Eq;
        left -> Gt;
        true -> Lt
    }.
pub parse_true(): Bool ->
    from_string("true") == Some(true) and Bool("true") == Some(true).
pub parse_false(): Bool ->
    from_string("false") == Some(false) and Bool("false") == Some(false).
pub parse_empty(): Bool -> from_string("") == None and Bool("") == None.
pub parse_case(): Bool -> from_string("True") == None and Bool("FALSE") == None.
pub parse_prefix(): Bool -> from_string("trues") == None and Bool("fals") == None.
pub parse_space(): Bool -> from_string(" true") == None and Bool("false ") == None.
pub parse_unicode(): Bool -> from_string("trüe") == None and Bool("真") == None.
pub rendering(value: Bool): Bool ->
    to_string(value) == if { value -> "true"; true -> "false" }.
"#,
    )
    .expect("parse Boolean family source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let ordering_syntax = parse_module_as_syntax_output(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../std/core/Ordering.terl"
    )))
    .expect("parse actual Comparison provider");
    let ordering_interfaces = checked_in_std_interfaces_for_module(&ordering_syntax);
    let ordering_resolved =
        resolve_syntax_module_output_with_interfaces(&ordering_syntax, &ordering_interfaces).module;
    let ordering = lower_syntax_module_output_to_core(&ordering_syntax, &ordering_resolved);
    let option_syntax = parse_module_as_syntax_output(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../std/core/Option.terl"
    )))
    .expect("parse actual Option provider");
    let option_interfaces = checked_in_std_interfaces_for_module(&option_syntax);
    let option_resolved =
        resolve_syntax_module_output_with_interfaces(&option_syntax, &option_interfaces).module;
    let option = lower_syntax_module_output_to_core(&option_syntax, &option_resolved);
    let modules = NativeModule::lower_application(&[&core, &ordering, &option])
        .expect("lower Boolean family");
    let object = super::emit_native_application_object("bool-family", &modules)
        .expect("emit Boolean family");
    use super::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let invocation = |name: &str, arguments: Vec<i64>, expected| {
        let export = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("Boolean export");
        NativeObjectInvocation {
            export_id: export.export_id,
            arguments,
            expected_status: super::status::OK,
            expected_result: Some(expected),
        }
    };
    let mut invocations = Vec::new();
    for left in [0, 1] {
        for right in [0, 1] {
            invocations.push(invocation(
                "equality",
                vec![left, right],
                i64::from(left == right),
            ));
            invocations.push(invocation("comparison", vec![left, right], 1));
        }
        invocations.push(invocation("rendering", vec![left], 1));
    }
    for name in [
        "parse_true",
        "parse_false",
        "parse_empty",
        "parse_case",
        "parse_prefix",
        "parse_space",
        "parse_unicode",
    ] {
        invocations.push(invocation(name, vec![], 1));
    }
    assert_managed_native_object_invocations("bool-family", &modules, &object, &invocations);
}
