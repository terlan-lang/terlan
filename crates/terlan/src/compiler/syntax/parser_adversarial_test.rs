#[cfg(test)]
mod tests {
    use crate::terlan_syntax::{parse_module, parse_terlan_expr};

    /// Verifies malformed nesting fails as a parse error without panicking.
    ///
    /// Inputs:
    /// - Expressions with mismatched delimiters and unterminated control forms.
    ///
    /// Output:
    /// - Test passes when every input returns a parser error.
    ///
    /// Transformation:
    /// - Exercises the expression parser against adversarial delimiter shapes
    ///   that commonly expose cursor or recovery bugs.
    #[test]
    fn adversarial_expr_rejects_mismatched_or_unclosed_delimiters() {
        for source in [
            "((1 + 2]",
            "case value { Some(x) -> x",
            "if { ready -> run();",
            "Vector(1, 2, 3",
            "user.display_name(",
            "html { <div>",
        ] {
            let error = parse_terlan_expr(source).expect_err(source);
            assert!(
                !error.message.trim().is_empty(),
                "empty parse error for {source:?}"
            );
        }
    }

    /// Verifies large but bounded nested expressions remain supported.
    ///
    /// Inputs:
    /// - A parenthesized expression fixture inside the parser depth budget.
    ///
    /// Output:
    /// - Test passes when parsing succeeds without stack overflow or cursor
    ///   corruption.
    ///
    /// Transformation:
    /// - Builds a generated nesting fixture large enough to exercise repeated
    ///   recursive descent while keeping the test deterministic and fast.
    #[test]
    fn adversarial_expr_accepts_guarded_balanced_parentheses_without_panic() {
        let depth = 8;
        let source = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));

        parse_terlan_expr(&source).expect("guarded balanced expression should parse");
    }

    /// Verifies excessive nesting is rejected before recursive descent.
    ///
    /// Inputs:
    /// - A parenthesized expression fixture beyond the parser depth budget.
    ///
    /// Output:
    /// - Test passes when parsing returns a diagnostic instead of overflowing
    ///   the process stack.
    ///
    /// Transformation:
    /// - Guards the release parser against generated or malicious source that
    ///   attempts to exhaust recursive descent.
    #[test]
    fn adversarial_expr_rejects_excessive_parentheses_without_stack_overflow() {
        let depth = 256;
        let source = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));

        let error = parse_terlan_expr(&source).expect_err("excessive nesting should fail");
        assert!(
            error.message.contains("nesting depth"),
            "unexpected nesting error: {}",
            error.message
        );
    }

    /// Verifies SQL interpolation expressions inherit parser depth guards.
    ///
    /// Inputs:
    /// - A typed SQL form containing a deeply nested Terlan interpolation.
    ///
    /// Output:
    /// - Test passes when the SQL form returns the same parser nesting
    ///   diagnostic instead of recursing through the interpolation parser.
    ///
    /// Transformation:
    /// - Exercises the raw SQL scanner boundary where SQL text hands `${...}`
    ///   source back to normal Terlan expression parsing.
    #[test]
    fn adversarial_sql_interpolation_rejects_excessive_nested_expression() {
        let depth = 256;
        let interpolation = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
        let source = format!("sql[UserRow] {{select ${{{interpolation}}}}}");

        let error = parse_terlan_expr(&source).expect_err("excessive SQL nesting should fail");
        assert!(
            error.message.contains("nesting depth"),
            "unexpected SQL nesting error: {}",
            error.message
        );
    }

    /// Verifies Unicode text does not turn into Unicode identifiers.
    ///
    /// Inputs:
    /// - A module declaration containing non-ASCII identifier characters.
    /// - A valid source string containing Unicode text.
    ///
    /// Output:
    /// - Test passes when the identifier is rejected and the string payload is
    ///   still accepted.
    ///
    /// Transformation:
    /// - Guards the language boundary where Unicode is data, while identifiers
    ///   remain predictable ASCII source names.
    #[test]
    fn adversarial_module_rejects_unicode_identifiers_but_accepts_unicode_strings() {
        let error = parse_module("module café.Main.\n")
            .expect_err("Unicode module identifiers must not parse");
        assert!(!error.message.trim().is_empty());

        parse_module(
            r#"
module unicode_string_accept.

pub hello(): String ->
    "λ🔥".
"#,
        )
        .expect("Unicode string payload should parse");
    }

    /// Verifies removed Erlang source syntax remains rejected.
    ///
    /// Inputs:
    /// - Source using raw Erlang send/receive and binary pattern shapes.
    ///
    /// Output:
    /// - Test passes when canonical Terlan parsing rejects each module.
    ///
    /// Transformation:
    /// - Protects the source grammar from reintroducing BEAM-only syntax as a
    ///   side effect of backend work.
    #[test]
    fn adversarial_module_rejects_removed_erlang_source_grammar() {
        for source in [
            r#"
module adversarial.Send.

pub main(): Unit ->
    self() ! ok.
"#,
            r#"
module adversarial.Receive.

pub main(): Unit ->
    receive { ok -> Unit }.
"#,
            r#"
module adversarial.Binary.

pub decode(value: Binary): Int ->
    case value {
        <<SourcePort:16, Payload/binary>> -> SourcePort
    }.
"#,
        ] {
            let error = parse_module(source).expect_err("removed Erlang grammar parsed");
            assert!(
                !error.message.trim().is_empty(),
                "empty parse error for {source:?}"
            );
        }
    }

    /// Verifies declaration parsing rejects duplicate-looking syntax pivots.
    ///
    /// Inputs:
    /// - Struct-like and constructor-like declarations with invalid body
    ///   delimiters or misplaced field assignments.
    ///
    /// Output:
    /// - Test passes when each source fails during parsing.
    ///
    /// Transformation:
    /// - Guards the constructor-call pivot against accepting ambiguous
    ///   declaration forms that look valid but have no canonical meaning.
    #[test]
    fn adversarial_module_rejects_ambiguous_constructor_and_struct_shapes() {
        for source in [
            r#"
module adversarial.StructAssignment.

pub struct User {
    id = 1,
    name = "Ada"
}.
"#,
            r#"
module adversarial.ConstructorBody.

pub constructor User {
    id: Int,
    name: String
}.
"#,
        ] {
            let error = parse_module(source).expect_err("ambiguous declaration parsed");
            assert!(
                !error.message.trim().is_empty(),
                "empty parse error for {source:?}"
            );
        }
    }
}
