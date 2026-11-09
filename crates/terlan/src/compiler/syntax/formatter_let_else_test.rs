use super::format_source_module;

#[test]
fn formatter_canonicalizes_grouped_refutable_if_and_is_idempotent() {
    let source = r#"
module let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    if { Ok(left)<-first; Ok(right)<-second } else { Err(reason)->Err(reason) }; Ok(left+right).
"#;
    let expected = r#"module let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    if {
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

#[test]
fn formatter_keeps_explicit_fallback_in_grouped_if() {
    let source = r#"
module single_let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(value: Result[Int, String]): Result[Int, String] ->
    if { Ok(result) <- value } else { Err(reason) -> Err(reason) }; Ok(result).
"#;
    let expected = r#"module single_let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(value: Result[Int, String]): Result[Int, String] ->
    if {
        Ok(result) <- value
    } else {
        Err(reason) -> Err(reason)
    };
    Ok(result).
"#;

    let formatted = format_source_module(source).expect("format single refutable let");
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source_module(&formatted).expect("format single refutable let twice"),
        expected
    );
}

#[test]
fn formatter_omits_else_for_single_binding_without_fallback_and_is_idempotent() {
    let source = r#"
module let_without_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(value: Result[Int, String]): Result[Int, String] ->
    Ok(result) <- value; Ok(result).
"#;
    let expected = r#"module let_without_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(value: Result[Int, String]): Result[Int, String] ->
    Ok(result) <- value;
    Ok(result).
"#;

    let formatted = format_source_module(source).expect("format let without else");
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source_module(&formatted).expect("format let without else twice"),
        expected
    );
}

#[test]
fn formatter_rewrites_linear_case_pyramid_as_grouped_refutable_if() {
    let source = r#"
module grouped_case_format.

pub type Option[T] = Some[T] | None.

pub resolve(first: Option[Int], second: Option[Int]): Option[Int] ->
    case first {
        None -> None;
        Some(left) -> case second {
            None -> None;
            Some(right) -> Some(left + right)
        }
    }.
"#;
    let expected = r#"module grouped_case_format.

pub type Option[T] = Some[T] | None.

pub resolve(first: Option[Int], second: Option[Int]): Option[Int] ->
    if {
        Some(left) <- first;
        Some(right) <- second
    } else {
        _ -> None
    };
    Some(left + right).
"#;

    let formatted = format_source_module(source).expect("format linear case pyramid");
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source_module(&formatted).expect("format grouped case twice"),
        expected
    );
}

#[test]
fn formatter_preserves_priority_of_failure_before_an_overlapping_continuation() {
    let source = r#"
module overlapping_case_format.
pub check(first: Result[String, String], second: Result[Int, String]): Bool ->
    case first {
        Ok(_) -> true;
        _ -> case second { Ok(_) -> true; _ -> false }
    }.
"#;
    let formatted = format_source_module(source).expect("format overlapping case pyramid");
    assert!(formatted.contains("case first"));
    assert!(formatted.contains("case second"));
    assert!(!formatted.contains("_ <-"));
    assert_eq!(format_source_module(&formatted).unwrap(), formatted);
}

#[test]
fn formatter_keeps_failure_binding_dependent_case_pyramid_explicit() {
    let source = r#"
module dependent_case_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    case first {
        Err(reason) -> Err(reason);
        Ok(left) -> case second {
            Err(reason) -> Err(reason);
            Ok(right) -> Ok(left + right)
        }
    }.
"#;

    let formatted = format_source_module(source).expect("format dependent case pyramid");
    assert!(formatted.contains("case first"));
    assert!(formatted.contains("case second"));
    assert!(!formatted.contains("if {\n        Ok(left) <-"));
}

#[test]
fn formatter_rewrites_and_reparses_case_pyramid_inside_lambda() {
    let source = r#"
module grouped_lambda_case_format.

pub type Option[T] = Some[T] | None.

pub resolve(values: List[Option[Int]]): List[Option[Int]] ->
    values.map((first) ->
        case first {
            None -> None;
            Some(left) -> case first {
                None -> None;
                Some(right) -> Some(left + right)
            }
        }
    ).
"#;

    let formatted = format_source_module(source).expect("format lambda case pyramid");
    assert!(formatted.contains("Some(left) <- first;"));
    assert!(formatted.contains("Some(right) <- first"));
    assert_eq!(
        format_source_module(&formatted).expect("reparse formatted lambda grouped refutable if"),
        formatted
    );
}

#[test]
fn formatter_rewrites_forwarding_lambda_as_direct_function_reference() {
    let source = r#"
module function_reference_format.

pub generated(value: Int): Bool -> value >= 0.

pub property(): Bool ->
    for_all(int_range(-2, 0), (width) -> generated(width)).
"#;
    let expected = r#"module function_reference_format.

pub generated(value: Int): Bool ->
    value >= 0.

pub property(): Bool ->
    for_all(int_range(-2, 0), generated).
"#;

    let formatted = format_source_module(source).expect("format forwarding lambda");
    assert_eq!(formatted, expected);
    assert_eq!(
        format_source_module(&formatted).expect("format direct function reference twice"),
        expected
    );
}

#[test]
fn formatter_keeps_lambda_that_transforms_its_argument() {
    let source = r#"
module transformed_lambda_format.

pub generated(value: Int): Bool -> value >= 0.

pub property(): Bool ->
    for_all(int_range(-2, 0), (width) -> generated(width + 1)).
"#;

    let formatted = format_source_module(source).expect("format transformed lambda");
    assert!(formatted.contains("(width) -> generated(width + 1)"));
}

#[test]
fn formatter_keeps_discard_as_a_refutable_pattern() {
    let source = r#"
module discard_format.

pub sequence(value: Int): Int ->
    _ <- consume(value);
    value.
"#;

    let formatted = format_source_module(source).expect("format discard pattern");
    assert!(formatted.contains("_ <- consume(value);"));
    assert!(!formatted.contains("let _ = consume(value)"));
    assert_eq!(
        format_source_module(&formatted).expect("format discard pattern twice"),
        formatted
    );
}
