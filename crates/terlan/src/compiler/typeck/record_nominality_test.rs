use super::test_support::check_syntax_output;

/// Verifies same-shaped structs cannot substitute for one another by field layout.
#[test]
fn record_access_parity_rejects_same_shaped_distinct_struct_argument() {
    let diagnostics = check_syntax_output(
        r#"
module record_access_nominality_rejection.

pub struct Turtle {
    a: Int,
    b: Int,
    c: Int
}.

pub struct Tortoise {
    a: Int,
    b: Int,
    c: Int
}.

inspect(value: Tortoise): Dynamic ->
    {value.a, value.b}.

pub run(): Dynamic ->
    let turtle = Turtle {a: 1, b: 2, c: 3};
    inspect(turtle).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("Tortoise") && diagnostic.message.contains("Turtle")
        }),
        "diagnostics: {diagnostics:?}"
    );
}
