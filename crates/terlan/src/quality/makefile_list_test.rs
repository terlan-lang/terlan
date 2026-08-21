use super::parse_make_list_variable_values;

#[test]
fn parses_continued_values_in_declaration_order() {
    let makefile = "OTHER := ignored\nCHECK_GATES := first \\\n\tsecond\n";

    assert_eq!(
        parse_make_list_variable_values(makefile, "CHECK_GATES"),
        ["first", "second"]
    );
}
