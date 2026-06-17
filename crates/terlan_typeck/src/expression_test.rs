use super::test_support::*;
use super::*;
use terlan_hir::resolve_syntax_module_output;
use terlan_syntax::{parse_expr_as_syntax_output, parse_module_as_syntax_output};

/// Verifies that syntax-output boolean operators typecheck as Bool.
///
/// Inputs:
/// - A module whose function body combines `and`, `or`, and comparison
///   expressions with a `Bool` return annotation.
///
/// Output:
/// - Test passes when no type diagnostics are produced.
///
/// Transformation:
/// - Parses through the formal syntax-output path, resolves the module, and
///   typechecks the resulting expression tree.
#[test]
fn syntax_output_boolean_binary_ops_typecheck_as_bool() {
    let diagnostics = check_syntax_output(
        "\
module boolean_ops.\n\
pub decide(ready: Bool, fallback: Bool, value: Int): Bool ->\n\
    ready and value == 1 or fallback.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies that syntax-output boolean operators reject non-Bool operands.
///
/// Inputs:
/// - A module whose function body uses an `Int` as the right operand of
///   `and`.
///
/// Output:
/// - Test passes when typechecking reports a Bool operand mismatch.
///
/// Transformation:
/// - Parses through the formal syntax-output path and checks the generated
///   diagnostics for the Bool mismatch emitted by binary operator inference.
#[test]
fn syntax_output_boolean_binary_ops_require_bool_operands() {
    let diagnostics = check_syntax_output(
        "\
module boolean_ops_bad.\n\
pub decide(ready: Bool): Bool ->\n\
    ready and 1.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Bool found")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_unary_expr_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_unary_expr.\n\
pub flip(flag: Bool): Bool ->\n\
    not flag.\n\
pub negate(value: Int): Int ->\n\
    -value.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_remote_fun_ref_on_formal_path() {
    let parsed = parse_module_as_syntax_output(
        "\
module syntax_remote_fun_ref.\n\
pub ref(): Dynamic ->\n\
    fun math:double/1.\n\
",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

#[test]
fn syntax_output_checks_if_expr_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_if_expr.\n\
pub choose(flag: Bool): Int ->\n\
    if {\n\
        flag -> 1;\n\
        true -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_receive_expr_on_formal_path() {
    let parse_result = parse_module_as_syntax_output(
        "\
module syntax_receive_expr.\n\
pub wait(): Int ->\n\
    receive {\n\
        {:ok, value} -> value;\n\
        :stop -> 0\n\
    }.\n\
",
    );

    assert!(parse_result.is_err(), "receive syntax should be rejected");
}

#[test]
fn syntax_output_checks_try_expr_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_try_expr.\n\
pub wait(): Int ->\n\
    try risky() {\n\
        {:ok, value} -> value\n\
    catch\n\
        :error -> 0\n\
    }.\n\
risky(): {:ok, Int} ->\n\
    {:ok, 1}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_supports_try_after_cleanup() {
    let diagnostics = check_syntax_output(
        "\
module syntax_try_after_expr.\n\
pub wait(): Int ->\n\
    try risky() {\n\
    after\n\
        0 -> 1\n\
    }.\n\
risky(): Int ->\n\
    1.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

#[test]
fn syntax_output_binds_list_comprehension_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_patterns.\n\
pub inc_all(values: List[Int]): List[Int] ->\n\
    [x + 1 | x <- values].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_stacked_list_comprehension_filters_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_stacked_filters.\n\
pub values(items: List[Int]): List[Int] ->\n\
    [x | x <- items, x > 0, x < 10].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_iterable_list_comprehension_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_iterable_comprehension.
pub type Iterator[T] = List[T].

pub trait Iterable[C, T] {
    iterator(collection: C): Iterator[T].
}.

pub struct IntCollection implements Iterable[IntCollection, Int] {
    values: List[Int]
}.

pub (collection: IntCollection) iterator(): Iterator[Int] ->
    collection.values.

pub values(items: IntCollection): List[Int] ->
    [value | value <- items, value > 0].
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_non_bool_list_comprehension_filter_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_filter_type.\n\
pub values(items: List[Int]): List[Int] ->\n\
    [x | x <- items, x].\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("list comprehension filter")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_list_comprehension_non_list_source_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_source.\n\
pub inc_all(value: Int): List[Int] ->\n\
    [x + 1 | x <- value].\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("list comprehension source must be List or Iterable")
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_local_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_call_inference.\n\
add_one(x: Int): Int ->\n\
    x + 1.\n\
pub inc_all(values: List[Int]): List[Int] ->\n\
    [add_one(x) | x <- values].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_standalone_expression_on_formal_path() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_expr_query.\n\
pub add_one(value: Int): Int ->\n\
    value + 1.\n\
",
    )
    .expect("parse syntax module");
    let resolved = resolve_syntax_module_output(&module).module;
    let expression = parse_expr_as_syntax_output("add_one(41)").expect("parse syntax expr");

    let (ty, diagnostics) = infer_syntax_expression_type(&expression, &module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    assert_eq!(pretty_type(&ty), "Int");
}

/// Verifies bracket reads infer through `IndexGet`.
///
/// Inputs:
/// - A syntax-output module declaring `IndexGet[C, I, T]`.
/// - One struct and one explicit `IndexGet[IndexedBox, Int, Int]` impl.
/// - A function body indexing a value parameter.
///
/// Output:
/// - Test passes when the function body is reported as `Int` against an
///   intentionally wrong `String` return annotation.
///
/// Transformation:
/// - Exercises the compiler-owned desugaring contract that treats
///   `collection[index]` as a trait-backed `IndexGet.get_at(collection,
///   index)` lookup while keeping parser and CoreIR index syntax
///   collection-neutral.

/// Verifies bracket reads infer through `IndexGet`.
///
/// Inputs:
/// - A syntax-output module declaring `IndexGet[C, I, T]`.
/// - One struct and one explicit `IndexGet[IndexedBox, Int, Int]` impl.
/// - A function body indexing a value parameter.
///
/// Output:
/// - Test passes when the function body is reported as `Int` against an
///   intentionally wrong `String` return annotation.
///
/// Transformation:
/// - Exercises the compiler-owned desugaring contract that treats
///   `collection[index]` as a trait-backed `IndexGet.get_at(collection,
///   index)` lookup while keeping parser and CoreIR index syntax
///   collection-neutral.
#[test]
fn syntax_output_infers_index_read_through_index_get_trait() {
    let diagnostics = check_syntax_output(
        "\
module syntax_index_get_trait.\n\
\n\
pub trait IndexGet[C, I, T] {\n\
    get_at(collection: C, index: I): T.\n\
}.\n\
\n\
pub struct IndexedBox {\n\
    value: Int\n\
}.\n\
\n\
pub impl IndexGet[IndexedBox, Int, Int] for IndexedBox {\n\
    get_at(collection: IndexedBox, index: Int): Int ->\n\
        collection.value.\n\
}.\n\
\n\
pub read(value: IndexedBox): String ->\n\
    value[0].\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found Int")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies bracket assignments infer through `IndexSet`.
///
/// Inputs:
/// - A syntax-output module declaring `IndexSet[C, I, T]`.
/// - One struct declaring `implements IndexSet[IndexedBox, Int, Int]`.
/// - One mutable receiver method satisfying the trait contract.
/// - A function body assigning through bracket syntax.
///
/// Output:
/// - Test passes when the function body is reported as `Unit` against an
///   intentionally wrong `String` return annotation.
///
/// Transformation:
/// - Exercises the compiler-owned desugaring contract that treats
///   `collection[index] = value` as a trait-backed
///   `IndexSet.set_at(collection, index, value)` update while preserving
///   target-neutral parser syntax.

/// Verifies bracket assignments infer through `IndexSet`.
///
/// Inputs:
/// - A syntax-output module declaring `IndexSet[C, I, T]`.
/// - One struct declaring `implements IndexSet[IndexedBox, Int, Int]`.
/// - One mutable receiver method satisfying the trait contract.
/// - A function body assigning through bracket syntax.
///
/// Output:
/// - Test passes when the function body is reported as `Unit` against an
///   intentionally wrong `String` return annotation.
///
/// Transformation:
/// - Exercises the compiler-owned desugaring contract that treats
///   `collection[index] = value` as a trait-backed
///   `IndexSet.set_at(collection, index, value)` update while preserving
///   target-neutral parser syntax.
#[test]
fn syntax_output_infers_index_assignment_through_index_set_trait() {
    let diagnostics = check_syntax_output(
        "\
module syntax_index_set_trait.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
    set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub struct IndexedBox implements IndexSet[IndexedBox, Int, Int] {\n\
    value: Int\n\
}.\n\
\n\
pub (mut collection: IndexedBox) set_at(index: Int, value: Int): Unit ->\n\
    Unit.\n\
\n\
pub write(value: IndexedBox): String ->\n\
    value[0] = 1.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found Unit")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_pipe_forward_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_pipe_inference.\n\
add_one(x: Int): Int ->\n\
    x + 1.\n\
pub via_pipe(x: Int): Int ->\n\
    x |> add_one().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_binary_ops_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_binary_op_inference.\n\
pub add(x: Int, y: Int): Int ->\n\
    x + y.\n\
pub compare(x: Int, y: Int): Bool ->\n\
    x <= y.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_field_access_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_field_inference.\n\
pub struct User {\n\
    id: Int,\n\
    name: Binary\n\
}.\n\
pub get_id(user: User): Int ->\n\
    user.id.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_template_instantiation_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_instantiation.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    Page{ title = title }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_html_blocks_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_html_blocks.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    html {\n\
        <section class={[\"hero\"]}>{title}</section>\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
