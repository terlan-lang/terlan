//! Source-to-object proof that nested shadowing retains exact lexical values.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::assert_native_object_result;
use super::{emit_native_application_object, NativeModule};

#[test]
fn native_lowering_preserves_outer_and_nested_binding_identities() {
    let syntax = parse_module_as_syntax_output(
        r#"
module native_binding_identity.

pub choose(value: Int, use_inner: Bool): Int ->
    if {
        use_inner ->
            case 5 {
                value -> value
            };
        true -> value
    }.
"#,
    )
    .expect("parse native binding fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.binding_identities
        .validate()
        .expect("native CoreIR binding evidence");
    let value_ids = core
        .binding_identities
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .map(|binding| binding.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(value_ids.len(), 2, "{value_ids:#?}");

    let modules =
        NativeModule::lower_application(&[&core]).expect("lower native binding application");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "choose")
        .expect("choose export")
        .export_id;
    let object =
        emit_native_application_object("native_binding_identity", &modules).expect("emit object");

    assert_native_object_result("native-binding-outer", &object, export_id, &[17, 0], 17);
    assert_native_object_result("native-binding-inner", &object, export_id, &[17, 1], 5);
}
