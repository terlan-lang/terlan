use super::lex;
use crate::terlan_syntax::token::TokenKind;

#[test]
fn module_decl_dot_is_a_separator_not_identifier_char() {
    let src = "module mathx.\n";
    let tokens = lex(src).expect("lexer should parse module declaration");
    assert_eq!(tokens[0].text, "module");
    assert_eq!(tokens[1].text, "mathx");
    assert_eq!(tokens[2].text, ".");
    assert_eq!(tokens[3].text, "");
    assert_eq!(tokens[3].kind, TokenKind::EOF);
}

#[test]
fn doc_comments_are_distinct_tokens() {
    let src = "//! Module docs.\n/// Adds one.\nmodule mathx.\n";
    let tokens = lex(src).expect("lexer should parse doc comments");
    assert_eq!(tokens[0].kind, TokenKind::ModuleDocComment);
    assert_eq!(tokens[0].text, "Module docs.");
    assert_eq!(tokens[1].kind, TokenKind::DocComment);
    assert_eq!(tokens[1].text, "Adds one.");
}

#[test]
fn doc_block_comments_are_public_doc_tokens() {
    let src = "/**\n * Adds one.\n *\n * @param x The value.\n * @returns The incremented value.\n */\nmodule mathx.\n";
    let tokens = lex(src).expect("lexer should parse doc block comments");
    assert_eq!(tokens[0].kind, TokenKind::DocBlockComment);
    assert_eq!(
        tokens[0].text,
        "Adds one.\n\n@param x The value.\n@returns The incremented value."
    );
}

#[test]
fn line_and_block_comments_are_not_public_docs() {
    let src = "// implementation note\n/* implementation block */\nmodule mathx.\n";
    let tokens = lex(src).expect("lexer should parse implementation comments");
    assert_eq!(tokens[0].kind, TokenKind::Comment);
    assert_eq!(tokens[1].kind, TokenKind::Comment);
    assert_eq!(tokens[2].kind, TokenKind::Module);
}

#[test]
fn rejects_unterminated_doc_block_comments() {
    let errors = lex("/** missing close").expect_err("unterminated doc block");
    assert_eq!(
        errors[0].message,
        "unterminated documentation block comment"
    );
}

#[test]
fn rejects_nested_doc_block_comments() {
    let errors = lex("/** Outer /** Inner */ Outer */").expect_err("nested doc block");
    assert_eq!(
        errors[0].message,
        "nested documentation block comments are not supported"
    );
}

/// Verifies that exact inequality is tokenized as one comparison operator.
///
/// Inputs:
/// - A source fragment containing `=/=`.
///
/// Output:
/// - Test passes when the lexer emits `TokenKind::NotEqEq` for the exact
///   inequality operator.
///
/// Transformation:
/// - Runs the lexer over a short comparison expression and inspects the
///   middle token, guarding against splitting `=/=` into `=` plus `/=`.
#[test]
fn exact_inequality_is_one_token() {
    let tokens = lex("x =/= y").expect("lexer should parse exact inequality");

    assert_eq!(tokens[1].kind, TokenKind::NotEqEq);
    assert_eq!(tokens[1].text, "=/=");
}

/// Verifies that canonical inequality is tokenized as one comparison operator.
///
/// Inputs:
/// - A source fragment containing `!=`.
///
/// Output:
/// - Test passes when the lexer emits `TokenKind::NotEq` for the canonical
///   inequality operator.
///
/// Transformation:
/// - Runs the lexer over a short comparison expression and inspects the
///   middle token, guarding against treating `!` as unary syntax before `=`.
#[test]
fn canonical_inequality_is_one_token() {
    let tokens = lex("x != y").expect("lexer should parse canonical inequality");

    assert_eq!(tokens[1].kind, TokenKind::NotEq);
    assert_eq!(tokens[1].text, "!=");
}

/// Verifies that symbolic Boolean aliases are rejected.
///
/// Inputs:
/// - A source fragment containing `&&` and `||`.
///
/// Output:
/// - Test passes when both symbolic aliases fail lexing.
///
/// Transformation:
/// - Locks `and` and `or` as the only canonical Boolean operator spellings.
#[test]
fn symbolic_boolean_operators_are_rejected() {
    let and_errors = lex("a && b").expect_err("&& must be rejected");
    let or_errors = lex("a || b").expect_err("|| must be rejected");

    assert_eq!(
        and_errors[0].message,
        "symbolic Boolean operators are not supported; use `and`"
    );
    assert_eq!(
        or_errors[0].message,
        "symbolic Boolean operators are not supported; use `or`"
    );
}

/// Verifies scientific Float spellings remain one typed token.
#[test]
fn small_float_parity_scientific_literals_are_single_float_tokens() {
    let tokens = lex("1e3 2.5e-4 7E+8").expect("lexer should parse scientific floats");
    let numeric = tokens
        .iter()
        .filter(|token| token.kind != TokenKind::EOF)
        .map(|token| (token.kind.clone(), token.text.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(
        numeric,
        vec![
            (TokenKind::Float, "1e3"),
            (TokenKind::Float, "2.5e-4"),
            (TokenKind::Float, "7E+8"),
        ]
    );
}

/// Verifies incomplete exponents cannot be absorbed into numeric values.
#[test]
fn small_float_parity_malformed_scientific_exponents_remain_trailing_source() {
    for source in ["1e", "1e+", "2.0E-"] {
        let tokens = lex(source).expect("lexer should preserve malformed trailing source");
        assert!(matches!(tokens[0].kind, TokenKind::Int | TokenKind::Float));
        assert_ne!(tokens[0].text, source);
    }
}
