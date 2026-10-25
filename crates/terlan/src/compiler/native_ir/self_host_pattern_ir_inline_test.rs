use super::*;

fn instruction(operands: &[&str]) -> IrInstruction {
    IrInstruction {
        opcode: "prepare-pattern".into(),
        result: Some("environment".into()),
        operands: operands.iter().map(|value| (*value).into()).collect(),
        operand_types: (0..operands.len()).map(|_| None).collect(),
        type_name: "PatternMatch".into(),
        continuation_ordinal: None,
        try_region: None,
    }
}

#[test]
fn recursive_pattern_metadata_decodes_with_binding_paths() {
    let instruction = instruction(&[
        "subject",
        "list-cons-pattern",
        "|",
        "2",
        "binding-pattern",
        "head",
        "0",
        "wildcard-pattern",
        "_",
        "0",
    ]);

    let (subject, pattern) = decode_prepare_pattern(&instruction).expect("pattern metadata");

    assert_eq!(subject, "subject");
    assert_eq!(
        pattern.bindings(),
        vec![PatternBinding {
            name: "head".into(),
            path: vec![0],
        }]
    );
}

#[test]
fn truncated_pattern_metadata_is_rejected() {
    let error = decode_prepare_pattern(&instruction(&["subject", "tuple-pattern", "{}", "1"]))
        .expect_err("truncated pattern");

    assert!(error.contains("missing pattern kind"), "{error}");
}

#[test]
fn trailing_pattern_metadata_is_rejected() {
    let error = decode_prepare_pattern(&instruction(&[
        "subject",
        "wildcard-pattern",
        "_",
        "0",
        "trailing",
    ]))
    .expect_err("trailing pattern");

    assert!(error.contains("trailing operand"), "{error}");
}

#[test]
fn v2_prepare_pattern_marker_preserves_recursive_metadata() {
    let instruction = instruction(&[
        "subject",
        "pattern-v2",
        "list-cons-pattern",
        "|",
        "2",
        "binding-pattern",
        "head",
        "0",
        "wildcard-pattern",
        "_",
        "0",
    ]);

    let (_, pattern) = decode_prepare_pattern(&instruction).expect("v2 pattern metadata");

    assert!(prepare_pattern_deferred(&instruction));
    assert_eq!(pattern.kind, "list-cons-pattern");
    assert_eq!(pattern.bindings()[0].name, "head");
}

#[test]
fn v2_binding_metadata_decodes_selector_source_and_path() {
    let instruction = IrInstruction {
        opcode: "get-pattern-binding".into(),
        result: Some("head".into()),
        operands: [
            "environment",
            "0",
            "List[Widget]",
            "list-cons-pattern",
            "|",
            "2",
            "binding-pattern",
            "head",
            "0",
            "wildcard-pattern",
            "_",
            "0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        operand_types: Vec::new(),
        type_name: "Widget".into(),
        continuation_ordinal: None,
        try_region: None,
    };

    let (selector, source, pattern) =
        decode_binding_pattern(&instruction).expect("v2 binding metadata");

    assert_eq!(selector, 0);
    assert_eq!(source, "List[Widget]");
    assert_eq!(pattern.bindings()[0].path, vec![0]);
}
