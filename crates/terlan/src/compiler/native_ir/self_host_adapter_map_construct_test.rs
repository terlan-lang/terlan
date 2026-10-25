use super::*;

fn instruction(operands: &[&str], operand_types: &[&str]) -> IrInstruction {
    IrInstruction {
        opcode: "put-map".to_owned(),
        result: Some("%map".to_owned()),
        operands: operands.iter().map(|value| (*value).to_owned()).collect(),
        operand_types: operand_types
            .iter()
            .map(|value| Some((*value).to_owned()))
            .collect(),
        type_name: "Map[String, Int]".to_owned(),
        continuation_ordinal: None,
        try_region: None,
    }
}

#[test]
fn map_construct_accepts_exact_alternating_type_evidence() {
    let mut values = ValueCatalog::default();
    values.insert("%key".to_owned(), (0, NativeType::StringRef));
    values.insert("%value".to_owned(), (1, NativeType::Int));

    let (expression, result_type) = map_construct_expr(
        &instruction(&["%key", "%value"], &["String", "Int"]),
        &values,
    )
    .expect("typed map construction");

    assert!(matches!(result_type, NativeType::ManagedRef(_)));
    assert!(matches!(
        expression,
        NativeExpr::ManagedOperation { args, .. } if args.len() == 2
    ));
}

#[test]
fn map_construct_rejects_odd_operands_with_positive_companion() {
    map_construct_accepts_exact_alternating_type_evidence();
    let values = ValueCatalog::default();
    let error = map_construct_expr(&instruction(&["%key"], &["String"]), &values)
        .expect_err("odd map operands must fail");
    assert!(error.contains("alternating key/value operands"));
}
