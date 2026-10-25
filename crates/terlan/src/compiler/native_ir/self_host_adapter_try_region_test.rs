use super::*;

fn literal(result: &str, value: &str, type_name: &str) -> IrInstruction {
    IrInstruction {
        opcode: "load-literal".to_owned(),
        result: Some(result.to_owned()),
        operands: vec![value.to_owned()],
        operand_types: vec![None],
        type_name: type_name.to_owned(),
        continuation_ordinal: None,
        try_region: None,
    }
}

fn region(
    label: &str,
    parameters: Vec<IrParameter>,
    result: &str,
    type_name: &str,
    instructions: Vec<IrInstruction>,
) -> IrRegion {
    IrRegion {
        label: label.to_owned(),
        parameters,
        blocks: vec![IrBlock {
            label: "entry".to_owned(),
            instructions,
            terminator: IrTerminator {
                opcode: "return".to_owned(),
                operands: vec![result.to_owned()],
                targets: vec![],
            },
        }],
        result: result.to_owned(),
        type_name: type_name.to_owned(),
    }
}

fn instruction(success_parameters: Vec<IrParameter>) -> IrInstruction {
    IrInstruction {
        opcode: "try-region".to_owned(),
        result: Some("%result".to_owned()),
        operands: vec![],
        operand_types: vec![],
        type_name: "Int".to_owned(),
        continuation_ordinal: None,
        try_region: Some(IrTryRegion {
            protected: region(
                "protected",
                vec![],
                "%protected",
                "Int",
                vec![literal("%protected", "1", "Int")],
            ),
            success: region("success", success_parameters, "%value", "Int", vec![]),
            failure: region(
                "failure",
                vec![IrParameter {
                    name: "%status".to_owned(),
                    type_name: "Int".to_owned(),
                }],
                "%status",
                "Int",
                vec![],
            ),
            cleanup: vec![region(
                "cleanup",
                vec![],
                "%unit",
                "Unit",
                vec![literal("%unit", "Unit", "Unit")],
            )],
        }),
    }
}

#[test]
fn structured_try_lowers_to_native_try() {
    let expression = try_region_expression(
        &instruction(vec![IrParameter {
            name: "%value".to_owned(),
            type_name: "Int".to_owned(),
        }]),
        "sample",
        &ValueCatalog::default(),
        &FunctionCatalog::new(),
        &mut vec![],
    )
    .expect("valid structured try");
    assert_eq!(expression.1, NativeType::Int);
    assert!(matches!(
        expression.0,
        NativeExpr::Try { cleanup, .. } if cleanup.len() == 1
    ));
}

#[test]
fn structured_try_rejects_invalid_success_role_with_positive_companion() {
    structured_try_lowers_to_native_try();
    let error = try_region_expression(
        &instruction(vec![]),
        "sample",
        &ValueCatalog::default(),
        &FunctionCatalog::new(),
        &mut vec![],
    )
    .expect_err("missing success parameter must fail");
    assert!(error.contains("success region must accept exactly"));
}
