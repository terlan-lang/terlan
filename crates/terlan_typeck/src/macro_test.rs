use super::test_support::*;
use super::*;
use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_checks_macro_expr_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_macro_expr.\n\
pub module_name(): Dynamic ->\n\
    ?MODULE.\n\
pub compare(a: Int, b: Int): Dynamic ->\n\
    ?assert_equal(a, b).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_macro_expr_with_declared_return_type() {
    let diagnostics = check_syntax_output(
        "\
module syntax_macro_return_type.
pub macro to_bool(X: Int): Ast[Bool] ->
    quote X.

pub bad(X: Int): Int ->
    ?to_bool(X).
",
    );

    assert!(
        diagnostics.iter().any(
            |diag| diag.message.contains("expected Int") && diag.message.contains("found Bool")
        ),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_config_declaration_placeholders() {
    let diagnostics = check_syntax_output(
        "\
	module syntax_config_declaration_placeholders.
target erlang.
machine linux.
static site.
	",
    );

    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("unsupported raw declaration kind")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn expands_syntax_raw_macros_no_ops_without_raw_macros() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_raw_macro_expansion_ok.\n\
    pub query(): Dynamic ->\n    42.\n\
",
    )
    .expect("parse syntax-output module");

    let (expanded, diagnostics) = expand_syntax_raw_macros(module.clone());

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    assert_eq!(
        expanded, module,
        "non-macro modules must pass through unchanged"
    );
}

#[test]
fn syntax_output_checks_macro_signatures_on_formal_path() {
    let module = parse_module_as_syntax_output(
        "\
module bad_macro_return.\n\
pub macro bad(X: Int): Int ->\n\
    X.\n\
",
    )
    .expect("parse syntax output macro fixture");

    let diagnostics = check_syntax_macro_decl_signatures(&module);

    assert!(
        diagnostics.iter().any(
            |diag| diag.message.contains("macro `bad` must return Ast[T]")
                && diag.message.contains("found Int")
        ),
        "diagnostics: {:?}",
        diagnostics
    );
}
