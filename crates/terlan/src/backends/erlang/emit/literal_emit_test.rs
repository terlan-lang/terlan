use std::collections::BTreeMap;

use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_lowers_raw_atom_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_raw_atom_pattern_emit.

pub classify(value: Dynamic): Dynamic ->
case value {
    :none -> :ok;
    :empty -> :ok;
    other -> other
}.
"#,
    )
    .expect("parse syntax output raw atom pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("raw atom patterns should lower directly from syntax output")
    .render();

    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
    assert!(output.contains("'empty' -> ok"), "output:\n{}", output);
    assert!(output.contains("Other -> Other"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_quoted_atom_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_quoted_atom_emit.

pub module_atom(): Dynamic ->
:'Elixir.Module'.

pub classify(value: Dynamic): Dynamic ->
case value {
    :'some atom' -> :ok;
    :none -> :ok
}.
"#,
    )
    .expect("parse syntax output quoted atom fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quoted atoms should lower directly from syntax output")
    .render();

    assert!(
        output.contains("module_atom() ->\n    'Elixir.Module'."),
        "output:\n{}",
        output
    );
    assert!(output.contains("'some atom' -> ok"), "output:\n{}", output);
    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_bool_literals_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_bool_literal_emit.

pub negate(value: Bool): Bool ->
case value {
    true -> false;
    false -> true
}.
"#,
    )
    .expect("parse syntax output bool literal fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("bool literals should lower directly from syntax output")
    .render();

    assert!(output.contains("true -> false"), "output:\n{}", output);
    assert!(output.contains("false -> true"), "output:\n{}", output);
}
