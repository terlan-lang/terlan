use std::collections::BTreeMap;

use crate::terlan_syntax::parse_module_as_syntax_output;

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

#[test]
fn formal_syntax_output_direct_emit_lowers_typed_sql_form_to_runtime_wrapper_call() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_sql_emit.

pub find_user(id: Int): Dynamic ->
sql[UserRow] {SELECT id FROM users WHERE id = ${id} LIMIT 1}.
"#,
    )
    .expect("parse syntax output typed SQL fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("typed SQL form should lower to a runtime wrapper call")
    .render();

    assert!(output.contains("find_user(Id) ->"), "output:\n{}", output);
    assert!(
        output.contains("terlan_sql_runtime:query_one("),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("<<\"SELECT id FROM users WHERE id = $1 LIMIT 1\">>"),
        "output:\n{}",
        output
    );
    assert!(output.contains("[Id]"), "output:\n{}", output);
    assert!(output.contains("<<\"UserRow\">>"), "output:\n{}", output);
    assert!(output.contains("[<<\"id\">>]"), "output:\n{}", output);
    assert!(
        output.contains("<<\"Result[Option[UserRow], Error]\">>"),
        "output:\n{}",
        output
    );
}
