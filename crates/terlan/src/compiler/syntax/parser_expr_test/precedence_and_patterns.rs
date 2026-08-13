use crate::terlan_syntax::parse_tree::{Decl, Expr, Pattern, StringPatternSegment};
use crate::terlan_syntax::{parse_module, parse_terlan_expr};

#[test]
fn formal_expr_precedence_keeps_pipe_below_boolean_chain() {
    let expr = parse_terlan_expr("A |> B + C * D or Ready").expect("parse formal precedence");
    let Expr::BinaryOp { op, right, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));

    let Expr::BinaryOp { op, left, .. } = right.as_ref() else {
        panic!("expected or expression on pipe right side");
    };
    assert!(matches!(op, crate::terlan_syntax::parse_tree::BinaryOp::Or));

    let Expr::BinaryOp { op, right, .. } = left.as_ref() else {
        panic!("expected additive expression on or left side");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::Add
    ));
    assert!(matches!(
        right.as_ref(),
        Expr::BinaryOp {
            op: crate::terlan_syntax::parse_tree::BinaryOp::Mul,
            ..
        }
    ));
}

/// Verifies range membership parses as `value in (start..end)`.
///
/// Inputs:
/// - A source expression using the inclusive range membership surface.
///
/// Output:
/// - Test passes when the parser preserves `in` outside a nested range
///   expression.
///
/// Transformation:
/// - Locks precedence so guard expressions such as `status in 200..299`
///   keep the range expression on the right side of membership.
#[test]
fn formal_expr_parses_range_membership_with_range_precedence() {
    let expr = parse_terlan_expr("status in 200..299").expect("parse range membership");
    let Expr::BinaryOp { op, right, .. } = expr else {
        panic!("expected membership binary op");
    };
    assert!(matches!(op, crate::terlan_syntax::parse_tree::BinaryOp::In));
    assert!(matches!(
        right.as_ref(),
        Expr::BinaryOp {
            op: crate::terlan_syntax::parse_tree::BinaryOp::Range,
            ..
        }
    ));
}

/// Verifies bare `=` remains illegal as expression syntax.
///
/// Inputs:
/// - A source expression shaped like Erlang-style match assignment.
///
/// Output:
/// - Test passes when parsing rejects the expression with guidance for
///   binding, equality, pattern matching, and indexed updates.
///
/// Transformation:
/// - Locks Terlan's pattern-matching contract to `case`, `let`, and
///   callable parameter positions instead of accepting bare match
///   expressions.
#[test]
fn formal_expr_rejects_bare_match_assignment_with_guidance() {
    let error = parse_terlan_expr("a = a + 1").expect_err("bare match assignment parsed");
    assert!(error.message.contains("plain `=` is not assignment"));
    assert!(error.message.contains("use `let name = value` to bind"));
    assert!(error.message.contains("`==` to compare"));
    assert!(error.message.contains("`case` to match shapes"));
    assert!(error
        .message
        .contains("`collection[index] = value` for indexed collection updates"));
}

/// Verifies `if` fallback clauses accept `_` without allowing wildcard as
/// a general expression.
///
/// Inputs:
/// - An `if` expression with `_ ->` as the final clause.
/// - A standalone wildcard expression.
///
/// Output:
/// - Test passes when the `if` fallback parses and standalone `_` remains
///   rejected.
///
/// Transformation:
/// - Confirms the parser normalizes `_` only in `if` clause-head position.
#[test]
fn if_expr_accepts_wildcard_fallback_clause_only_in_clause_head() {
    let expr = parse_terlan_expr("if { ready -> 1; _ -> 0 }").expect("parse if fallback clause");
    let Expr::If { clauses } = expr else {
        panic!("expected if expression");
    };
    assert_eq!(clauses.len(), 2);
    assert!(matches!(&clauses[1].condition, Expr::Var(name) if name == "true"));

    let error = parse_terlan_expr("_").expect_err("standalone wildcard should fail");
    assert!(error
        .message
        .contains("wildcard '_' is only valid in pattern position"));
}

/// Verifies module function bodies accept `_ ->` fallback clauses in nested
/// `if` expressions.
///
/// Inputs:
/// - Source module containing nested `if { ... }` expressions with `_`
///   fallback clauses.
///
/// Output:
/// - Test passes when the full module parser accepts both fallback clauses.
///
/// Transformation:
/// - Exercises the same parser entry point used by `terlc build` instead
///   of only the isolated expression parser.
#[test]
fn module_if_expr_accepts_nested_wildcard_fallback_clauses() {
    parse_module(
        r#"
module test.Other.

pub binary_search_range(items: Vector<Int>, target: Int, low: Int, high: Int): Option<Int> ->
    if {
      low > high -> None;
      _ ->
        let mid = low + ((high - low) / 2);
        let value = items[mid];
        case value == target {
          true -> Some(mid);
          false ->
            if {
              value < target -> binary_search_range(items, target, mid + 1, high);
              _ -> binary_search_range(items, target, low, mid - 1)
            }
        }
    }.
"#,
    )
    .expect("parse nested if wildcard fallbacks");
}

/// Verifies string-pattern capture syntax parses in case clause heads.
///
/// Inputs:
/// - A case expression whose pattern uses the planned `${name: Type}`
///   string capture surface.
///
/// Output:
/// - Parsed segmented string pattern with typed capture metadata.
///
/// Transformation:
/// - Confirms capture-bearing strings are represented separately from exact
///   literal string patterns.
#[test]
fn parses_string_capture_pattern_in_case_clause() {
    let module = parse_module(
        r#"
module string_pattern_case_capture.

pub match_path(path: String): Int ->
    case path {
        "test/${id: Id}.txt" -> 1;
        _ -> 0
    }.
"#,
    )
    .expect("parse string capture pattern in case");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function");
    };
    let Expr::Case { clauses, .. } = &function.clauses[0].body else {
        panic!("expected case expression");
    };
    assert_typed_string_capture_pattern(&clauses[0].pattern, "test/", "id", "Id", ".txt");
}

/// Verifies string patterns parse in let destructuring.
///
/// Inputs:
/// - A let expression whose binding pattern is a planned string-pattern
///   capture.
///
/// Output:
/// - Parsed let binding with a string capture pattern.
///
/// Transformation:
/// - Confirms let binding-start detection routes string tokens through the
///   pattern parser and keeps capture metadata.
#[test]
fn parses_string_capture_pattern_in_let_binding() {
    let module = parse_module(
        r#"
module string_pattern_let_capture.

pub match_path(path: String): Int ->
    let "test/${id: Id}.txt" = path;
    1.
"#,
    )
    .expect("parse let string capture pattern");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function");
    };
    let Expr::Let { bindings, .. } = &function.clauses[0].body else {
        panic!("expected let expression");
    };
    assert_typed_string_capture_pattern(&bindings[0].pattern, "test/", "id", "Id", ".txt");
}

/// Verifies string patterns parse in function clause heads.
///
/// Inputs:
/// - A declared function with an implementation clause using the planned
///   string-pattern capture surface.
///
/// Output:
/// - Parsed function-head string capture pattern.
///
/// Transformation:
/// - Keeps function-head and case/let pattern parsing aligned through the
///   generic pattern parser.
#[test]
fn parses_string_capture_pattern_in_function_clause() {
    let module = parse_module(
        r#"
module string_pattern_function_capture.

pub match_path(path: String): Int.
match_path("test/${id: Id}.txt") ->
    1.
"#,
    )
    .expect("parse function-head string capture pattern");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function");
    };
    assert_typed_string_capture_pattern(
        &function.clauses[0].patterns[0],
        "test/",
        "id",
        "Id",
        ".txt",
    );
}

/// Verifies string patterns parse in lambda parameter positions.
///
/// Inputs:
/// - A lambda expression whose parameter uses the planned string-pattern
///   capture surface.
///
/// Output:
/// - Parsed lambda clause with a segmented string pattern.
///
/// Transformation:
/// - Confirms anonymous function parameters use the same generic pattern
///   parser as `case`, `let`, and named function-head clauses.
#[test]
fn parses_string_capture_pattern_in_lambda_parameter() {
    let expr = parse_terlan_expr(r#"("test/${id: Id}.txt") -> 1"#)
        .expect("parse lambda string capture pattern");
    let Expr::Fun { clauses } = expr else {
        panic!("expected lambda expression");
    };
    assert_typed_string_capture_pattern(&clauses[0].patterns[0], "test/", "id", "Id", ".txt");
}

/// Verifies Elixir-style string interpolation is explicitly rejected for
/// Terlan string patterns.
///
/// Inputs:
/// - A case pattern using `#{...}` inside a string literal.
///
/// Output:
/// - Stable parser diagnostic pointing users at `${...}`.
///
/// Transformation:
/// - Prevents the reserved string-pattern feature from inheriting
///   Elixir/Erlang interpolation spelling.
#[test]
fn rejects_elixir_style_string_capture_pattern() {
    let err = parse_module(
        r#"
module string_pattern_elixir_capture_rejected.

pub match_path(path: String): Int ->
    case path {
        "test/#{id}.txt" -> 1;
        _ -> 0
    }.
"#,
    )
    .expect_err("Elixir-style string capture pattern should be rejected");
    assert_eq!(
        err.message,
        "string patterns use `${...}` captures; `#{...}` is not Terlan syntax"
    );
}

/// Verifies adjacent string captures are rejected before type checking.
///
/// Inputs:
/// - A string pattern with two captures and no literal separator.
///
/// Output:
/// - Stable parser diagnostic explaining that a separator is required.
///
/// Transformation:
/// - Keeps string capture matching deterministic before CoreIR or VM
///   execution sees the pattern.
#[test]
fn rejects_adjacent_string_capture_patterns() {
    let err = parse_module(
        r#"
module string_pattern_adjacent_capture_rejected.

pub match_path(path: String): Int ->
    case path {
        "${prefix}${suffix}" -> 1;
        _ -> 0
    }.
"#,
    )
    .expect_err("adjacent string captures should be rejected");
    assert_eq!(
        err.message,
        "adjacent string captures require a literal separator"
    );
}

/// Verifies unterminated string captures are rejected during parsing.
///
/// Inputs:
/// - A string pattern with `${...` and no closing brace.
///
/// Output:
/// - Stable parser diagnostic for the malformed capture.
///
/// Transformation:
/// - Prevents malformed capture syntax from becoming a generic string
///   literal or leaking to later compiler phases.
#[test]
fn rejects_unterminated_string_capture_patterns() {
    let err = parse_module(
        r#"
module string_pattern_unterminated_capture_rejected.

pub match_path(path: String): Int ->
    case path {
        "users/${id.txt" -> 1;
        _ -> 0
    }.
"#,
    )
    .expect_err("unterminated string captures should be rejected");
    assert_eq!(err.message, "unterminated string capture pattern");
}

/// Verifies empty string captures are rejected during parsing.
///
/// Inputs:
/// - A string pattern containing `${}`.
///
/// Output:
/// - Stable parser diagnostic for an empty capture slot.
///
/// Transformation:
/// - Requires every capture to declare a real binding name before the
///   typechecker assigns capture types.
#[test]
fn rejects_empty_string_capture_patterns() {
    let err = parse_module(
        r#"
module string_pattern_empty_capture_rejected.

pub match_path(path: String): Int ->
    case path {
        "users/${}.txt" -> 1;
        _ -> 0
    }.
"#,
    )
    .expect_err("empty string captures should be rejected");
    assert_eq!(err.message, "empty string capture pattern");
}

/// Verifies string-pattern captures parse inside nested tuple, list, and
/// keyed-map pattern positions.
///
/// Inputs:
/// - Case patterns that embed the planned `${...}` capture syntax inside
///   ordinary pattern containers.
///
/// Output:
/// - Parsed nested patterns with capture-bearing string children.
///
/// Transformation:
/// - Confirms capture-bearing strings are a generic pattern family rather
///   than only top-level case, let, and function-head syntax.
#[test]
fn parses_string_capture_pattern_in_nested_patterns() {
    for source in [
        r#"
module string_pattern_nested_tuple_capture.

pub match_path(value: Dynamic): Int ->
    case value {
        {"route", "test/${id: Id}.txt"} -> 1;
        _ -> 0
    }.
"#,
        r#"
module string_pattern_nested_list_capture.

pub match_path(value: Dynamic): Int ->
    case value {
        ["test/${id: Id}.txt"] -> 1;
        _ -> 0
    }.
"#,
        r#"
module string_pattern_nested_map_capture.

pub match_path(value: Dynamic): Int ->
    case value {
        {path: "test/${id: Id}.txt"} -> 1;
        _ -> 0
    }.
"#,
    ] {
        parse_module(source).expect("nested string capture pattern should parse");
    }
}

/// Verifies descriptor-backed binary layouts parse as expression syntax.
///
/// Inputs:
/// - A canonical `Binary[big] { ... }` expression using `UInt` and `Rest`.
///
/// Output:
/// - Parsed binary layout expression with endian policy and ordered fields.
///
/// Transformation:
/// - Reserves Terlan-native binary layout syntax without accepting Erlang
///   `<<...>>` forms or lowering the layout to runtime execution.
#[test]
fn parses_binary_layout_expression_scaffold() {
    let expr = parse_terlan_expr("Binary[big] { source_port: UInt[16], payload: Rest }")
        .expect("parse binary layout expression");

    let Expr::BinaryLayout { endian, fields } = expr else {
        panic!("expected binary layout expression");
    };
    assert_eq!(endian, "big");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name, "source_port");
    assert_eq!(fields[0].descriptor.text, "UInt[16]");
    assert_eq!(fields[1].name, "payload");
    assert_eq!(fields[1].descriptor.text, "Rest");
}

/// Verifies descriptor-backed binary layouts parse in function-head
/// pattern position.
#[test]
fn parses_binary_layout_function_head_pattern_scaffold() {
    let module = parse_module(
        r#"
module binary_layout_pattern.

decode(Binary[little] { opcode: UInt[8], payload: Rest }): Int ->
    1.
"#,
    )
    .expect("parse binary layout pattern");

    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function");
    };
    let Pattern::BinaryLayout { endian, fields } = &function.clauses[0].patterns[0] else {
        panic!("expected binary layout pattern");
    };
    assert_eq!(endian, "little");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].descriptor.text, "UInt[8]");
    assert_eq!(fields[1].descriptor.text, "Rest");
}

/// Verifies descriptor-backed binary layouts parse in case pattern position.
#[test]
fn parses_binary_layout_case_pattern_scaffold() {
    let module = parse_module(
        r#"
module binary_layout_case_pattern.

decode(packet: Dynamic): Int ->
    case packet {
        Binary[big] { flags: UInt[8], payload: Rest } -> 1;
        _ -> 0
    }.
"#,
    )
    .expect("parse binary layout case pattern");

    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function");
    };
    let Expr::Case { clauses, .. } = &function.clauses[0].body else {
        panic!("expected case expression");
    };
    let Pattern::BinaryLayout { endian, fields } = &clauses[0].pattern else {
        panic!("expected binary layout case pattern");
    };
    assert_eq!(endian, "big");
    assert_eq!(fields[0].name, "flags");
    assert_eq!(fields[0].descriptor.text, "UInt[8]");
    assert_eq!(fields[1].descriptor.text, "Rest");
}

/// Verifies descriptor-backed binary layouts parse in lambda parameters.
#[test]
fn parses_binary_layout_lambda_pattern_scaffold() {
    let expr = parse_terlan_expr("(Binary[big] { opcode: UInt[8], payload: Rest }) -> 1")
        .expect("parse binary layout lambda pattern");

    let Expr::Fun { clauses } = expr else {
        panic!("expected lambda expression");
    };
    let Pattern::BinaryLayout { endian, fields } = &clauses[0].patterns[0] else {
        panic!("expected binary layout lambda pattern");
    };
    assert_eq!(endian, "big");
    assert_eq!(fields[0].name, "opcode");
    assert_eq!(fields[0].descriptor.text, "UInt[8]");
}

/// Verifies binary layout syntax rejects unsupported endian policies.
#[test]
fn rejects_binary_layout_unknown_endian_policy() {
    let err = parse_terlan_expr("Binary[middle] { field: UInt[16] }")
        .expect_err("unknown endian should fail");

    assert_eq!(
        err.message,
        "binary layout endian must be `big` or `little`"
    );
}

/// Verifies binary layout syntax rejects duplicate field names.
#[test]
fn rejects_binary_layout_duplicate_fields() {
    let err = parse_terlan_expr("Binary[big] { field: UInt[16], field: UInt[8] }")
        .expect_err("duplicate field should fail");

    assert!(err
        .message
        .contains("duplicate binary layout field `field`"));
}

/// Verifies binary layout syntax rejects non-terminal rest fields.
#[test]
fn rejects_binary_layout_non_terminal_rest() {
    let err = parse_terlan_expr("Binary[big] { payload: Rest, flags: UInt[8] }")
        .expect_err("non-terminal Rest should fail");

    assert_eq!(err.message, "binary layout Rest field must be terminal");
}

/// Verifies binary layout syntax rejects multiple rest descriptors.
#[test]
fn rejects_binary_layout_multiple_rest_fields() {
    let err = parse_terlan_expr("Binary[big] { first: UInt[8], payload: Rest, tail: Rest }")
        .expect_err("multiple Rest fields should fail");

    assert_eq!(err.message, "binary layouts allow only one Rest field");
}

/// Verifies binary layout syntax rejects empty field lists.
#[test]
fn rejects_empty_binary_layout_fields() {
    let err = parse_terlan_expr("Binary[big] {}").expect_err("empty layout should fail");

    assert_eq!(
        err.message,
        "binary layouts require at least one descriptor field"
    );
}

/// Verifies binary layout syntax rejects non-canonical descriptors.
#[test]
fn rejects_binary_layout_unknown_descriptor() {
    let err =
        parse_terlan_expr("Binary[big] { flags: U8 }").expect_err("unknown descriptor should fail");

    assert_eq!(
        err.message,
        "binary layout field uses unsupported descriptor `U8`"
    );
}

/// Verifies binary layouts accept canonical widthless Unicode scalar descriptors.
#[test]
fn parses_binary_layout_unicode_scalar_descriptors() {
    for descriptor in ["Utf8", "Utf16", "Utf32"] {
        let source = format!("Binary[big] {{ tag: UInt[8], scalar: {descriptor} }}");
        let expr = parse_terlan_expr(&source).expect("Unicode descriptor should parse");
        let Expr::BinaryLayout { fields, .. } = expr else {
            panic!("expected binary layout expression");
        };

        assert_eq!(fields[1].name, "scalar");
        assert_eq!(fields[1].descriptor.text, descriptor);
    }
}

fn assert_typed_string_capture_pattern(
    pattern: &Pattern,
    literal_prefix: &str,
    capture_name: &str,
    capture_type: &str,
    literal_suffix: &str,
) {
    let Pattern::StringSegments(segments) = pattern else {
        panic!("expected segmented string pattern");
    };
    assert_eq!(segments.len(), 3);
    assert!(matches!(
        &segments[0],
        StringPatternSegment::Literal(value) if value == literal_prefix
    ));
    let StringPatternSegment::Capture(capture) = &segments[1] else {
        panic!("expected capture segment");
    };
    assert_eq!(capture.name, capture_name);
    assert_eq!(
        capture
            .annotation
            .as_ref()
            .map(|annotation| annotation.text.as_str()),
        Some(capture_type)
    );
    assert!(matches!(
        &segments[2],
        StringPatternSegment::Literal(value) if value == literal_suffix
    ));
}

/// Verifies the boolean precedence chain introduced by the canonical EBNF.
///
/// Inputs:
/// - A source expression containing pipe, `or`, `and`, comparison, and
///   arithmetic operators.
///
/// Output:
/// - Test passes when parsing preserves `pipe < or < and < cmp`.
///
/// Transformation:
/// - Parses one expression through the recursive-descent parser and
///   inspects the nested binary operator tree.

/// Verifies the boolean precedence chain introduced by the canonical EBNF.
///
/// Inputs:
/// - A source expression containing pipe, `or`, `and`, comparison, and
///   arithmetic operators.
///
/// Output:
/// - Test passes when parsing preserves `pipe < or < and < cmp`.
///
/// Transformation:
/// - Parses one expression through the recursive-descent parser and
///   inspects the nested binary operator tree.
#[test]
fn formal_boolean_operators_preserve_ebnf_precedence() {
    let expr = parse_terlan_expr("A |> C or D and E == F + G").expect("parse boolean precedence");
    let Expr::BinaryOp { op, right, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));

    let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
        panic!("expected or expression on pipe right side");
    };
    assert!(matches!(op, crate::terlan_syntax::parse_tree::BinaryOp::Or));

    let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
        panic!("expected and expression on or right side");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::And
    ));

    let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
        panic!("expected comparison expression on and right side");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::EqEq
    ));
    assert!(matches!(
        right.as_ref(),
        Expr::BinaryOp {
            op: crate::terlan_syntax::parse_tree::BinaryOp::Add,
            ..
        }
    ));
}

/// Verifies explicit cast syntax follows the canonical precedence chain.
///
/// Inputs:
/// - Expressions containing `as`, multiplication, pipe, and keyword forms.
///
/// Output:
/// - Test passes when `Cast` binds above multiplication and below postfix
///   primary parsing, including keyword expressions.
///
/// Transformation:
/// - Parses representative expressions and inspects the preserved syntax
///   tree instead of resolving the conversion semantically.

/// Verifies explicit cast syntax follows the canonical precedence chain.
///
/// Inputs:
/// - Expressions containing `as`, multiplication, pipe, and keyword forms.
///
/// Output:
/// - Test passes when `Cast` binds above multiplication and below postfix
///   primary parsing, including keyword expressions.
///
/// Transformation:
/// - Parses representative expressions and inspects the preserved syntax
///   tree instead of resolving the conversion semantically.
#[test]
fn formal_cast_expr_preserves_ebnf_precedence() {
    let expr = parse_terlan_expr("Value as Int * Count").expect("parse cast before multiply");
    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected multiplication expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::Mul
    ));
    assert!(matches!(
        left.as_ref(),
        Expr::Cast {
            target_type,
            ..
        } if target_type.text == "Int"
    ));

    let expr = parse_terlan_expr(
        "case Option { Atom[\"none\"] -> 0; value -> value } as Int |> inspect()",
    )
    .expect("parse casted keyword expression before pipe");
    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));
    let Expr::Cast { expr, target_type } = left.as_ref() else {
        panic!("expected cast expression on pipe left side");
    };
    assert_eq!(target_type.text, "Int");
    assert!(matches!(expr.as_ref(), Expr::Case { .. }));
}

/// Verifies that canonical Terlan source rejects backend-style equality
/// spellings.
///
/// Inputs:
/// - Three source expressions using deprecated equality spellings.
///
/// Output:
/// - Test passes when all deprecated spellings fail parsing.
///
/// Transformation:
/// - Parses each expression through the recursive-descent parser and
///   asserts the comparison operator guard fires before syntax output is
///   accepted.

/// Verifies that canonical Terlan source rejects backend-style equality
/// spellings.
///
/// Inputs:
/// - Three source expressions using deprecated equality spellings.
///
/// Output:
/// - Test passes when all deprecated spellings fail parsing.
///
/// Transformation:
/// - Parses each expression through the recursive-descent parser and
///   asserts the comparison operator guard fires before syntax output is
///   accepted.
#[test]
fn formal_deprecated_equality_operators_are_rejected() {
    for operator in ["=:=", "/=", "=/="] {
        let source = format!("left {operator} right");
        let error = parse_terlan_expr(&source)
            .err()
            .expect("deprecated equality spelling should fail");

        assert!(
            error.message.contains("deprecated"),
            "unexpected diagnostic for {operator}: {}",
            error.message
        );
    }
}
