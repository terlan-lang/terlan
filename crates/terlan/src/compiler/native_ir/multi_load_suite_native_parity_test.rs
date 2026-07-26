//! Direct-AOT replacements for portable `multi_load_SUITE` contracts.

use std::collections::{HashMap, HashSet};

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreModule,
};

use super::native_object_test_support::{assert_native_object_invocations, NativeObjectInvocation};
use super::{emit_native_application_object, status, NativeModule};

const MODULES: usize = 100;
const FUNCTIONS_PER_MODULE: usize = 4;

fn core_module(index: usize) -> CoreModule {
    let base = index * FUNCTIONS_PER_MODULE;
    let source = format!(
        "\
module multi_load_{index:03}.\n\
\n\
pub f1(): Int -> {}.\n\
pub f2(): Int -> {}.\n\
pub f3(): Int -> {}.\n\
pub f4(): Int -> {}.\n",
        base + 1,
        base + 2,
        base + 3,
        base + 4
    );
    let syntax = parse_module_as_syntax_output(&source).expect("parse multi-load source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

#[test]
fn multi_load_suite_many_modules_execute_from_one_linked_aot_image() {
    let cores = (0..MODULES).map(core_module).collect::<Vec<_>>();
    let core_refs = cores.iter().collect::<Vec<_>>();
    let modules =
        NativeModule::lower_application(&core_refs).expect("lower closed multi-module image");

    assert_eq!(modules.len(), MODULES);
    assert_eq!(
        modules
            .iter()
            .map(|module| module.name.as_str())
            .collect::<HashSet<_>>()
            .len(),
        MODULES
    );
    let exports = modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(|function| function.public)
                .map(move |function| {
                    (
                        (module.name.clone(), function.name.clone()),
                        function.export_id,
                    )
                })
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(exports.len(), MODULES * FUNCTIONS_PER_MODULE);
    assert_eq!(
        exports.values().copied().collect::<HashSet<_>>().len(),
        exports.len(),
        "every export in the closed image must have a unique dispatch identity"
    );

    let object =
        emit_native_application_object("multi_load_suite_native", &modules).expect("native object");
    let invocations = (0..MODULES)
        .flat_map(|index| {
            (1..=FUNCTIONS_PER_MODULE).map({
                let exports = &exports;
                move |function| NativeObjectInvocation {
                    export_id: exports[&(format!("multi_load_{index:03}"), format!("f{function}"))],
                    arguments: Vec::new(),
                    expected_status: status::OK,
                    expected_result: Some((index * FUNCTIONS_PER_MODULE + function) as i64),
                }
            })
        })
        .collect::<Vec<_>>();
    assert_native_object_invocations("multi-load-suite-native", &object, &invocations);
}
