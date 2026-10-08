use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};

#[test]
fn callback_arguments_require_a_binding_without_rejecting_dynamic_parameters() {
    for (body, valid) in [
        ("accept(missing)", false),
        ("let callback = identity; accept(callback)", true),
        ("accept(identity)", true),
        ("accept(dynamic)", true),
    ] {
        let source = format!(
            r#"module callback_bindings.
identity(value: Int): Int -> value.
accept(callback: (Int) -> Int): Int -> callback(7).
pub check(dynamic: Dynamic): Int -> {body}.
"#
        );
        let syntax = parse_module_as_syntax_output(&source).unwrap();
        let resolved =
            resolve_syntax_module_output_with_interfaces(&syntax, &HashMap::new()).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert_eq!(diagnostics.is_empty(), valid, "{body}: {diagnostics:?}");
        if !valid {
            assert!(diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == "unresolved value `missing`"));
        }
    }
}

#[test]
fn imported_factory_preserves_renamed_callback_types() {
    let modules = [
        "module app.Values. pub struct Payload { value: Int }.",
        r#"module app.Factory.
import type app.Values.{Payload as Value}.
pub make(): (Value) -> Value -> (value: Value) -> value.
"#,
    ];
    let interfaces = modules
        .into_iter()
        .map(|source| {
            let syntax = parse_module_as_syntax_output(source).unwrap();
            let interface = syntax_module_output_to_interface(&syntax);
            crate::terlan_hir::parse_interface_text(&interface.to_terlan_interface_text()).unwrap()
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(
        interfaces["app.Factory"].functions[&("make".into(), 0)].return_type,
        "(app.Values.Payload) -> app.Values.Payload"
    );
    let syntax = parse_module_as_syntax_output(
        r#"module app.Consumer.
import app.Factory.{make}.
import type app.Values.Payload.
pub struct Receiver { value: Int }.
pub (receiver: Receiver) select(callback: (Payload) -> Payload): Receiver -> receiver.
pub check(): Receiver -> let callback = make(); Receiver(value = 0).select(callback).
"#,
    )
    .unwrap();
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn receiver_callback_aliases_qualify_imported_types_before_matching() {
    for alias in [
        "type Callback = (Payload) -> Payload.",
        "type Transform[T] = (T) -> T.\ntype Callback = Transform[Payload].",
    ] {
        let provider =
            parse_module_as_syntax_output("module app.Values. pub struct Payload { value: Int }.")
                .unwrap();
        let interface = syntax_module_output_to_interface(&provider);
        let interfaces = HashMap::from([(interface.module.clone(), interface)]);
        for (argument, valid) in [
            ("identity", true),
            ("wrong_input", false),
            ("wrong_output", false),
        ] {
            let source = format!(
                r#"
module app.Receiver.
import type app.Values.Payload.
pub struct Receiver {{ value: Int }}.
{alias}
pub (receiver: Receiver) select(callback: Callback): Callback -> callback.
identity(value: Payload): Payload -> value.
wrong_input(value: Int): Payload -> Payload(value = value).
wrong_output(value: Payload): Int -> value.value.
pub check(): Callback -> Receiver(value = 0).select({argument}).
"#
            );
            let syntax = parse_module_as_syntax_output(&source).unwrap();
            let resolved =
                resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
            let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
            assert_eq!(diagnostics.is_empty(), valid, "{argument}: {diagnostics:?}");
        }
    }
}
