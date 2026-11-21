use super::super::format_source_module;

/// Verifies a leading ordinary binding does not regroup its remaining body.
///
/// Inputs:
/// - A short function body with one `let` followed by effectful calls.
///
/// Output:
/// - Every statement remains on its own line without synthetic parentheses.
///
/// Transformation:
/// - Flattens a sequence used as an ordinary binding's continuation so the
///   formatter output satisfies the semicolon-chain lint rule.
#[test]
pub(super) fn formatter_splits_sequence_after_ordinary_let() {
    let output = format_source_module(
        r#"
module let_sequence_fmt.

pub main(): Bool ->
    let values = List.new();
    values.push(1);
    values.push(2);
    values.length() == 2.
"#,
    )
    .expect("format ordinary let continuation sequence");

    assert!(output.contains(
        "pub main(): Bool ->\n    let values = List.new();\n    values.push(1);\n    values.push(2);\n    values.length() == 2."
    ));
    assert!(!output.contains("(values.push(1);"));
}

/// Verifies lambda continuations follow the same statement layout contract.
#[test]
pub(super) fn formatter_splits_sequence_after_lambda_let() {
    let output = format_source_module(
        r#"
module lambda_let_sequence_fmt.

pub apply(): Bool ->
    for_all(values(), (value) -> let state = Set.new[Int](); state.add(value); state.contains(value)).
"#,
    )
    .expect("format lambda let continuation sequence");

    assert!(
        output.contains(
            "(value) ->\n            let state = Set.new[Int]();\n            state.add(value);\n            state.contains(value)"
        ),
        "unexpected lambda layout:\n{output}"
    );
    assert!(!output.contains("(state.add(value);"));
}

/// Branch semicolons delimit clauses, so a let continuation needs its group.
#[test]
fn formatter_preserves_grouped_sequence_in_branch_let_continuations() {
    for branch in [
        "case input { true -> let valid = true; (noop(); noop(); valid); _ -> false }",
        "if { input -> let valid = true; (noop(); noop(); valid); true -> false }",
    ] {
        let source = format!("module branch_sequence.\nnoop(): Unit -> Unit.\npub value(input: Bool): Bool -> {branch}.\n");
        let formatted = format_source_module(&source).expect("format branch sequence");
        let second = format_source_module(&formatted).expect("formatted branch must parse");
        assert_eq!(formatted, second, "branch formatting must be stable");
        assert!(formatted.contains("(noop(); noop(); valid)"), "{formatted}");
    }
}
