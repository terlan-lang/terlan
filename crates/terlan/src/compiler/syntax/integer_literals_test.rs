use super::*;

#[test]
fn separators_preserve_values_across_radices_and_bounds() {
    for (text, value) in [
        ("1_700_000_000_000_000_000", 1_700_000_000_000_000_000),
        ("0b1010_0011", 163),
        ("0o1_777", 1023),
        ("0xFF_FF", 65535),
        ("9_223_372_036_854_775_807", i64::MAX),
        ("-9_223_372_036_854_775_808", i64::MIN),
    ] {
        assert_eq!(parse_integer(text), Some(value), "{text}");
    }
    assert_eq!(parse_integer("9_223_372_036_854_775_808"), None);
}

#[test]
fn malformed_separators_and_invalid_radix_digits_are_rejected() {
    for text in [
        "_1", "1_", "1__0", "0x_FF", "0b_1", "0o7_", "0b1_2", "0xF_G", "", "1_000.5", "1e1_0",
    ] {
        assert_eq!(normalize_integer(text), None, "{text}");
    }
}

#[test]
fn decimal_grouping_is_stable_and_handles_negative_extremes() {
    for value in [
        0,
        999,
        1000,
        -1000,
        1_700_000_000_000_000_000,
        i64::MAX,
        i64::MIN,
    ] {
        let text = format_integer(value);
        assert_eq!(parse_integer(&text), Some(value));
        assert_eq!(format_integer(parse_integer(&text).unwrap()), text);
    }
    assert_eq!(
        format_integer(1_700_000_000_000_000_000),
        "1_700_000_000_000_000_000"
    );
}

#[test]
fn parser_and_formatter_share_separator_rules() {
    use crate::terlan_syntax::{format_source_module, parse_terlan_expr, parse_tree::Expr};
    for (literal, expected) in [
        ("1_700_000_000_000_000_000", 1_700_000_000_000_000_000),
        ("0b1010_0011", 163),
        ("0o1_777", 1023),
        ("0xFF_FF", 65535),
    ] {
        assert!(
            matches!(parse_terlan_expr(literal), Ok(Expr::Int(value)) if value == expected),
            "{literal}"
        );
    }
    for literal in ["1_", "1__0", "0x_FF", "0b1_2", "0o7_", "1_000.5"] {
        assert!(parse_terlan_expr(literal).is_err(), "{literal}");
    }
    let source = "module integers.Main.\npub value(): Int -> 1700000000000000000.\n";
    let formatted = format_source_module(source).unwrap();
    assert!(formatted.contains("1_700_000_000_000_000_000"));
    assert_eq!(format_source_module(&formatted).unwrap(), formatted);
    let protected = format_source_module(
        "module integers.Strings.\npub value(): String -> \"1700000000000000000\".\n",
    )
    .unwrap();
    assert!(protected.contains("\"1700000000000000000\""));
    let array = format_source_module(
        "module integers.Array.\npub keep(value: Array[10000, Int]): Array[10000, Int] -> value.\n",
    )
    .unwrap();
    assert!(array.contains("Array[10_000, Int]"));
    assert_eq!(format_source_module(&array).unwrap(), array);
}

#[test]
fn integer_type_formatting_preserves_literal_strings_and_comments() {
    assert_eq!(
        format_type_integers("{Array[10000, Int], Atom[\"10000\"]}"),
        "{Array[10_000, Int], Atom[\"10000\"]}"
    );
    assert_eq!(
        format_type_integers("Array[10000, Int] /* 10000 */"),
        "Array[10_000, Int] /* 10000 */"
    );
}
