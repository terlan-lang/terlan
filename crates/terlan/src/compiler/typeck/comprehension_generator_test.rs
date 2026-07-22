use super::test_support::check_syntax_output;

#[test]
fn syntax_output_accepts_ordered_list_comprehension_generators() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_ordered_generators.\n\
pub flatten(rows: List[List[Int]]): List[Int] ->\n\
    [value | row <- rows, value <- row].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_forward_list_comprehension_generator_binding() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_forward_generator_binding.\n\
pub invalid(rows: List[List[Int]]): List[Int] ->\n\
    [value | value <- row, row <- rows].\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("source references later generator binding `row`")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_accepts_nested_shadow_of_later_generator_binding() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_nested_generator_shadow.\n\
pub valid(rows: List[List[Int]]): List[Int] ->\n\
    [value | value <- case 1 { row -> [row] }, row <- rows].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}
