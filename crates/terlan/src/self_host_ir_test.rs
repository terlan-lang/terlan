use super::*;

fn envelope(instructions: Vec<IrInstruction>, terminator: IrTerminator) -> BackendEnvelope {
    BackendEnvelope {
        abi_major: ABI_MAJOR,
        abi_minor: ABI_MINOR,
        target: "terlan-vm".to_owned(),
        module: IrModule {
            name: "probe".to_owned(),
            structs: Vec::new(),
            functions: vec![IrFunction {
                module_name: "probe".to_owned(),
                name: "run".to_owned(),
                arity: 0,
                capture_count: 0,
                receiver_mutable: false,
                receiver_command: false,
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
        operand_types: vec![None; operands.len()],
        type_name: "Bool".to_owned(),
        continuation_ordinal: None,
        try_region: None,
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

#[test]
fn continuation_ordinal_round_trips_in_abi_1_5() {
    let instruction = IrInstruction {
        opcode: "call".to_owned(),
        result: Some("%result".to_owned()),
        operands: vec!["demo.run".to_owned()],
        operand_types: Vec::new(),
        type_name: "Int".to_owned(),
        continuation_ordinal: Some(7),
        try_region: None,
    };
    let encoded = serde_json::to_string(&instruction).expect("encode instruction");
    let decoded: IrInstruction = serde_json::from_str(&encoded).expect("decode instruction");
    assert_eq!(decoded.continuation_ordinal, Some(7));
}

#[test]
fn structured_try_region_round_trips_in_abi_1_6() {
    fn region(label: &str, parameter: Option<(&str, &str)>, result: &str, type_name: &str) -> IrRegion {
        IrRegion {
            label: label.to_owned(),
            parameters: parameter
                .into_iter()
                .map(|(name, type_name)| IrParameter {
                    name: name.to_owned(),
                    type_name: type_name.to_owned(),
                })
                .collect(),
            blocks: Vec::new(),
            result: result.to_owned(),
            type_name: type_name.to_owned(),
        }
    }

    let instruction = IrInstruction {
        opcode: "try-region".to_owned(),
        result: Some("%result".to_owned()),
        operands: Vec::new(),
        operand_types: Vec::new(),
        type_name: "Int".to_owned(),
        continuation_ordinal: None,
        try_region: Some(IrTryRegion {
            protected: region("protected", None, "%protected", "Int"),
            success: region("success", Some(("%value", "Int")), "%success", "Int"),
            failure: region("failure", Some(("%status", "Int")), "%failure", "Int"),
            cleanup: vec![region("cleanup", None, "%unit", "Unit")],
        }),
    };
    let encoded = serde_json::to_string(&instruction).expect("encode structured try");
    let decoded: IrInstruction = serde_json::from_str(&encoded).expect("decode structured try");
    assert_eq!(decoded.try_region, instruction.try_region);
}
