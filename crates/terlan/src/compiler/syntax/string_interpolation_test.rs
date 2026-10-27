use super::*;

#[test]
fn string_interpolation_parses_nested_expressions_strings_and_comments() {
    for source in [
        r#""Count: ${count + 1}""#,
        r#""${String.append("a", "}")}""#,
        r#""${"nested ${1 + 2}"}""#,
        "\"${1 /* } */ + 2}\"",
        "\"${1 // }\n + 2}\"",
        r#""${case value { Some(x) -> x; None -> 0 }}""#,
    ] {
        let expression = super::super::parser::parse_terlan_expr(source).unwrap();
        assert!(
            matches!(expression, Expr::InterpolatedString { .. }),
            "{source}"
        );
    }
}

#[test]
fn string_interpolation_rejects_empty_invalid_and_unclosed_expressions() {
    for source in [r#""${}""#, r#""${ }""#, r#""${1 +}""#, r#""${value""#] {
        assert!(
            super::super::parser::parse_terlan_expr(source).is_err(),
            "{source}"
        );
    }
}

#[test]
fn string_interpolation_preserves_escapes_and_does_not_reparse_literal_dollars() {
    for (source, expected, holes) in [
        (r#""\${count}""#, r#""${count}""#, 0),
        (r#""\\${count}""#, r#""\\""#, 1),
        (r#""é\n\${count} ${count}""#, r#""é\n${count} ""#, 1),
    ] {
        let Expr::InterpolatedString {
            segments,
            expressions,
        } = parse(source).unwrap()
        else {
            panic!("{source}");
        };
        assert_eq!(expressions.len(), holes);
        assert_eq!(literal(&segments[0]), expected);
    }
    assert!(matches!(
        parse(r#""plain {text} $count""#).unwrap(),
        Expr::Binary(_)
    ));
}

#[test]
fn string_interpolation_formatter_round_trips_nested_quotes_and_references() {
    let source = r#"module interpolation. import std.core.Int.
pub text(n: Int): String -> "é ${Int.to_string(n)} ${"inner ${n}"} \${literal}".
"#;
    let formatted = super::super::format_source_module(source).unwrap();
    assert!(formatted.contains("${"));
    assert!(formatted.contains(r#"\${literal}"#));
    assert_eq!(
        formatted,
        super::super::format_source_module(&formatted).unwrap()
    );
    super::super::parser::parse_module(&formatted).unwrap();
}

#[test]
fn string_interpolation_lowers_to_bound_show_calls_in_source_order() {
    let output =
        super::super::parse_expr_as_syntax_output(r#""a${first()}b${second()}c""#).unwrap();
    assert_eq!(output.remote.as_deref(), Some("std.core.String"));
    assert_eq!(output.children[0].text.as_deref(), Some("concat"));
    let parts = &output.children[1].children;
    assert_eq!(parts.len(), 5);
    for (index, callee) in [(1, "first"), (3, "second")] {
        assert_eq!(parts[index].remote.as_deref(), Some("std.core.String"));
        assert_eq!(parts[index].children[0].text.as_deref(), Some("show"));
        assert_eq!(
            parts[index].children[1].children[0].text.as_deref(),
            Some(callee)
        );
    }
}
