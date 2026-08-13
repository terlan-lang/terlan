use super::*;

/// Verifies valid patterns compile and match through the maintained regex crate.
#[test]
fn compile_and_match_delegates_to_regex_crate() {
    let regex = compile("ter[a-z]+").expect("valid regex should compile");

    assert!(is_match(&regex, "terlan"));
    assert!(!is_match(&regex, "rust"));
}

#[test]
fn matching_line_numbers_are_one_based_and_ordered() {
    let regex = compile("(?i)cuda|(^|[^a-z])ptx([^a-z]|$)").expect("valid regex");

    assert_eq!(
        matching_line_numbers(&regex, "CUDA kernel\nordinary text\nptx module\n"),
        vec![1, 3]
    );
    assert!(matching_line_numbers(&regex, "empty\nsource").is_empty());
}

/// Verifies invalid patterns return stable portable errors.
#[test]
fn compile_reports_stable_error_for_invalid_pattern() {
    let error = compile("(").expect_err("invalid regex should fail");

    assert_eq!(error.code(), "regex.compile");
    assert!(error.message().contains("unclosed group"));
    assert_eq!(error.offset(), 0);
}

/// Verifies match, capture, replacement, split, and escape helpers.
#[test]
fn helpers_cover_common_regex_operations() {
    let regex = compile("(?P<name>[A-Z][a-z]+) ([0-9]+)").expect("regex should compile");

    assert_eq!(find(&regex, "Ada 42").as_deref(), Some("Ada 42"));
    assert_eq!(capture(&regex, "Ada 42", 2).as_deref(), Some("42"));
    assert_eq!(
        named_capture(&regex, "Ada 42", "name").as_deref(),
        Some("Ada")
    );

    let digits = compile("[0-9]+").expect("regex should compile");
    assert_eq!(find_all(&digits, "a12 b34"), vec!["12", "34"]);
    assert_eq!(replace(&digits, "a12 b34", "#"), "a# b#");

    let comma = compile(",").expect("regex should compile");
    assert_eq!(split(&comma, "a,b,c"), vec!["a", "b", "c"]);
    assert_eq!(escape("a+b"), "a\\+b");
}
