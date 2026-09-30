use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};

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
