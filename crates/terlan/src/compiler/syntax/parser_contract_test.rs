use super::*;

/// Verifies parser contract output includes the program entry and
/// declaration rules.
///
/// Inputs:
/// - Source module containing imports, a type declaration, and a function.
///
/// Output:
/// - Assertions over the projected EBNF contract tree.
///
/// Transformation:
/// - Parses canonical source and projects it through the contract path to
///   ensure declaration classes and module-name terminal rules are stable.
#[test]
fn module_contract_includes_program_entry_and_declarations() {
    let output = parse_module_as_contract(
        r#"
            module demo.

            import lib.Mod.
            type Item = Int.
            pub add(X: Int): Int -> X + 1.
            "#,
    )
    .expect("parse to contract");

    assert_eq!(output.entry_rule.as_deref(), Some("Program"));
    assert!(output
        .rules
        .iter()
        .any(|rule| rule.name == "Program" || rule.name == "ModuleDecl"));
    let module_name_rule = output
        .rules
        .iter()
        .find(|rule| rule.name == "ModuleName")
        .expect("module name rule");
    assert!(matches!(
        module_name_rule.expr.kind,
        EbnfGrammarExprKind::Terminal { .. }
    ));
    let EbnfGrammarExprKind::Terminal { value } = &module_name_rule.expr.kind else {
        panic!("expected terminal module name")
    };
    assert_eq!(value, "demo");
    assert_eq!(module_name_rule.id, "rule:ModuleName");
    assert_eq!(module_name_rule.expr.id, "rule:ModuleName/expr");
}

/// Verifies interface parsing uses the same contract projection rules.
///
/// Inputs:
/// - Interface module containing an export summary.
///
/// Output:
/// - Assertions over the projected EBNF contract tree.
///
/// Transformation:
/// - Parses `.terli` interface text and checks that interface-only
///   declarations still project through the shared contract shape.
#[test]
fn interface_contract_follows_same_rules() {
    let output = parse_interface_module_as_contract(
        r#"
            module demo.

            export demo/1.
            "#,
    )
    .expect("parse interface contract");

    assert_eq!(output.entry_rule.as_deref(), Some("Program"));
    assert!(output.rules.iter().any(|rule| rule.name == "ExportDecl"));
}

/// Verifies the normal source contract path cannot reintroduce export-list
/// declarations.
///
/// Inputs:
/// - `.terl` module source containing removed source-mode `export` syntax.
///
/// Output:
/// - Parse diagnostic from the normal source parser.
///
/// Transformation:
/// - Routes the source through `parse_module_as_contract`, proving contract
///   projection starts after canonical source validation.
#[test]
fn module_contract_rejects_source_export_declarations() {
    let error = parse_module_as_contract(
        r#"
            module demo.

            export demo/1.
            "#,
    )
    .expect_err("normal source contract must reject export lists");

    match error {
        EbnfCompileError::Parse(message, _) => {
            assert!(message.contains("source export declarations are not part of canonical Terlan"));
        }
        other => panic!("unexpected contract error: {other:?}"),
    }
}

/// Verifies parser contract output can serialize through JSON.
///
/// Inputs:
/// - Source module with a simple type declaration.
///
/// Output:
/// - Round-tripped `EbnfGrammarContract` with stable entry and rule count.
///
/// Transformation:
/// - Exercises serde serialization for parser contract artifacts used by
///   grammar validation tooling.
#[test]
fn contract_output_is_serializable_via_grammar_contract_path() {
    let output = parse_module_as_contract(
        r#"
            module demo.

            type X = Int.
            "#,
    )
    .expect("parse contract");

    let raw = serde_json::to_string(&output).expect("to json");
    let decoded = serde_json::from_str::<EbnfGrammarContract>(&raw).expect("from json");
    assert_eq!(decoded.entry_rule, Some("Program".to_string()));
    assert_eq!(decoded.rules.len(), output.rules.len());
}

/// Verifies parser declaration classes remain stable.
///
/// Inputs:
/// - Synthetic raw config declaration.
///
/// Output:
/// - Assertion that config raw declarations project as `ConfigDecl`.
///
/// Transformation:
/// - Protects the compatibility shim that maps preserved config syntax
///   into the formal parser contract class.
#[test]
fn module_decl_class_mapping_is_stable() {
    use crate::terlan_syntax::parse_tree::Decl;
    let class = contract_decl_class(&Decl::Raw(
        crate::terlan_syntax::parse_tree::UnsupportedDecl {
            kind: "target".into(),
            text: "{}".into(),
            docs: vec![],
            span: crate::terlan_syntax::span::Span::new(0, 0),
        },
    ));
    assert_eq!(class, "ConfigDecl");
}
