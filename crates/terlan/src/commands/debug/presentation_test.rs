use super::render_bounded;

#[test]
fn huge_values_are_truncated_with_a_stable_marker() {
    let rendered = render_bounded(&"x".repeat(5_000));
    assert!(rendered.ends_with("…<truncated>"));
    assert_eq!(rendered.chars().count(), 4_108);
}
