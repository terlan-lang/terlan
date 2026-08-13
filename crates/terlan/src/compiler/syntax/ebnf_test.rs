use super::*;
use serde::{Deserialize, Serialize};

/// Verifies that basic EBNF rules parse into a contract.
#[test]
fn parses_simple_rules() {
    let grammar = parse_ebnf_ast(
        r#"
            (* comments are skipped *)
            Program ::= { Declaration } .
            Declaration ::= "module" Identifier "." | RawDecl "." .
            Identifier ::= [ "@" ] Letter+ .
            "#,
    )
    .expect("parse ebnf");

    assert_eq!(grammar.rules.len(), 3);
    assert!(grammar.rule("Program").is_some());
    assert!(matches!(
        grammar.rule("Declaration").unwrap().expr.kind,
        EbnfGrammarExprKind::Alternation { .. }
    ));
}

/// Verifies that the canonical Terlan grammar parses as EBNF.
#[test]
fn parses_canonical_terlan_ebnf() {
    let grammar = parse_ebnf_ast(include_str!(
        "../../../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf"
    ))
    .expect("parse canonical Terlan EBNF");

    assert!(grammar.rule("SyntaxSpec").is_some());
    assert!(grammar.rule("Script").is_some());
    assert!(grammar.rule("ScriptBody").is_some());
    assert!(grammar.rule("ScriptBinding").is_some());
    assert!(grammar.rule("Declaration").is_some());
    assert!(grammar.rule("ShapeDecl").is_some());
    assert!(grammar.rule("StructuralParameterPattern").is_some());
    assert!(grammar.rule("Expr").is_some());
    assert!(grammar.rule("StringChar").is_some());
    assert!(matches!(
        grammar.rule("LowerIdent").unwrap().expr.kind,
        EbnfGrammarExprKind::Sequence { .. }
    ));
    assert!(grammar.rules.len() > 100);
}

/// Verifies the public parse entry point returns a grammar contract.
#[test]
fn parse_ebnf_returns_grammar_contract() {
    let output = parse_ebnf("Program ::= Symbol .\nSymbol ::= \"a\" .").expect("compile ebnf");

    assert_eq!(output.format_version, 1);
    assert_eq!(output.entry_rule, Some("Program".to_string()));
    assert_eq!(output.rules.len(), 2);
}

/// Verifies the compile entry point returns rule metadata.
#[test]
fn compiles_ebnf_to_grammar_contract() {
    let output = compile_ebnf("Program ::= Symbol .\nSymbol ::= \"a\" .").expect("compile ebnf");

    assert_eq!(output.format_version, 1);
    assert_eq!(output.entry_rule, Some("Program".to_string()));
    assert_eq!(output.rules.len(), 2);
    assert_eq!(output.rules[0].name, "Program");
    assert_eq!(output.rules[1].name, "Symbol");
}

/// Verifies the contract entry point assigns rule and expression spans.
#[test]
fn compiles_ebnf_to_spanned_contract() {
    let output =
        compile_ebnf_contract("Program ::= Symbol .\nSymbol ::= \"a\" .").expect("compile ebnf");

    assert_eq!(output.format_version, 1);
    assert_eq!(output.entry_rule, Some("Program".to_string()));
    assert_eq!(output.rules.len(), 2);
    let program = output.rule("Program").expect("Program rule");
    assert_eq!(program.id, "rule:Program");
    assert_eq!(program.expr.id, "rule:Program/expr");
    assert!(program.span.end > program.span.start);
    assert!(matches!(
        program.expr.kind,
        EbnfGrammarExprKind::Nonterminal { .. }
    ));
}

/// Verifies the canonical grammar contract summary remains stable.
#[test]
fn canonical_terlan_ebnf_contract_matches_golden_summary() {
    let output = compile_ebnf_contract(include_str!(
        "../../../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf"
    ))
    .expect("compile canonical Terlan EBNF contract");

    let actual = ContractSummary::from_contract(&output);
    let expected = serde_json::from_str::<ContractSummary>(include_str!(
        "../../../../../docs/grammar/fixtures/contract/terlan_syntax_spec_contract_summary.json"
    ))
    .expect("parse golden contract summary");

    assert_eq!(actual, expected);
}

/// Verifies EBNF contracts serialize to JSON.
#[test]
fn compiles_ebnf_to_json() {
    let json = compile_ebnf_to_json("Program ::= Symbol .\nSymbol ::= \"a\" .")
        .expect("compile ebnf to json");

    let value = serde_json::from_str::<serde_json::Value>(&json).expect("json output");
    assert_eq!(value["entry_rule"], "Program");
    assert_eq!(value["rules"].as_array().map(|rules| rules.len()), Some(2));
}

/// Verifies unterminated comments report a specific parse diagnostic.
#[test]
fn reports_unterminated_comment() {
    let error = parse_ebnf("Rule ::= Atom . (*").expect_err("unterminated comment");

    let EbnfCompileError::Parse(message, _) = error else {
        panic!("expected parse error");
    };
    assert_eq!(message, "unterminated EBNF comment");
}

/// Verifies missing rule terminators report a specific parse diagnostic.
#[test]
fn reports_missing_rule_dot() {
    let error = parse_ebnf("Rule ::= Atom").expect_err("missing dot");

    let EbnfCompileError::Parse(message, _) = error else {
        panic!("expected parse error");
    };
    assert_eq!(message, "expected '.' after EBNF rule");
}

/// Stable summary fixture for canonical EBNF contract tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ContractSummary {
    format_version: u32,
    entry_rule: String,
    rule_count: usize,
    key_rules: Vec<RuleSummary>,
}

/// Stable per-rule summary fixture for canonical EBNF contract tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RuleSummary {
    name: String,
    id: String,
    expr_id: String,
    kind: String,
}

impl ContractSummary {
    /// Builds a stable summary from a full grammar contract.
    ///
    /// Inputs:
    /// - `contract`: compiled grammar contract.
    ///
    /// Output:
    /// - Compact summary containing selected key rules.
    ///
    /// Transformation:
    /// - Extracts deterministic metadata for the rules that protect the
    ///   public syntax contract.
    fn from_contract(contract: &EbnfGrammarContract) -> Self {
        let key_rules = [
            "SyntaxSpec",
            "Declaration",
            "DeclarationCore",
            "Annotation",
            "AnnotationBlock",
            "AnnotationItem",
            "AnnotationEntry",
            "AnnotationValue",
            "AnnotationSchemaDecl",
            "ShapeDecl",
            "StructuralParameterPattern",
            "Expr",
            "PipeExpr",
            "OrExpr",
            "AndExpr",
            "PostfixExpr",
            "PrimaryExpr",
            "Pattern",
            "ListPattern",
            "CallExpr",
            "ScopedCallExpr",
            "RawMacroExpr",
            "ConfigDecl",
            "MetadataBlock",
            "TypeRef",
        ]
        .into_iter()
        .map(|name| {
            let rule = contract
                .rule(name)
                .unwrap_or_else(|| panic!("missing rule {name}"));
            RuleSummary {
                name: rule.name.clone(),
                id: rule.id.clone(),
                expr_id: rule.expr.id.clone(),
                kind: expr_kind_name(&rule.expr).to_string(),
            }
        })
        .collect();

        Self {
            format_version: contract.format_version,
            entry_rule: contract
                .entry_rule
                .clone()
                .expect("canonical grammar has entry rule"),
            rule_count: contract.rules.len(),
            key_rules,
        }
    }
}

/// Returns the stable fixture name for an expression kind.
///
/// Inputs:
/// - `expr`: grammar expression to classify.
///
/// Output:
/// - Snake-case kind name used in contract summaries.
///
/// Transformation:
/// - Maps enum variants to their serialized fixture spelling.
fn expr_kind_name(expr: &EbnfGrammarExpr) -> &'static str {
    match &expr.kind {
        EbnfGrammarExprKind::Nonterminal { .. } => "nonterminal",
        EbnfGrammarExprKind::Terminal { .. } => "terminal",
        EbnfGrammarExprKind::CharacterClass { .. } => "character_class",
        EbnfGrammarExprKind::Special { .. } => "special",
        EbnfGrammarExprKind::Sequence { .. } => "sequence",
        EbnfGrammarExprKind::Alternation { .. } => "alternation",
        EbnfGrammarExprKind::Optional { .. } => "optional",
        EbnfGrammarExprKind::Repetition { .. } => "repetition",
        EbnfGrammarExprKind::Group { .. } => "group",
        EbnfGrammarExprKind::OneOrMore { .. } => "one_or_more",
    }
}
