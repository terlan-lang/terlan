use std::collections::BTreeMap;

use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_lowers_maps_and_funs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_container_emit.

pub make(value: Int): Dynamic ->
#{count => value, ok = :ok}.

pub pick(input: Dynamic): Dynamic ->
case input {
    #{count = value} -> value
}.

pub mapper(): Dynamic ->
(value) -> value + 1.
"#,
    )
    .expect("parse syntax output container fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("container subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Value) ->\n    #{count=>Value, ok=>ok}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("#{count:=Value} -> Value"));
    assert!(output.contains("mapper() ->\n    fun\n    (Value) -> Value + 1\nend."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_map_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_map_emit.

pub make(value: Int): Dynamic ->
#{count => value, ok = :ok}.

pub pick(input: Dynamic): Dynamic ->
case input {
    #{count = value} -> value
}.
"#,
    )
    .expect("parse syntax output map fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("map expressions and patterns should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Value) ->\n    #{count=>Value, ok=>ok}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("#{count:=Value} -> Value"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_anonymous_fun_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_fun_emit.

pub mapper(): Dynamic ->
(value) -> value + 1.
"#,
    )
    .expect("parse syntax output anonymous fun fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("anonymous fun should lower directly from syntax output")
    .render();

    assert!(
        output.contains("mapper() ->\n    fun\n    (Value) -> Value + 1\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_fixed_array_indexes() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_fixed_array_index_emit.

pub second(): Dynamic ->
#[1, 2, 3][1].
"#,
    )
    .expect("parse syntax output fixed array index fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("fixed array index should lower directly from syntax output")
    .render();

    assert!(
        output.contains("second() ->\n    element((1) + 1, {1, 2, 3})."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_list_comprehensions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_comprehension_emit.

pub increment(values: List[Int]): List[Int] ->
[value + 1 | value <- values].
"#,
    )
    .expect("parse syntax output list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("list comprehension should lower directly from syntax output")
    .render();

    assert!(
        output.contains("increment(Values) ->\n    [Value + 1 || Value <- Values]."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_stacked_list_comprehension_filters() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_comprehension_filter_emit.

pub selected(values: List[Int]): List[Int] ->
[value | value <- values, value > 0, value < 10].
"#,
    )
    .expect("parse stacked-filter list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("stacked-filter list comprehension should lower directly from syntax output")
    .render();

    assert!(
        output.contains(
            "selected(Values) ->\n    [Value || Value <- Values, (Value > 0) andalso (Value < 10)]."
        ),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_list_cons_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_cons_emit.

pub prepend(head: Int, tail: List[Int]): List[Int] ->
[head | tail].

pub first(list: List[Int]): Int ->
case list {
    [head | _tail] -> head
}.
"#,
    )
    .expect("parse syntax output list cons fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("list cons subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("prepend(Head, Tail) ->\n    [Head|Tail]."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("[Head|_tail] -> Head"),
        "output:\n{}",
        output
    );
}
