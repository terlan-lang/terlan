//! Compiler-to-shard transition checks for declared asynchronous capabilities.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::{NativeExpr, NativeModule, NativeTransitionOperation};

fn lower(source: &str) -> NativeModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse capability source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core])
        .expect("lower capability application")
        .remove(0)
}

#[test]
fn declared_console_and_file_capabilities_suspend_with_typed_results() {
    let module = lower(
        "module native_capabilities.\n\n\
         pub print(): Unit -> std.io.Console.println(\"hello\").\n\n\
         pub exists(path: String): Bool -> std.io.File.exists(path).\n",
    );
    for name in ["print", "exists"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert!(matches!(
            function.body,
            NativeExpr::Suspend {
                operation: NativeTransitionOperation::Capability,
                ref arguments,
                ..
            } if matches!(arguments.first(), Some(NativeExpr::Int(1 | 2)))
                && arguments.len() == 5
        ));
    }
}
