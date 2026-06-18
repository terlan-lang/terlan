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

/// Verifies assignment-compatible casts typecheck without conversion errors.
///
/// Inputs:
/// - A module using literal widening and a local type alias with `as`.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Parses source through the formal syntax-output path and proves the
///   typechecker accepts casts that require no runtime conversion after alias
///   expansion.
#[test]
fn syntax_output_accepts_assignable_casts_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_cast_assignable.\n\
\n\
pub type UserId = Int.\n\
\n\
pub literal(): Int ->\n\
    1 as Int.\n\
\n\
pub alias(id: UserId): Int ->\n\
    id as Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies unsupported casts still require explicit conversion semantics.
///
/// Inputs:
/// - A module attempting to cast a `String` value to `Int`.
///
/// Output:
/// - Test passes when typechecking reports the stable trait-backed conversion
///   diagnostic.
///
/// Transformation:
/// - Confirms `as` does not silently become an unchecked backend cast when the
///   source type is not already assignment-compatible with the target type.
#[test]
fn syntax_output_rejects_unproven_casts_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_cast_unproven.\n\
\n\
pub value(text: String): Int ->\n\
    text as Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("cast from Binary to Int requires trait-backed conversion")
        }),
        "expected cast conversion diagnostic, got {:?}",
        diagnostics
    );
}

/// Verifies explicit conversion conformances satisfy non-assignable casts.
///
/// Inputs:
/// - A module declaring `Convertable[From, To]`, an explicit
///   `Convertable[String, Int] for Int` implementation, and a cast from
///   `String` to `Int`.
///
/// Output:
/// - Test passes when the cast no longer reports the unsupported conversion
///   diagnostic.
///
/// Transformation:
/// - Parses through the formal syntax-output path and confirms `as` conversion
///   proof reuses the same trait conformance table as ordinary generic bounds.
#[test]
fn syntax_output_accepts_trait_backed_casts_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_cast_convertable.\n\
\n\
pub trait Convertable[From, To] {\n\
    convert(value: From): To.\n\
}.\n\
\n\
pub impl Convertable[String, Int] for Int {\n\
    convert(value: String): Int ->\n\
        1.\n\
}.\n\
\n\
pub value(text: String): Int ->\n\
    text as Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies lambda callback values satisfy function-typed parameters.
///
/// Inputs:
/// - A module declaring a callback-accepting function shaped like a generated
///   event registration API.
/// - A caller passing a lambda value into that function.
///
/// Output:
/// - Test passes when typechecking accepts the lambda as `(Event) -> Unit`.
///
/// Transformation:
/// - Exercises the L0.2 callback path without relying on generated `std.js`
///   bindings: the lambda expression is inferred as a function value, unified
///   with the API parameter type, and accepted as an ordinary argument.
#[test]
fn syntax_output_accepts_lambda_callback_arguments_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_callback_lambda.\n\
\n\
pub type Event = {id: Int}.\n\
\n\
pub register(callback: (Event) -> Unit): Unit ->\n\
    Unit.\n\
\n\
pub demo(): Unit ->\n\
    register((event: Event) -> Unit).\n\
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

/// Verifies imported overloads resolve by argument type.
///
/// Inputs:
/// - A provider interface declaring two `pick/1` signatures with different
///   parameter and return types.
/// - A consumer module calling both overloads through a qualified import.
///
/// Output:
/// - Test passes when both calls typecheck against their declared return types.
///
/// Transformation:
/// - Parses the provider as an interface module so duplicate same-name
///   same-arity signatures are preserved in `ModuleInterface.function_overloads`,
///   then typechecks the consumer through ordinary remote-call inference.
#[test]
fn syntax_output_selects_imported_overloads_by_argument_type_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module overload.Consumer.\n\
\n\
import overload.Provider.\n\
\n\
pub int_value(): Int ->\n\
    Provider.pick(1).\n\
\n\
pub string_value(): String ->\n\
    Provider.pick(\"x\").\n\
",
        "\
module overload.Provider.\n\
\n\
pub pick(value: Int): Int.\n\
pub pick(value: String): String.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local overload declarations resolve by argument type.
///
/// Inputs:
/// - A source module declaring two public `pick/1` functions with different
///   parameter and return types.
/// - Functions that call each overload through ordinary local call syntax.
///
/// Output:
/// - Test passes when both calls typecheck and HIR does not report a duplicate
///   function diagnostic for distinct overload shapes.
///
/// Transformation:
/// - Exercises parser output, HIR duplicate-shape filtering, type signature
///   candidate collection, and local call overload selection in one formal
///   source path.
#[test]
fn syntax_output_selects_local_overloads_by_argument_type_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module overload_local.\n\
\n\
pub pick(value: Int): Int ->\n\
    value.\n\
\n\
pub pick(value: String): String ->\n\
    value.\n\
\n\
pub int_value(): Int ->\n\
    pick(1).\n\
\n\
pub string_value(): String ->\n\
    pick(\"x\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported receiver-method overloads resolve by receiver type.
///
/// Inputs:
/// - A provider interface declaring two `length/0` receiver methods on
///   different wrapper types.
/// - A consumer module importing those types and calling `value.length()`.
///
/// Output:
/// - Test passes when receiver-method dispatch selects the candidate whose
///   receiver type matches the call target.
///
/// Transformation:
/// - Exercises generated-style method overloads through imported interface
///   summaries, receiver-method dispatch collection, and method-call
///   typechecking.
#[test]
fn syntax_output_selects_imported_receiver_overloads_by_receiver_type_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module overload.Consumer.\n\
\n\
import type overload.Provider.{JsArray, JsString}.\n\
import overload.Provider.\n\
\n\
pub string_length(value: JsString): Int ->\n\
    value.length().\n\
\n\
pub array_length(value: JsArray): Int ->\n\
    value.length().\n\
",
        "\
module overload.Provider.\n\
\n\
pub type JsString.\n\
pub type JsArray.\n\
\n\
pub (value: JsString) length(): Int.\n\
pub (value: JsArray) length(): Int.\n\
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

/// Verifies imported wrappers can satisfy bracket read and write contracts.
///
/// Inputs:
/// - A generated-style provider interface exporting an opaque wrapper type.
/// - Public `IndexGet` and `IndexSet` traits plus wrapper conformances.
/// - A consumer module importing the wrapper and trait contracts.
///
/// Output:
/// - Test passes when `values[0]` infers `String` and `values[0] = "x"`
///   infers `Unit` through imported interface metadata.
///
/// Transformation:
/// - Exercises the same trait-backed bracket desugaring that JS DOM wrappers
///   need, without relying on local source impl bodies.
#[test]
fn syntax_output_infers_imported_index_get_and_set_for_generated_wrapper_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module index_generated.Consumer.\n\
\n\
import type std.js.Dom.{ElementList}.\n\
import std.js.Dom.{IndexGet, IndexSet}.\n\
\n\
pub read(values: ElementList): String ->\n\
    values[0].\n\
\n\
pub write(values: ElementList): Unit ->\n\
    values[0] = \"x\".\n\
",
        "\
module std.js.Dom.\n\
\n\
pub type ElementList.\n\
\n\
pub trait IndexGet[C, I, T] {\n\
    get_at(collection: C, index: I): T.\n\
}.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
    set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub impl IndexGet[ElementList, Int, String] for ElementList {\n\
}.\n\
\n\
pub impl IndexSet[ElementList, Int, String] for ElementList {\n\
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
