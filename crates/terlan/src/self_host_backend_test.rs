use super::*;
use crate::self_host_ir::{
    BackendEnvelope, IrBlock, IrFunction, IrInstruction, IrModule, IrTerminator, ABI_MAJOR,
    ABI_MINOR,
};

fn instruction(opcode: &str, result: &str, operands: &[&str], type_name: &str) -> IrInstruction {
    IrInstruction {
        opcode: opcode.to_owned(),
        result: Some(result.to_owned()),
        operands: operands.iter().map(|value| (*value).to_owned()).collect(),
        operand_types: vec![None; operands.len()],
        type_name: type_name.to_owned(),
        continuation_ordinal: None,
        try_region: None,
    }
}

fn envelope(blocks: Vec<IrBlock>) -> BackendEnvelope {
    BackendEnvelope {
        abi_major: ABI_MAJOR,
        abi_minor: ABI_MINOR,
        target: "terlan-vm".to_owned(),
        module: IrModule {
            name: "demo.Adapter".to_owned(),
            structs: Vec::new(),
            functions: vec![IrFunction {
                module_name: "demo.Adapter".to_owned(),
                name: "choose".to_owned(),
                arity: 0,
                capture_count: 0,
                receiver_mutable: false,
                receiver_command: false,
                parameters: Vec::new(),
                return_type: "Int".to_owned(),
                blocks,
                native_symbol: None,
            }],
        },
    }
}

#[test]
fn resolves_branch_targets_to_block_indices() {
    let program = lower_backend_program(envelope(vec![
        IrBlock {
            label: "entry".to_owned(),
            instructions: vec![instruction("load-atom", "%condition", &["true"], "Bool")],
            terminator: IrTerminator {
                opcode: "branch".to_owned(),
                operands: vec!["%condition".to_owned()],
                targets: vec!["then".to_owned(), "else".to_owned()],
            },
        },
        IrBlock {
            label: "then".to_owned(),
            instructions: vec![instruction("load-literal", "%one", &["1"], "Int")],
            terminator: IrTerminator {
                opcode: "return".to_owned(),
                operands: vec!["%one".to_owned()],
                targets: Vec::new(),
            },
        },
        IrBlock {
            label: "else".to_owned(),
            instructions: vec![instruction("load-literal", "%zero", &["0"], "Int")],
            terminator: IrTerminator {
                opcode: "return".to_owned(),
                operands: vec!["%zero".to_owned()],
                targets: Vec::new(),
            },
        },
    ]))
    .expect("typed backend program");

    assert_eq!(
        program.functions[0].blocks[0].terminator,
        BackendTerminator::Branch {
            condition: "%condition".to_owned(),
            then_block: 1,
            else_block: 2,
        }
    );
}

#[test]
fn resolves_phi_predecessor_labels() {
    let program = lower_backend_program(envelope(vec![
        IrBlock {
            label: "entry".to_owned(),
            instructions: vec![instruction("load-literal", "%value", &["1"], "Int")],
            terminator: IrTerminator {
                opcode: "jump".to_owned(),
                operands: Vec::new(),
                targets: vec!["join".to_owned()],
            },
        },
        IrBlock {
            label: "join".to_owned(),
            instructions: vec![instruction("phi", "%result", &["entry", "%value"], "Int")],
            terminator: IrTerminator {
                opcode: "return".to_owned(),
                operands: vec!["%result".to_owned()],
                targets: Vec::new(),
            },
        },
    ]))
    .expect("typed backend program");

    assert_eq!(
        program.functions[0].blocks[1].instructions[0].operands,
        vec![
            BackendOperand::Block(0),
            BackendOperand::Text("%value".to_owned()),
        ]
    );
}
