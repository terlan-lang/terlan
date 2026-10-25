use super::*;
use crate::self_host_ir::{IrModule, IrParameter, IrTerminator, ABI_MAJOR, ABI_MINOR};

fn function(
    module: &str,
    name: &str,
    instructions: Vec<IrInstruction>,
    returned: &str,
) -> IrFunction {
    IrFunction {
        module_name: module.to_owned(),
        name: name.to_owned(),
        arity: 1,
        capture_count: 0,
        receiver_mutable: false,
        receiver_command: false,
        parameters: vec![IrParameter {
            name: "value".to_owned(),
            type_name: "Int".to_owned(),
        }],
        return_type: "Int".to_owned(),
        blocks: vec![IrBlock {
            label: "entry".to_owned(),
            instructions,
            terminator: IrTerminator {
                opcode: "return".to_owned(),
                operands: vec![returned.to_owned()],
                targets: Vec::new(),
            },
        }],
        native_symbol: None,
    }
}

fn envelope(module: &str, function: IrFunction) -> BackendEnvelope {
    BackendEnvelope {
        abi_major: ABI_MAJOR,
        abi_minor: ABI_MINOR,
        target: "terlan-vm".to_owned(),
        module: IrModule {
            name: module.to_owned(),
            structs: Vec::new(),
            functions: vec![function],
        },
    }
}

fn imported_call(module: &str) -> IrInstruction {
    IrInstruction {
        opcode: "call-import".to_owned(),
        result: Some("result".to_owned()),
        operands: vec![module.to_owned(), "id".to_owned(), "value".to_owned()],
        operand_types: Vec::new(),
        type_name: "Int".to_owned(),
        continuation_ordinal: None,
        try_region: None,
    }
}

#[test]
fn imported_call_uses_global_application_index() {
    let provider = envelope("provider", function("provider", "id", Vec::new(), "value"));
    let caller = envelope(
        "caller",
        function("caller", "run", vec![imported_call("provider")], "result"),
    );
    let modules = lower_self_hosted_application(&[provider, caller])
        .expect("admitted cross-module application");
    let NativeExpr::Let { bindings, .. } = &modules[1].functions[0].body else {
        panic!("caller body must bind imported result")
    };
    assert!(matches!(
        bindings.as_slice(),
        [NativeExpr::Call { function: 0, .. }]
    ));
}

#[test]
fn missing_import_target_is_rejected() {
    let caller = envelope(
        "caller",
        function("caller", "run", vec![imported_call("missing")], "result"),
    );
    let error = lower_self_hosted_application(&[caller]).expect_err("missing import");
    assert!(error.contains("not in this ABI module"));
}
