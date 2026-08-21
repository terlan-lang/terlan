use crate::terlan_syntax::parse_tree::{Decl, Expr, Pattern};
use crate::terlan_syntax::{
    parse_module, COMMA_GROUPED_LET_BINDING_DIAGNOSTIC, REPEATED_LET_BINDING_DIAGNOSTIC,
};

#[test]
fn parses_repeated_let_bindings_in_source_order() {
    let source = r#"
module repeated_let_bindings.

pub total(price: Int, tax: Int): Int ->
    let subtotal = price;
    let total = subtotal + tax;
    total.
"#;

    let module = parse_module(source).expect("repeated lets should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    let Expr::Let {
        bindings,
        else_clauses,
        body,
    } = &function.clauses[0].body
    else {
        panic!("expected let expression");
    };
    assert_eq!(bindings.len(), 2);
    assert!(else_clauses.is_empty());
    assert!(matches!(body.as_deref(), Some(Expr::Var(name)) if name == "total"));
}

#[test]
fn non_finite_float_spellings_remain_let_binding_identifiers() {
    let source = r#"
module non_finite_identifier_bindings.

pub retain(value: Int): Int ->
    let infinity = value;
    let inf = infinity;
    let nan = inf;
    let nanoseconds = nan;
    let infimum = nanoseconds;
    infimum.
"#;

    let module = parse_module(source).expect("identifier bindings should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    let Expr::Let { bindings, .. } = &function.clauses[0].body else {
        panic!("expected let expression");
    };
    let names = bindings
        .iter()
        .map(|binding| match &binding.pattern {
            Pattern::Var(name) => name.as_str(),
            pattern => panic!("expected variable pattern, found {pattern:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(names, ["infinity", "inf", "nan", "nanoseconds", "infimum"]);
}

#[test]
fn rejects_implicit_subsequent_let_binding() {
    let source = r#"
module implicit_let_binding.

pub total(price: Int, tax: Int): Int ->
    let subtotal = price; total = subtotal + tax; total.
"#;

    let error = parse_module(source).expect_err("implicit binding should fail");
    assert_eq!(error.message, REPEATED_LET_BINDING_DIAGNOSTIC);
}

#[test]
fn rejects_comma_grouped_let_bindings() {
    let source = r#"
module comma_grouped_let.

pub total(price: Int, tax: Int): Int ->
    let subtotal = price, total = subtotal + tax; total.
"#;

    let error = parse_module(source).expect_err("comma-grouped binding should fail");
    assert_eq!(error.message, COMMA_GROUPED_LET_BINDING_DIAGNOSTIC);
}

#[test]
fn accepts_commas_and_equality_inside_let_bound_comprehensions() {
    let source = r#"
module comprehension_let_binding.

pub pairs(left: List[Int], right: List[Int]): List[{Int, Int}] ->
    let selected = [{x, y} | x <- left, y <- right, x == y];
    selected.
"#;

    parse_module(source).expect("comprehension punctuation belongs to the binding value");
}

#[test]
fn parses_grouped_refutable_bindings_with_shared_else() {
    let source = r#"
module grouped_let_else.

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    let {
        Ok(left) <- first;
        Ok(right) <- second
    }
    else {
        Err(reason) -> Err(reason)
    };
    Ok(left + right).
"#;

    let module = parse_module(source).expect("grouped let else should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    let Expr::Let {
        bindings,
        else_clauses,
        body,
    } = &function.clauses[0].body
    else {
        panic!("expected let expression");
    };
    assert_eq!(bindings.len(), 2);
    assert_eq!(else_clauses.len(), 1);
    assert!(matches!(body.as_deref(), Some(Expr::Call { .. })));
}

#[test]
fn parses_grouped_refutable_bindings_with_multi_argument_calls() {
    let source = r#"
module grouped_let_call_values.

pub resolve(value: Dynamic): Option[Int] ->
    let {
        Some(left) <- read_int(value, "left");
        Some(right) <- read_int(value, "right")
    } else {
        _ -> None
    };
    Some(left + right).
"#;

    let generated = super::super::lalrpop_boundary::parse_lalrpop_module_syntax(source);
    assert!(
        generated.is_ok(),
        "generated boundary rejected grouped calls: {generated:?}"
    );
    let module = parse_module(source).expect("call-valued grouped let else should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    let Expr::Let { bindings, .. } = &function.clauses[0].body else {
        panic!("expected grouped let expression");
    };
    assert_eq!(bindings.len(), 2);
}

#[test]
fn parses_grouped_refutable_bindings_inside_lambda_body() {
    let source = r#"
module lambda_grouped_let_else.

pub resolve(values: List[Dynamic]): List[Option[Int]] ->
    values.map((value) ->
        let {
            Some(left) <- read_int(value, "left");
            Some(right) <- read_int(value, "right")
        } else {
            _ -> None
        };
        Some(left + right)
    ).
"#;

    parse_module(source).expect("grouped let else should parse inside lambda body");
}

#[test]
fn repeated_let_else_attaches_only_to_the_nested_binding() {
    let source = r#"
module nested_let_else.

pub resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    let first_result = first;
    let {
        Ok(right) <- second
    } else {
        Err(reason) -> Err(reason)
    };
    Ok(right).
"#;

    let module = parse_module(source).expect("nested let else should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    let Expr::Let {
        else_clauses, body, ..
    } = &function.clauses[0].body
    else {
        panic!("expected outer let expression");
    };
    assert!(else_clauses.is_empty());
    assert!(matches!(
        body.as_deref(),
        Some(Expr::Let { else_clauses, .. }) if else_clauses.len() == 1
    ));
}

#[test]
fn rejects_grouped_let_else_without_fallback_clauses() {
    let source = r#"
module empty_let_else.

pub resolve(value: Dynamic): Dynamic ->
    let {
        Ok(result) <- value
    } else {};
    result.
"#;

    let error = parse_module(source).expect_err("empty let else should fail");
    assert_eq!(
        error.message,
        "let else requires at least one fallback clause"
    );
}

#[test]
fn rejects_empty_refutable_let_group_with_stable_diagnostic() {
    let source = r#"
module empty_refutable_let_group.

pub resolve(): Int ->
    let {} else {
        _ -> 0
    };
    1.
"#;

    let error = parse_module(source).expect_err("empty refutable let group should fail");
    assert_eq!(
        error.message,
        "refutable let group requires at least one binding"
    );
}

#[test]
fn rejects_refutable_let_group_without_else() {
    let source = r#"
module missing_refutable_let_else.

pub resolve(value: Dynamic): Dynamic ->
    let {
        Ok(result) <- value
    };
    result.
"#;

    let error = parse_module(source).expect_err("refutable let group without else should fail");
    assert_eq!(
        error.message,
        "refutable let group requires an else fallback"
    );
}

#[test]
fn accepts_trailing_semicolons_in_refutable_let_blocks() {
    let source = r#"
module trailing_refutable_let_semicolons.

pub resolve(value: Dynamic): Dynamic ->
    let {
        Ok(result) <- value;
    } else {
        Err(reason) -> reason;
    };
    result.
"#;

    parse_module(source).expect("trailing semicolons should parse");
}

#[test]
fn accepts_comments_at_refutable_let_group_boundaries() {
    let source = r#"
module commented_refutable_let.

pub resolve(value: Dynamic): Dynamic ->
    let {
        // Inspect the result without exposing a partial binding.
        Ok(result) <- value
    }
    // All mismatches share this fallback.
    else {
        Err(reason) -> reason
    }
    // The success continuation starts after the fallback.
    ;
    result.
"#;

    parse_module(source).expect("comments at refutable let boundaries should parse");
}

/// Verifies implementation comments may introduce a callable body.
///
/// Inputs:
/// - A function with an ordinary line comment immediately after its arrow.
///
/// Output:
/// - A successfully parsed module.
///
/// Transformation:
/// - Confirms body parsing skips non-documentation commentary before routing
///   the first expression through the precedence parser.
#[test]
fn accepts_comment_before_first_body_expression() {
    let source = r#"
module commented_body_start.

pub answer(): Int ->
    // Preserve implementation intent next to the expression it describes.
    let value = 42;
    value.
"#;

    parse_module(source).expect("comment before first body expression should parse");
}

#[test]
fn rejects_grouped_let_else_without_success_result() {
    let source = r#"
module bodyless_let_else.

pub resolve(value: Dynamic): Dynamic ->
    let {
        Ok(result) <- value
    } else {
        Err(reason) -> reason
    }.
"#;

    let error = parse_module(source).expect_err("bodyless let else should fail");
    assert!(
        error.message.contains("expected Semicolon"),
        "unexpected diagnostic: {error:?}"
    );
}

#[test]
fn rejects_comma_grouped_refutable_bindings() {
    let source = r#"
module comma_grouped_let_else.

pub resolve(first: Dynamic, second: Dynamic): Dynamic ->
    let {
        Ok(left) <- first, Ok(right) <- second
    } else {
        Err(reason) -> reason
    };
    {left, right}.
"#;

    let error = parse_module(source).expect_err("comma-grouped let else should fail");
    assert_eq!(error.message, COMMA_GROUPED_LET_BINDING_DIAGNOSTIC);
}

#[test]
fn ordinary_tuple_destructuring_is_not_parsed_as_a_refutable_group() {
    let source = r#"
module tuple_let.

pub add(pair: {Int, Int}): Int ->
    let {left, right} = pair;
    left + right.
"#;

    let module = parse_module(source).expect("ordinary tuple let should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert!(matches!(
        &function.clauses[0].body,
        Expr::Let { else_clauses, .. } if else_clauses.is_empty()
    ));
}

#[test]
fn parses_single_unbraced_refutable_let_form() {
    let source = r#"
module unbraced_let_else.

pub resolve(value: Dynamic): Dynamic ->
    let Ok(result) <- value else {
        _ -> 0
    };
    result.
"#;

    let module = parse_module(source).expect("single unbraced refutable let should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    let Expr::Let {
        bindings,
        else_clauses,
        body,
    } = &function.clauses[0].body
    else {
        panic!("expected refutable let expression");
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(else_clauses.len(), 1);
    assert!(matches!(body.as_deref(), Some(Expr::Var(name)) if name == "result"));
}
