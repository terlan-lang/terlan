use super::*;
use crate::compiler::syntax::{canonical_terlan_syntax_contract, compile_ebnf};

fn recognizer(source: &str) -> Recognizer {
    Recognizer::new(&compile_ebnf(source).unwrap()).unwrap()
}

#[test]
fn handles_left_recursion_ambiguity_and_complete_input() {
    let grammar = recognizer(r#"Start ::= Start "+" Start | "x" ."#);
    for source in ["x", "x+x", "x+x+x"] {
        assert_eq!(grammar.recognizes("Start", source), Ok(true), "{source}");
    }
    for source in ["", "x+", "x x", "xyz", "x trailing"] {
        assert_eq!(grammar.recognizes("Start", source), Ok(false), "{source}");
    }
}

#[test]
fn handles_nullable_cycles_and_late_nullable_completion() {
    let grammar = recognizer(r#"Start ::= Empty Empty { [ "x" ] } . Empty ::= [ Empty ] ."#);
    for source in ["", "x", "x x x"] {
        assert_eq!(grammar.recognizes("Start", source), Ok(true));
    }
    assert_eq!(grammar.recognizes("Start", "y"), Ok(false));
    let grammar = recognizer(r#"Start ::= "x"+ ."#);
    assert_eq!(grammar.recognizes("Start", ""), Ok(false));
    assert_eq!(grammar.recognizes("Start", "x x"), Ok(true));
}

#[test]
fn errors_are_not_syntax_rejections() {
    let grammar = recognizer("Start ::= ? opaque syntax ? .");
    assert!(grammar
        .recognizes("Start", "input")
        .unwrap_err()
        .contains("unsupported"));
    assert!(grammar.recognizes("Missing", "").is_err());
    let grammar = recognizer(r#"Start ::= { "x" } ."#);
    assert!(grammar
        .recognizes_with_budget("Start", "x", 1)
        .unwrap_err()
        .contains("budget"));
    assert!(grammar.recognizes("Start", &"x".repeat(16_385)).is_err());
    assert!(Recognizer::new(&compile_ebnf("Start ::= Missing .").unwrap()).is_err());
}

#[test]
fn lexical_roots_use_the_grammar_and_preserve_token_boundaries() {
    let grammar = Recognizer::new(&canonical_terlan_syntax_contract().unwrap()).unwrap();
    for (rule, source) in [
        ("LowerIdent", "some_name9"),
        ("UpperIdent", "Thing"),
        ("Binding", "_value"),
        ("Int", "0xff"),
        ("Int", "0b101"),
        ("Int", "0o17"),
        ("Float", "1.25e-3"),
        ("Float", "12e2"),
        ("Expr", "0xFE+1"),
        ("Expr", "a <= b"),
        ("StringLiteral", r#""hello \"世界\"""#),
        ("Program", "module contract.Cases. // trailing\n"),
        (
            "Program",
            "/* before */ module /* between */ contract.Cases.",
        ),
    ] {
        assert_eq!(
            grammar.recognizes(rule, source),
            Ok(true),
            "{rule}: {source}"
        );
    }
    for (rule, source) in [
        ("LowerIdent", "module"),
        ("LowerIdent", "ab cd"),
        ("Binding", "_ value"),
        ("Int", "0b102"),
        ("Int", "1.0"),
        ("Int", "123abc"),
        ("Float", "1e+"),
        ("StringLiteral", r#""unterminated"#),
        ("Program", "modulecontract.Cases."),
        ("Expr", "a < = b"),
    ] {
        assert_eq!(
            grammar.recognizes(rule, source),
            Ok(false),
            "{rule}: {source}"
        );
    }
    let changed = recognizer(r#"Start ::= LowerIdent . LowerIdent ::= "only" ."#);
    assert_eq!(changed.recognizes("Start", "only"), Ok(true));
    assert_eq!(changed.recognizes("Start", "other"), Ok(false));
}

#[test]
fn unknown_raw_predicates_never_silently_reject_input() {
    let grammar = Recognizer::new(&canonical_terlan_syntax_contract().unwrap()).unwrap();
    assert!(grammar
        .recognizes("RawBlock", "{ anything }")
        .unwrap_err()
        .contains("unsupported"));
}
