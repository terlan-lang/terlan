use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_lowers_macro_exprs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_macro_emit.

pub module_name(): Dynamic ->
?MODULE.

pub compare(a: Int, b: Int): Dynamic ->
?assert_equal(a, b).
"#,
    )
    .expect("parse syntax output macro expr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("macro exprs should lower directly from syntax output")
    .render();

    assert!(
        output.contains("module_name() ->\n    ?MODULE."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("compare(A, B) ->\n    ?assert_equal(A, B)."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_raw_macro_exprs_without_resolution() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_raw_macro_emit.

pub query(): Dynamic ->
sql{select * from users}.
"#,
    )
    .expect("parse syntax output raw macro expr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "raw macro expr should require macro resolution before direct emit"
    );
}
