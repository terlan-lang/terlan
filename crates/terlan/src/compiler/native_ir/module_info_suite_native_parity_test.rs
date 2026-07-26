//! Direct-AOT replacements for portable `module_info_SUITE` contracts.

use std::collections::HashSet;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

fn native_module_with_public_and_private_functions() -> Vec<NativeModule> {
    let syntax = parse_module_as_syntax_output(
        "\
module module_info_suite_native.\n\
\n\
helper(value: Int): Int -> value + 1.\n\
pub current(): Int -> 17.\n\
pub value(input: Int): Int -> helper(input).\n",
    )
    .expect("parse module-info replacement source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core]).expect("lower module-info replacement")
}

#[test]
fn native_descriptor_has_stable_unique_ids_and_no_beam_pseudo_exports() {
    let modules = native_module_with_public_and_private_functions();
    let module = modules
        .iter()
        .find(|module| module.name == "module_info_suite_native")
        .expect("compiled source module");
    let mut functions = module
        .functions
        .iter()
        .map(|function| {
            (
                function.name.as_str(),
                function.arity,
                function.public,
                function.export_id,
            )
        })
        .collect::<Vec<_>>();
    functions.sort_unstable_by_key(|(name, arity, _, _)| (*name, *arity));

    assert_eq!(
        functions
            .iter()
            .map(|(name, arity, public, _)| (*name, *arity, *public))
            .collect::<Vec<_>>(),
        vec![
            ("current", 0, true),
            ("helper", 1, false),
            ("value", 1, true),
        ]
    );
    assert!(functions
        .iter()
        .map(|(_, _, _, export_id)| export_id)
        .all(|export_id| *export_id != 0));
    assert_eq!(
        functions
            .iter()
            .map(|(_, _, _, export_id)| export_id)
            .collect::<HashSet<_>>()
            .len(),
        functions.len()
    );
    assert!(
        functions
            .iter()
            .all(|(name, _, _, _)| *name != "module_info"),
        "Terlan must not synthesize BEAM module_info/0 or module_info/1 exports"
    );
}

#[test]
fn public_descriptor_entries_execute_through_the_linked_native_object() {
    let modules = native_module_with_public_and_private_functions();
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.public)
        .map(|function| (function.name.as_str(), function.export_id))
        .collect::<std::collections::HashMap<_, _>>();
    let object = emit_native_application_object("module_info_suite_native", &modules)
        .expect("emit module-info replacement object");

    assert_native_object_invocations(
        "module-info-suite-native",
        &object,
        &[
            NativeObjectInvocation {
                export_id: exports["current"],
                arguments: Vec::new(),
                expected_status: status::OK,
                expected_result: Some(17),
            },
            NativeObjectInvocation {
                export_id: exports["value"],
                arguments: vec![41],
                expected_status: status::OK,
                expected_result: Some(42),
            },
        ],
    );
}
