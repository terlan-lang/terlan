//! Source-to-object stack-safety proof for compiler-owned tail lowering.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::assert_native_object_result_on_small_stack;
use super::{emit_native_application_object, NativeModule};

#[test]
fn source_case_tail_recursion_executes_one_million_edges_on_a_small_stack() {
    let syntax = parse_module_as_syntax_output(
        "module source_case_tail.\n\n\
         pub count(n: Int, acc: Int): Int ->\n\
             case n {\n\
                 0 -> acc;\n\
                 _ -> count(n - 1, acc + 1)\n\
             }.\n",
    )
    .expect("parse source tail recursion");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower source tail-recursive application");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "count")
        .expect("source count export")
        .export_id;
    let object =
        emit_native_application_object("source_case_tail", &modules).expect("emit source object");

    assert_native_object_result_on_small_stack(
        "source-case-tail",
        &object,
        export_id,
        &[1_000_000, 0],
        1_000_000,
    );
}
