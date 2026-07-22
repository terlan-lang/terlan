use super::format_source_module;

#[test]
fn formatter_canonicalizes_grouped_let_else_and_is_idempotent() {
    let source = r#"
module let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    let { Ok(left)<-first; Ok(right)<-second } else { Err(reason)->Err(reason) }; Ok(left+right).
"#;
    let expected = r#"module let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    let {
        Ok(left) <- first;
        Ok(right) <- second
    } else {
        Err(reason) -> Err(reason)
    };
    Ok(left + right).
"#;

    let formatted = format_source_module(source).expect("format grouped let else");
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source_module(&formatted).expect("format grouped let else twice"),
        expected
    );
}

#[test]
fn formatter_keeps_repeated_ordinary_lets_explicit() {
    let source = r#"
module nested_let_format.

pub total(first: Int, second: Int): Int ->
    let left = first; let right = second; left + right.
"#;
    let formatted = format_source_module(source).expect("format nested lets");

    assert!(formatted.contains("let left = first;\n    let right = second;\n    left + right."));
}
