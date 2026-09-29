use super::*;

fn envelope(instructions: Vec<IrInstruction>, terminator: IrTerminator) -> BackendEnvelope {
    BackendEnvelope {
        abi_major: ABI_MAJOR,
        abi_minor: ABI_MINOR,
        target: "terlan-vm".to_owned(),
        module: IrModule {
            name: "probe".to_owned(),
            functions: vec![IrFunction {
                module_name: "probe".to_owned(),
                name: "run".to_owned(),
                arity: 0,
                parameters: Vec::new(),
                return_type: "Bool".to_owned(),
                blocks: vec![IrBlock {
                    label: "entry".to_owned(),
                    instructions,
                    terminator,
                }],
                native_symbol: None,
            }],
        },
    }
}

fn instruction(opcode: &str, operands: &[&str]) -> IrInstruction {
    IrInstruction {
        opcode: opcode.to_owned(),
        result: Some("%result".to_owned()),
        operands: operands.iter().map(|value| (*value).to_owned()).collect(),
        type_name: "Bool".to_owned(),
    }
}

fn return_result() -> IrTerminator {
    IrTerminator {
        opcode: "return".to_owned(),
        operands: vec!["%result".to_owned()],
        targets: Vec::new(),
    }
}

#[test]
fn accepts_complete_pattern_encoding() {
    let checked = envelope(
        vec![instruction(
            "prepare-pattern",
            &[
                "%subject",
                "list-cons-pattern",
                "|",
                "2",
                "binding-pattern",
                "value",
                "0",
                "wildcard-pattern",
                "_",
                "0",
            ],
        )],
        return_result(),
    );
    assert!(validate(&checked).is_empty());
}

#[test]
fn rejects_unknown_instruction_opcode() {
    let checked = envelope(vec![instruction("invented", &["value"])], return_result());
    assert!(validate(&checked)
        .iter()
        .any(|diagnostic| diagnostic.code == "unsupported-instruction"));
}

#[test]
fn rejects_truncated_pattern_encoding() {
    let checked = envelope(
        vec![instruction(
            "prepare-pattern",
            &["%subject", "list-cons-pattern", "|", "1"],
        )],
        return_result(),
    );
    assert!(validate(&checked)
        .iter()
        .any(|diagnostic| diagnostic.code == "invalid-instruction-operands"));
}

#[test]
fn rejects_invalid_terminator_operand_count() {
    let checked = envelope(
        vec![instruction("load-literal", &["true"])],
        IrTerminator {
            opcode: "return".to_owned(),
            operands: Vec::new(),
            targets: Vec::new(),
        },
    );
    assert!(validate(&checked)
        .iter()
        .any(|diagnostic| diagnostic.code == "invalid-terminator-operand-count"));
}

#[test]
fn accepts_empty_aggregate_operands() {
    let checked = envelope(vec![instruction("put-list", &[])], return_result());
    assert!(validate(&checked).is_empty());
}
