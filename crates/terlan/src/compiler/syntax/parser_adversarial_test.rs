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
