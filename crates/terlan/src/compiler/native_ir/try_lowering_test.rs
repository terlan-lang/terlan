//! Executable checks for native protected regions and cleanup.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::native_object_test_support::assert_native_object_result;
use super::{emit_native_application_object, NativeExpr, NativeModule};

fn lower(source: &str) -> NativeModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse Try source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core])
        .expect("lower Try application")
        .remove(0)
}

#[test]
fn checked_failure_is_caught_and_cleanup_remains_in_native_ir() {
    let module = lower(
        "module native_try.\n\n\
         pub recover(divisor: Int): Int ->\n\
             try 84 div divisor {\n\
                 value -> value\n\
             catch\n\
                 _reason -> 42\n\
             after\n\
                 0 -> 7\n\
             }.\n",
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "recover")
        .expect("recover function");
    assert!(matches!(
        function.body,
        NativeExpr::Try { ref cleanup, .. } if cleanup.len() == 2
    ));
    let export = function.export_id;
    let object = emit_native_application_object("native_try", &[module]).expect("native object");
    assert_native_object_result("native-try-success", &object, export, &[2], 42);
    assert_native_object_result("native-try-catch", &object, export, &[0], 42);
}
