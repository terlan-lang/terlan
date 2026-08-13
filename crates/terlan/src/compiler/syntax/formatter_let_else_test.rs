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

#[test]
fn formatter_canonicalizes_single_refutable_let_without_grouping_braces() {
    let source = r#"
module single_let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(value: Result[Int, String]): Result[Int, String] ->
    let { Ok(result) <- value } else { Err(reason) -> Err(reason) }; Ok(result).
"#;
    let expected = r#"module single_let_else_format.

pub type Result[T, E] = Ok[T] | Err[E].

pub resolve(value: Result[Int, String]): Result[Int, String] ->
    let Ok(result) <- value else {
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
fn formatter_rewrites_linear_case_pyramid_as_grouped_let_else() {
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
    let {
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
    assert!(!formatted.contains("let {"));
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
        format_source_module(&formatted).expect("reparse formatted lambda grouped let"),
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
