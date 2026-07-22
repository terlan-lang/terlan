//! Focused checks for representation-safe checked-cast lowering.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, CoreExpr, CoreModule, CoreType};

use super::{NativeExpr, NativeModule};

fn cast_module(target_type: CoreType) -> CoreModule {
    let syntax = parse_module_as_syntax_output(
        "module checked_cast.\n\npub cast(value: Int): Int -> value.\n",
    )
    .expect("parse checked-cast source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    *core.functions[0].clauses[0]
        .body
        .core_expr
        .as_mut()
        .expect("checked body") = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Var("value".to_string())),
        target_type,
    };
    core
}

#[test]
fn representation_preserving_checked_cast_is_erased_in_native_ir() {
    let core = cast_module(CoreType::Int);
    let modules = NativeModule::lower_application(&[&core]).expect("checked cast NativeIR");

    assert_eq!(modules[0].functions[0].body, NativeExpr::Param(0));
}

#[test]
fn representation_changing_checked_cast_fails_before_linking() {
    let core = cast_module(CoreType::Bool);
    let error = NativeModule::lower_application(&[&core]).expect_err("reject incompatible cast");

    assert!(error.starts_with("error[native_ir.cast_check]"), "{error}");
}
