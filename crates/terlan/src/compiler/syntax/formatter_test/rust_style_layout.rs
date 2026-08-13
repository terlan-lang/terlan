use super::super::format_source_module;
use crate::terlan_syntax::parse_module;

#[test]
fn formatter_wraps_long_calls_with_one_argument_per_line() {
    let output = format_source_module(
        r#"
module rust_style_call.

pub render(): String -> combine("the-first-deliberately-long-argument", "the-second-deliberately-long-argument", "the-third-deliberately-long-argument").
"#,
    )
    .expect("format long call");

    assert!(
        output.contains(
            r#"pub render(): String ->
    combine(
        "the-first-deliberately-long-argument",
        "the-second-deliberately-long-argument",
        "the-third-deliberately-long-argument",
    )."#
        ),
        "output:\n{output}"
    );
    parse_module(&output).expect("formatted trailing-comma call should parse");
}

#[test]
fn formatter_breaks_long_fluent_calls_one_method_per_line() {
    let output = format_source_module(
        r#"
module rust_style_chain.

pub report(): Json -> Json.object().put("schema", Json.string("terlan.release.report.v1")).put("decision", Json.string("pass")).put("artifact_sha256", Json.string("0123456789abcdef")).
"#,
    )
    .expect("format fluent chain");

    assert!(
        output.contains(
            r#"pub report(): Json ->
    Json.object()
        .put("schema", Json.string("terlan.release.report.v1"))
        .put("decision", Json.string("pass"))
        .put("artifact_sha256", Json.string("0123456789abcdef"))."#
        ),
        "output:\n{output}"
    );
    parse_module(&output).expect("formatted fluent chain should parse");
}

#[test]
fn formatter_indents_if_clauses_like_structural_block_arms() {
    let output = format_source_module(
        r#"
module rust_style_if.

pub choose(flag: Bool): Int -> if { flag -> 1; true -> 0 }.
"#,
    )
    .expect("format if clauses");

    assert!(output.contains(
        r#"pub choose(flag: Bool): Int ->
    if {
        flag -> 1;
        true -> 0
    }."#
    ));
    parse_module(&output).expect("indented if clauses should parse");
    assert_eq!(
        format_source_module(&output).expect("reformat indented if clauses"),
        output
    );
}

#[test]
fn formatter_wraps_long_collections_with_trailing_commas() {
    let output = format_source_module(
        r#"
module rust_style_collection.

pub values(): List[String] -> ["the-first-deliberately-long-value", "the-second-deliberately-long-value", "the-third-deliberately-long-value"].
"#,
    )
    .expect("format long list");

    assert!(output.contains(
        r#"pub values(): List[String] ->
    [
        "the-first-deliberately-long-value",
        "the-second-deliberately-long-value",
        "the-third-deliberately-long-value",
    ]."#
    ));
    parse_module(&output).expect("formatted trailing-comma list should parse");
}

#[test]
fn formatter_wraps_long_records_with_trailing_commas() {
    let output = format_source_module(
        r#"
module rust_style_record.

pub record(): ReleaseReport -> ReleaseReport {schema: "terlan.release.report.v1", decision: "pass", artifact_sha256: "0123456789abcdef0123456789abcdef"}.
"#,
    )
    .expect("format long record");

    assert!(output.contains(
        r#"ReleaseReport {
        schema: "terlan.release.report.v1",
        decision: "pass",
        artifact_sha256: "0123456789abcdef0123456789abcdef",
    }."#
    ));
    parse_module(&output).expect("formatted trailing-comma record should parse");
}

#[test]
fn formatter_wraps_long_fixed_arrays_with_trailing_commas() {
    let output = format_source_module(
        r#"
module rust_style_fixed_array.

pub values(): FixedArray[String] -> #["the-first-deliberately-long-value", "the-second-deliberately-long-value", "the-third-deliberately-long-value"].
"#,
    )
    .expect("format long fixed array");

    assert!(output.contains(
        r#"#[
        "the-first-deliberately-long-value",
        "the-second-deliberately-long-value",
        "the-third-deliberately-long-value",
    ]."#
    ));
    parse_module(&output).expect("formatted trailing-comma fixed array should parse");
}

#[test]
fn formatter_wraps_long_tuples_with_trailing_commas() {
    let output = format_source_module(
        r#"
module rust_style_tuple.

pub values(): {String, String, String} -> {"the-first-deliberately-long-value", "the-second-deliberately-long-value", "the-third-deliberately-long-value"}.
"#,
    )
    .expect("format long tuple");

    assert!(output.contains(
        r#"{
        "the-first-deliberately-long-value",
        "the-second-deliberately-long-value",
        "the-third-deliberately-long-value",
    }."#
    ));
    parse_module(&output).expect("formatted trailing-comma tuple should parse");
    assert_eq!(
        format_source_module(&output).expect("reformat trailing-comma tuple"),
        output
    );
}

#[test]
fn parser_accepts_formatter_owned_trailing_commas() {
    parse_module(
        r#"
module rust_style_trailing_commas.

struct Payload {
    first: String,
    second: String,
}.

pub build(
    first: String,
    second: String,
): Payload -> Payload {
    first: first,
    second: second,
}.

pub collections(): {names: List[String], slots: FixedArray[Int],} -> {
    names: ["one", "two",],
    slots: #[1, 2,],
}.

pub tuple(): {String, String} -> {
    "one",
    "two",
}.

pub tuple_pattern(value: {String, String}): Bool ->
    case value {
        {"one", "two",} -> true;
        _ -> false
    }.

pub invoke(): Dynamic -> consume(
    "one",
    "two",
).
"#,
    )
    .expect("formatter-owned multiline forms should accept trailing commas");
}

#[test]
fn formatter_preserves_operator_grouping_and_precedence() {
    let output = format_source_module(
        r#"
module rust_style_precedence.

pub negated_window(start: Int, clean: Int, finish: Int): Bool -> not (0 < start and start < clean and clean < finish).
pub grouped_product(left: Int, right: Int, scale: Int): Int -> (left + right) * scale.
pub right_nested_subtract(left: Int, middle: Int, right: Int): Int -> left - (middle - right).
pub grouped_cast(left: Int, right: Int): Int -> (left + right) as Int.
pub grouped_field(left: Dynamic, right: Dynamic): Dynamic -> (left + right).value.
pub grouped_pipe(source: Dynamic, middle: Dynamic, sink: Dynamic): Dynamic -> source |> (middle |> sink).
"#,
    )
    .expect("format precedence-sensitive expressions");

    assert!(output.contains("not (0 < start and start < clean and clean < finish)."));
    assert!(output.contains("(left + right) * scale."));
    assert!(output.contains("left - (middle - right)."));
    assert!(output.contains("(left + right) as Int."));
    assert!(output.contains("(left + right).value."));
    assert!(output.contains("|> (middle |> sink)."));
    parse_module(&output).expect("precedence-preserving formatter output should parse");
    assert_eq!(
        format_source_module(&output).expect("reformat precedence output"),
        output
    );
}

#[test]
fn formatter_wraps_long_boolean_chains_with_operator_led_continuations() {
    let source = r#"
module rust_style_boolean_chain.

pub all_valid(first: Bool, second: Bool, third: Bool, fourth: Bool): Bool -> assert_equal(true, first) and assert_equal(true, second) and assert_equal(true, third) and assert_equal(true, fourth).

pub any_valid(first: Bool, second: Bool, third: Bool, fourth: Bool): Bool -> assert_equal(true, first) or assert_equal(true, second) or assert_equal(true, third) or assert_equal(true, fourth).
"#;
    let output = format_source_module(source).expect("format long boolean chains");

    assert!(
        output.contains(
            r#"    assert_equal(true, first)
        and assert_equal(true, second)
        and assert_equal(true, third)
        and assert_equal(true, fourth)."#
        ),
        "output:\n{output}"
    );
    assert!(
        output.contains(
            r#"    assert_equal(true, first)
        or assert_equal(true, second)
        or assert_equal(true, third)
        or assert_equal(true, fourth)."#
        ),
        "output:\n{output}"
    );
    parse_module(&output).expect("multiline boolean chains should parse");
    assert_eq!(
        format_source_module(&output).expect("reformat multiline boolean chains"),
        output
    );
}

#[test]
fn formatter_keeps_short_and_mixed_boolean_precedence_stable() {
    let source = r#"
module rust_style_boolean_precedence.

pub short(first: Bool, second: Bool): Bool -> first and second.
pub mixed(first: Bool, second: Bool, third: Bool, fourth: Bool): Bool -> first or second and third or fourth.
"#;
    let output = format_source_module(source).expect("format boolean precedence");

    assert!(output.contains("first and second."));
    assert!(!output.contains("first\n        and second."));
    assert!(output.contains("first or second and third or fourth."));
    parse_module(&output).expect("boolean precedence output should parse");
    assert_eq!(
        format_source_module(&output).expect("reformat boolean precedence"),
        output
    );
}

#[test]
fn formatter_preserves_grouped_sequences_inside_clauses() {
    let output = format_source_module(
        r#"
module rust_style_sequence_grouping.

pub choose(flag: Bool): Int -> if { (tick(); flag) -> (tick(); 1); true -> 2 }.
pub choose_local(value: Int): Int -> if { (let adjusted = value + 1; tick(); adjusted > 0) -> value; true -> 0 }.
"#,
    )
    .expect("format grouped clause sequences");

    assert!(output.contains("(tick(); flag) -> (tick(); 1)"));
    assert!(output.contains("(let adjusted = value + 1; (tick(); adjusted > 0)) -> value"));
    parse_module(&output).unwrap_or_else(|error| {
        panic!("grouped clause sequence output should parse: {error:?}\n{output}")
    });
    assert_eq!(
        format_source_module(&output).expect("reformat grouped clause sequences"),
        output
    );
}

#[test]
fn formatter_preserves_zero_arity_shape_and_pattern_calls() {
    let output = format_source_module(
        r#"
module rust_style_zero_arity_shape.

constructor Ready {
    (): Dynamic -> Atom["ready"]
}.

shape ReadyValue() = Ready().

pub is_ready(value: Dynamic): Bool ->
    case value {
        ReadyValue() -> true;
        _ -> false
    }.
"#,
    )
    .expect("format zero-arity shape and pattern calls");

    assert!(output.contains("shape ReadyValue() = Ready()."));
    assert!(output.contains("ReadyValue() -> true;"));
    parse_module(&output).expect("zero-arity formatter output should parse");
    assert_eq!(
        format_source_module(&output).expect("reformat zero-arity shape output"),
        output
    );
}

#[test]
fn rust_style_structural_layout_is_idempotent() {
    let source = r#"
module rust_style_idempotence.

pub report(): Json -> Json.object().put("schema", Json.string("terlan.release.report.v1")).put("decision", Json.string("pass")).put("artifact_sha256", Json.string("0123456789abcdef")).
"#;
    let once = format_source_module(source).expect("first format");
    let twice = format_source_module(&once).expect("second format");

    assert_eq!(twice, once);
}
