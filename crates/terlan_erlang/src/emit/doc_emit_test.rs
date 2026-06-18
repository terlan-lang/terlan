use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_preserves_type_and_function_docs() {
    let module = parse_module_as_syntax_output(
        r#"
//! Module docs.
//! Second module line.

module syntax_output_docs_emit.

/// Status value.
pub type Status = :ok.

/// Adds one.
pub add(x: Int): Int ->
x + 1.
"#,
    )
    .expect("parse syntax output docs fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("docs should lower directly from syntax output")
    .render();

    assert!(
        output.contains("-moduledoc \"Module docs.\\nSecond module line.\"."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("-doc \"Status value.\".\n\n-type status() :: ok."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("-doc \"Adds one.\".\n\n-spec add(integer()) -> integer()."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_preserves_struct_and_field_docs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_docs_emit.

/// A user account.
pub struct User {
/// Stable internal ID.
id: Int,

/// Display name.
name: Text
}.
"#,
    )
    .expect("parse syntax output struct docs fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct docs should lower directly from syntax output")
    .render();

    assert!(output.contains("-export_type([user/0])."));
    assert!(output.contains("-type user() :: #user{}."));
    assert!(output.contains("id % Stable internal ID."));
    assert!(output.contains("name % Display name."));
}
