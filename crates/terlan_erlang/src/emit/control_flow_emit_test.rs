use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_lowers_if_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_if_emit.

pub choose(flag: Bool): Int ->
if {
    flag -> 1;
    true -> 0
}.
"#,
    )
    .expect("parse syntax output if fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("if expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains("choose(Flag) ->\n    if\n    Flag -> 1;\n    true -> 0\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_try_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_expr_emit.

pub wait(): Int ->
try risky() {
    {:ok, value} -> value
catch
    :error -> 0
}.

risky(): {:ok, Int} ->
{:ok, 1}.
"#,
    )
    .expect("parse syntax output try expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("try expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains(
            "wait() ->\n    try risky()\nof\n    {ok, Value} -> Value\n\ncatch\n    error -> 0\nend."
        ),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_try_after_cleanup() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_after_emit.

pub wait(): Int ->
try risky() {
after
    0 -> 1
}.

risky(): Int ->
1.
"#,
    )
    .expect("parse syntax output try after expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    let module = output.expect("try-after should lower directly from syntax output");

    let source = module.render();
    assert!(source.contains("after\n    0 -> 1"), "output:\n{}", source);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_function_clause_guards() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_function_guard_emit.

pub abs(value) when value < 0 ->
0 - value;
abs(value) ->
value.
"#,
    )
    .expect("parse syntax output function guard fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("function guards should lower directly from syntax output")
    .render();

    assert!(
        output.contains("abs(Value) when Value < 0 ->\n    0 - Value;"),
        "output:\n{}",
        output
    );
    assert!(output.contains("abs(Value) ->\n    Value."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_case_guards() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_case_guard_emit.

pub classify(value: Int): Int ->
case value {
    x when x > 0 -> x;
    _ -> 0
}.
"#,
    )
    .expect("parse syntax output case guard fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("case guard should lower directly from syntax output")
    .render();

    assert!(output.contains("X when X > 0 -> X"), "output:\n{}", output);
    assert!(output.contains("_ -> 0"), "output:\n{}", output);
}
