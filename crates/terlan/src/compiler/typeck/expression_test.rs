use super::test_support::*;
use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::{parse_expr_as_syntax_output, parse_module_as_syntax_output};

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

/// Verifies unresolved qualified calls fail during typechecking.
///
/// Inputs:
/// - A module that calls `Other.test()` without importing or defining
///   `Other`.
///
/// Output:
/// - Test passes when the typechecker reports the missing module.
///
/// Transformation:
/// - Locks the compiler contract that backend targets must not receive
///   unresolved qualified calls that would become target-specific runtime
///   failures.
#[test]
fn syntax_output_rejects_unresolved_qualified_call_module() {
    let diagnostics = check_syntax_output(
        "\
module missing_remote_call.\n\
\n\
pub main(): Unit ->\n\
    Other.test().\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("cannot resolve module `Other` for call `Other.test/0`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local private struct fields can be accessed with `#`.
///
/// Inputs:
/// - A module declaring `User.#email`.
/// - A local function reading `user.#email`.
///
/// Output:
/// - Test passes when typechecking accepts the private field access inside the
///   defining module.
///
/// Transformation:
/// - Exercises visibility metadata collected from the local struct declaration
///   during dot field-access inference.
#[test]
fn syntax_output_accepts_local_private_struct_field_access() {
    let diagnostics = check_syntax_output(
        "\
module private_field_access.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n\
\n\
pub email(user: User): String ->\n\
    user.#email.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies private struct fields require private access spelling.
///
/// Inputs:
/// - A module declaring `User.#email`.
/// - A local function attempting `user.email`.
///
/// Output:
/// - Test passes when typechecking reports that the field must be accessed as
///   `#email`.
///
/// Transformation:
/// - Confirms the typechecker does not treat private fields as ordinary public
///   fields even inside the defining module.
#[test]
fn syntax_output_rejects_bare_access_to_private_struct_field() {
    let diagnostics = check_syntax_output(
        "\
module private_field_access_bad.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n\
\n\
pub email(user: User): String ->\n\
    user.email.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("private field email on struct User must be accessed as #email")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local private struct fields can be updated with `#`.
///
/// Inputs:
/// - A module declaring `User.#email`.
/// - A local function updating `user#User { #email = ... }`.
///
/// Output:
/// - Test passes when typechecking accepts the private field update inside the
///   defining module.
///
/// Transformation:
/// - Exercises record-update visibility metadata using the inferred receiver
///   type.
#[test]
fn syntax_output_accepts_local_private_struct_field_update() {
    let diagnostics = check_syntax_output(
        "\
module private_field_update.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n\
\n\
pub update(user: User): User ->\n\
    user#User { #email = \"next@example.com\" }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies private struct field updates require private spelling.
///
/// Inputs:
/// - A module declaring `User.#email`.
/// - A local function updating `user#User { email = ... }`.
///
/// Output:
/// - Test passes when typechecking reports that the field must be written as
///   `#email`.
///
/// Transformation:
/// - Confirms record updates apply the same visibility rule as field access.
#[test]
fn syntax_output_rejects_bare_update_to_private_struct_field() {
    let diagnostics = check_syntax_output(
        "\
module private_field_update_bad.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n\
\n\
pub update(user: User): User ->\n\
    user#User { email = \"next@example.com\" }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("private field email on struct User must be accessed as #email")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local private struct fields can be pattern matched with `#`.
///
/// Inputs:
/// - A module declaring `User.#email`.
/// - A local case expression matching `User { #email = email }`.
///
/// Output:
/// - Test passes when typechecking accepts the private field pattern inside the
///   defining module.
///
/// Transformation:
/// - Exercises record-pattern visibility metadata during case pattern checking.
#[test]
fn syntax_output_accepts_local_private_struct_field_pattern() {
    let diagnostics = check_syntax_output(
        "\
module private_field_pattern.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n\
\n\
pub read(user: User): String ->\n\
    case user {\n\
      User { #email = email } -> email\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies private struct field patterns require private spelling.
///
/// Inputs:
/// - A module declaring `User.#email`.
/// - A local case expression matching `User { email = email }`.
///
/// Output:
/// - Test passes when typechecking reports that the field must be written as
///   `#email`.
///
/// Transformation:
/// - Confirms record patterns apply the same visibility rule as field access.
#[test]
fn syntax_output_rejects_bare_pattern_for_private_struct_field() {
    let diagnostics = check_syntax_output(
        "\
module private_field_pattern_bad.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n\
\n\
pub read(user: User): String ->\n\
    case user {\n\
      User { email = email } -> email\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("private field email on struct User must be accessed as #email")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local type-alias constructor calls accept tuple field labels.
///
/// Inputs:
/// - A transparent alias `Pair = {:pair, left: Int, right: Int}`.
/// - A constructor call using out-of-order named arguments.
///
/// Output:
/// - Test passes when typechecking accepts the call.
///
/// Transformation:
/// - Exercises alias-derived constructor schemes that retain source tuple
///   labels after the runtime tuple type erases those labels.
#[test]
fn syntax_output_accepts_alias_constructor_field_labels() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_field_labels.\n\
pub type Pair = {:pair, left: Int, right: Int}.\n\
pub make(): Dynamic ->\n\
    Pair(right = 2, left = 1).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local type-alias constructor calls reject unknown field labels.
///
/// Inputs:
/// - A transparent alias `Pair = {:pair, left: Int, right: Int}`.
/// - A constructor call using an unknown named argument.
///
/// Output:
/// - Test passes when typechecking reports an unknown named argument on the
///   source constructor call.
///
/// Transformation:
/// - Routes alias constructor labels through the shared named-argument
///   validator used by ordinary constructor declarations.
#[test]
fn syntax_output_rejects_unknown_alias_constructor_field_label() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_bad_field_label.\n\
pub type Pair = {:pair, left: Int, right: Int}.\n\
pub make(): Dynamic ->\n\
    Pair(first = 1, right = 2).\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| { diag.message == "unknown named argument `first` for call to `Pair`" }),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported type-alias constructor calls preserve field labels.
///
/// Inputs:
/// - A provider interface exporting `Ok[T] = {:ok, value: T}`.
/// - A consumer importing `Ok` and calling it with `value = 1`.
///
/// Output:
/// - Test passes when the imported alias constructor accepts its field label.
///
/// Transformation:
/// - Confirms interface-derived aliases retain constructor parameter names
///   across module boundaries for selected imports.
#[test]
fn syntax_output_accepts_imported_alias_constructor_field_labels() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_constructor_field_labels.\n\
import result.{Ok}.\n\
pub make(): Dynamic ->\n\
    Ok(value = 1).\n\
",
        "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
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

/// Verifies callback return types are covariant.
///
/// Inputs:
/// - A function accepting a callback shaped as `() -> Number`.
/// - A local callback returning `Int`.
///
/// Output:
/// - Test passes when the `Int` callback can be supplied where a `Number`
///   callback is expected.
///
/// Transformation:
/// - Exercises source-level function-value inference and function-type
///   subtyping through a normal local call.
#[test]
fn syntax_output_accepts_covariant_callback_return_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module callback_return_covariance.\n\
\n\
pub accept(callback: () -> Number): Number ->\n\
    callback.().\n\
\n\
pub returns_int(): Int ->\n\
    1.\n\
\n\
pub demo(): Number ->\n\
    accept(returns_int).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies callback parameter types are contravariant.
///
/// Inputs:
/// - A function accepting a callback shaped as `(Int) -> Number`.
/// - A local callback accepting `Number` and returning `Int`.
///
/// Output:
/// - Test passes when the broader `Number` callback parameter can satisfy an
///   `Int` callback slot and the narrower `Int` return can satisfy `Number`.
///
/// Transformation:
/// - Locks the function-type rule that argument positions are contravariant
///   while return positions are covariant.
#[test]
fn syntax_output_accepts_contravariant_callback_parameter_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module callback_parameter_contravariance.\n\
\n\
pub accept(callback: (Int) -> Number): Number ->\n\
    callback.(1).\n\
\n\
pub number_to_int(value: Number): Int ->\n\
    1.\n\
\n\
pub demo(): Number ->\n\
    accept(number_to_int).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies callback parameter covariance is rejected.
///
/// Inputs:
/// - A function accepting a callback shaped as `(Number) -> Int`.
/// - A local callback accepting only `Int` and returning `Int`.
///
/// Output:
/// - Test passes when the typechecker rejects the callback because it cannot
///   safely accept every `Number` input.
///
/// Transformation:
/// - Prevents unsound function-type widening from entering callback dispatch.
#[test]
fn syntax_output_rejects_unsound_callback_subtyping_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module callback_subtyping_bad.\n\
\n\
pub accept(callback: (Number) -> Int): Int ->\n\
    callback.(1).\n\
\n\
pub int_to_int(value: Int): Int ->\n\
    value.\n\
\n\
pub demo(): Int ->\n\
    accept(int_to_int).\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Number but found")),
        "diagnostics: {:?}",
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

/// Verifies string concatenation accepts printable scalar operands.
///
/// Inputs:
/// - A source module returning `String` from `"index: " + index`.
///
/// Output:
/// - Test passes when typechecking reports no diagnostics.
///
/// Transformation:
/// - Exercises the user-facing display concatenation rule that keeps numeric
///   `+` numeric while allowing string-plus-scalar print-path expressions.
#[test]
fn syntax_output_accepts_string_concat_with_int_operand_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module string_concat_int_operand.\n\
\n\
pub label(index: Int): String ->\n\
    \"index: \" + index.\n\
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

/// Verifies explicit type args on imported generic calls constrain results.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function returning `Vector[String]` from `Vector.new[Int]()`.
///
/// Output:
/// - Test passes when typechecking reports the explicit `Int` argument as
///   incompatible with the declared `Vector[String]` return type.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that `Call.type_args`
///   participates in generic interface-call inference instead of being parsed
///   only as syntax metadata.
#[test]
fn syntax_output_remote_generic_call_type_args_constrain_return_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.VectorGenericTypeArgs.\n\
\n\
import std.native.collections.Vector.\n\
\n\
pub wrong(): Vector[String] ->\n\
    Vector.new[Int]().\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found Int")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit generic call arguments can bind HKT constructor params.
///
/// Inputs:
/// - A generic local function with a unary higher-kinded parameter `F[_]`.
/// - A concrete `Option` type constructor supplied as an explicit type
///   argument.
///
/// Output:
/// - No diagnostics; `F[A]` specializes to `Option[Int]`.
///
/// Transformation:
/// - Protects explicit call type-argument parsing from expanding a bare
///   constructor argument into its structural alias body before HKT
///   substitution can apply it.
#[test]
fn syntax_output_explicit_hkt_call_type_arg_binds_constructor_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_type_arg.\n\
\n\
pub type None = Atom[\"none\"].\n\
pub type Some[T] = {Atom[\"some\"], value: T}.\n\
pub type Option[T] = None | Some[T].\n\
\n\
pub identity[F[_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Option[Int]): Option[Int] ->\n\
    identity[Option, Int](value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments honor covariant slot requirements.
///
/// Inputs:
/// - A generic function declaring `F[+_]`.
/// - A covariant `Box[+T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call typechecks.
///
/// Transformation:
/// - Exercises explicit call-site type argument validation against retained
///   callable generic parameter metadata.
#[test]
fn syntax_output_explicit_hkt_call_accepts_covariant_constructor_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_covariant_ok.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub keep[F[+_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Box[Int]): Box[Int] ->\n\
    keep[Box, Int](value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments reject invariant constructors.
///
/// Inputs:
/// - A generic function declaring `F[+_]`.
/// - An invariant `Cell[T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call reports the slot-variance mismatch.
///
/// Transformation:
/// - Prevents explicit type arguments from bypassing the variance contract that
///   trait applications already enforce.
#[test]
fn syntax_output_explicit_hkt_call_rejects_invariant_constructor_for_covariant_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_covariant_bad.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub keep[F[+_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Cell[Int]): Cell[Int] ->\n\
    keep[Cell, Int](value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("explicit type argument `Cell` for `F[+_]` requires slot 1 to be covariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments honor contravariant slot requirements.
///
/// Inputs:
/// - A generic function declaring `F[-_]`.
/// - A contravariant `Sink[-T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call typechecks.
///
/// Transformation:
/// - Covers the negative-variance explicit call path, complementing the
///   covariant slot tests.
#[test]
fn syntax_output_explicit_hkt_call_accepts_contravariant_constructor_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_contravariant_ok.\n\
\n\
pub opaque type Sink[-T] = {value: T}.\n\
\n\
pub keep[F[-_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Sink[Int]): Sink[Int] ->\n\
    keep[Sink, Int](value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments reject opposite variance.
///
/// Inputs:
/// - A generic function declaring `F[-_]`.
/// - A covariant `Box[+T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call reports the slot-variance mismatch.
///
/// Transformation:
/// - Ensures explicit HKT arguments cannot pass a producer-like constructor
///   into a consumer-like slot.
#[test]
fn syntax_output_explicit_hkt_call_rejects_covariant_constructor_for_contravariant_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_contravariant_bad.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub keep[F[-_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Box[Int]): Box[Int] ->\n\
    keep[Box, Int](value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "explicit type argument `Box` for `F[-_]` requires slot 1 to be contravariant"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies source-declared covariance affects function-call assignability.
///
/// Inputs:
/// - `Box[+T]` alias.
/// - Function expecting `Box[Number]`.
/// - Caller passing `Box[Int]`.
///
/// Output:
/// - No diagnostics, because `Int <: Number` and `Box` is covariant.
///
/// Transformation:
/// - Exercises variance metadata collected from Terlan source declarations
///   through ordinary local call checking.
#[test]
fn syntax_output_accepts_covariant_alias_argument_widening_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module covariant_alias_call.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub accept(value: Box[Number]): Box[Number] ->\n\
    value.\n\
\n\
pub demo(value: Box[Int]): Box[Number] ->\n\
    accept(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies unmarked generic aliases stay invariant.
///
/// Inputs:
/// - `Cell[T]` alias without a variance marker.
/// - Function expecting `Cell[Number]`.
/// - Caller passing `Cell[Int]`.
///
/// Output:
/// - Type diagnostic, because invariant parameters reject one-way widening.
///
/// Transformation:
/// - Protects the default generic assignability rule from becoming implicitly
///   covariant.
#[test]
fn syntax_output_rejects_invariant_alias_argument_widening_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module invariant_alias_call.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub accept(value: Cell[Number]): Cell[Number] ->\n\
    value.\n\
\n\
pub demo(value: Cell[Int]): Cell[Number] ->\n\
    accept(value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected Number but found Int/Float")),
        "expected invariant widening diagnostic, got: {:?}",
        diagnostics
    );
}

/// Verifies native vector constructor shorthand infers element type.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function returning `Vector[String]` from `Vector("Alice", "Bob")`.
/// - A second function reading the first value through bracket indexing.
///
/// Output:
/// - Test passes when typechecking produces no diagnostics.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that the explicit
///   constructor declaration on `Vector[T]` participates in vararg constructor
///   inference and the existing `IndexGet` bridge.
#[test]
fn syntax_output_vector_constructor_shorthand_infers_element_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.VectorConstructorShorthand.\n\
\n\
import std.native.collections.Vector.\n\
import type std.native.collections.Vector.Vector.\n\
\n\
pub values(): Vector[String] ->\n\
    Vector(\"Alice\", \"Bob\").\n\
\n\
pub first(): String ->\n\
    let users = Vector(\"Alice\", \"Bob\");\n\
    users[0].\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies empty native vector constructors can use return-type context.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function returning `Vector[String]` from `Vector()`.
///
/// Output:
/// - Test passes when typechecking accepts the empty constructor because the
///   declared return type supplies the missing element type.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that final return
///   unification can still resolve empty vararg constructor calls.
#[test]
fn syntax_output_empty_vector_constructor_uses_return_context() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.EmptyVectorConstructorReturnContext.\n\
\n\
import std.native.collections.Vector.\n\
import type std.native.collections.Vector.Vector.\n\
\n\
pub values(): Vector[String] ->\n\
    Vector().\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies empty portable collection constructors can use return context.
///
/// Inputs:
/// - A source module importing portable `List`, `Set`, and `Map` collection
///   constructors and type aliases.
/// - Functions returning explicit collection types from empty constructor
///   shorthand calls.
///
/// Output:
/// - Test passes when typechecking accepts each empty constructor because the
///   declared return type supplies the otherwise-missing generic arguments.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that final return
///   unification resolves empty portable collection constructor calls without
///   weakening local binding diagnostics.
#[test]
fn syntax_output_empty_portable_collection_constructors_use_return_context() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module collections.EmptyConstructorsReturnContext.\n\
\n\
import std.collections.List.\n\
import std.collections.Set.\n\
import std.collections.Map.\n\
import type std.collections.List.List.\n\
import type std.collections.Set.Set.\n\
import type std.collections.Map.Map.\n\
\n\
pub list_values(): List[String] ->\n\
    List().\n\
\n\
pub set_values(): Set[String] ->\n\
    Set().\n\
\n\
pub map_values(): Map[String, Int] ->\n\
    Map().\n\
",
        "std/collections/map.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies empty native vector constructors require local binding context.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A let binding that assigns `Vector()` without using it in a constraining
///   position.
///
/// Output:
/// - Test passes when typechecking reports a stable expected-type diagnostic.
///
/// Transformation:
/// - Exercises let-expression inference so empty generic constructor calls do
///   not silently bind unconstrained `Vector[T]` values.
#[test]
fn syntax_output_empty_vector_constructor_in_let_requires_expected_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.EmptyVectorConstructorLetContext.\n\
\n\
import std.native.collections.Vector.\n\
\n\
pub value(): Unit ->\n\
    let users = Vector();\n\
    Unit.\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("empty constructor `Vector()` requires an expected type")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies empty portable collection constructors require local context.
///
/// Inputs:
/// - A source module importing portable `List`, `Set`, and `Map` collection
///   constructors.
/// - Let bindings that assign empty constructor calls without constraining
///   their generic arguments.
///
/// Output:
/// - Test passes when typechecking reports stable expected-type diagnostics for
///   every unconstrained empty constructor binding.
///
/// Transformation:
/// - Exercises let-expression inference so empty generic constructor calls for
///   portable collections do not silently bind unconstrained collection values.
#[test]
fn syntax_output_empty_portable_collection_constructors_in_let_require_expected_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module collections.EmptyConstructorsLetContext.\n\
\n\
import std.collections.List.\n\
import std.collections.Set.\n\
import std.collections.Map.\n\
\n\
pub value(): Unit ->\n\
    let list = List();\n\
        set = Set();\n\
        map = Map();\n\
    Unit.\n\
",
        "std/collections/map.terl",
    );

    for constructor in ["List", "Set", "Map"] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(&format!(
                    "empty constructor `{constructor}()` requires an expected type"
                ))),
            "missing {constructor} diagnostic in {:?}",
            diagnostics
        );
    }
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

/// Verifies struct constructor-call field assignment typechecks.
///
/// Inputs:
/// - A module declaring `User`.
/// - A body using canonical `User(name = name)` struct construction.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Exercises the compiler-provided default field constructor for structs
///   that do not declare explicit constructors.
#[test]
fn syntax_output_accepts_default_struct_constructor_call() {
    let diagnostics = check_syntax_output(
        "\
module syntax_struct_constructor_call.\n\
pub struct User {\n\
    name: Binary\n\
}.\n\
pub make(name: Binary): User ->\n\
    User(name = name).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit constructors disable external implicit construction.
///
/// Inputs:
/// - A struct `User`.
/// - An explicit constructor declaration for `User`.
/// - A regular function attempting `User(name = name)`.
///
/// Output:
/// - Diagnostic from explicit-constructor resolution.
///
/// Transformation:
/// - Confirms public construction authority moves to explicit constructor
///   declarations once they exist instead of falling back to the field
///   initializer.
#[test]
fn syntax_output_rejects_default_struct_constructor_when_explicit_constructor_exists() {
    let diagnostics = check_syntax_output(
        "\
module syntax_struct_explicit_constructor_blocks_default.\n\
pub struct User {\n\
    name: Binary\n\
}.\n\
pub constructor User {\n\
    (name: Binary, role: Binary): User ->\n\
        User(name = name)\n\
}.\n\
pub make(name: Binary): User ->\n\
    User(name = name).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("missing required argument `role` for constructor `User`")),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies constructor bodies may use the internal default initializer.
///
/// Inputs:
/// - A struct with an explicit constructor.
/// - The constructor body uses `User(name = name)`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Marks the constructor target as active while checking constructor bodies,
///   allowing the internal initializer without reopening it to other call sites.
#[test]
fn syntax_output_accepts_default_struct_initializer_inside_explicit_constructor() {
    let diagnostics = check_syntax_output(
        "\
module syntax_struct_explicit_constructor_internal_initializer.\n\
pub struct User {\n\
    name: Binary\n\
}.\n\
pub constructor User {\n\
    (display_name: Binary): User ->\n\
        User(name = display_name)\n\
}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies direct named template calls typecheck as generated functions.
///
/// Inputs:
/// - A template declaration with one required property.
/// - A function returning `Page(title = title)`.
///
/// Output:
/// - Test passes when the call returns the template HTML value type without
///   being treated as an unknown constructor.
///
/// Transformation:
/// - Exercises template-call normalization before ordinary constructor and
///   function resolution.
#[test]
fn syntax_output_checks_named_template_call_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_named_call.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    Page(title = title).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies direct positional template calls use declaration prop order.
///
/// Inputs:
/// - A template declaration with one required property and one defaulted
///   property.
/// - A function returning `Page(title)`.
///
/// Output:
/// - Test passes when the positional argument maps to the first property and
///   the omitted trailing property uses its default.
///
/// Transformation:
/// - Confirms generated template functions preserve declaration order in the
///   typechecker.
#[test]
fn syntax_output_checks_positional_template_call_with_default_property() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_positional_call.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary,\n\
    subtitle: String = \"Ready\"\n\
}.\n\
pub view(title: String): Html[Dynamic] ->\n\
    Page(title).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generated template functions reject missing required props.
///
/// Inputs:
/// - A template declaration with one required property.
/// - A function returning `Page()`.
///
/// Output:
/// - Test passes when the template-instantiation diagnostic names the missing
///   required property.
///
/// Transformation:
/// - Confirms direct template-call normalization still uses the shared required
///   prop checker.
#[test]
fn syntax_output_rejects_template_call_missing_required_prop() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_missing_call_prop.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(): Html[Dynamic] ->\n\
    Page().\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("template `Page` instantiation is missing required prop `title`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies defaulted template properties may be omitted.
///
/// Inputs:
/// - A template declaration whose only property has a default value.
/// - A template instantiation that supplies no explicit fields.
///
/// Output:
/// - Test passes when typechecking accepts the omitted defaulted property.
///
/// Transformation:
/// - Uses template property default metadata while validating required
///   instantiation fields.
#[test]
fn syntax_output_accepts_omitted_template_default_property() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_default_instantiation.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: String = \"Untitled\"\n\
}.\n\
pub view(): Html[Dynamic] ->\n\
    Page{}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies omitted template defaults still typecheck against property types.
///
/// Inputs:
/// - A template property declared as `Int` with a binary default.
/// - An instantiation that omits the property and therefore uses the default.
///
/// Output:
/// - Test passes when typechecking reports a default-property mismatch.
///
/// Transformation:
/// - Infers the default expression at instantiation time and unifies it with
///   the declared template property type.
#[test]
fn syntax_output_rejects_mismatched_template_default_property() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_default_bad.\n\
template Page from \"./templates/page.terl.html\" {\n\
    count: Int = \"bad\"\n\
}.\n\
pub view(): Html[Dynamic] ->\n\
    Page{}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("template `Page` default prop `count`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies primitive receiver methods accept named arguments.
///
/// Inputs:
/// - A string receiver call using `pattern = ...`.
///
/// Output:
/// - Test passes when typechecking accepts the compiler-owned primitive method
///   with a named argument.
///
/// Transformation:
/// - Validates primitive receiver method names against the compiler-owned
///   parameter-name table before ordinary primitive unification.
#[test]
fn syntax_output_accepts_primitive_receiver_named_argument() {
    let diagnostics = check_syntax_output(
        "\
module primitive_receiver_named_arg_ok.\n\
pub demo(): Bool ->\n\
    \"hello\".contains(pattern = \"ell\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies primitive receiver named arguments reject unknown names.
///
/// Inputs:
/// - A string receiver call with an unsupported named argument.
///
/// Output:
/// - Test passes when typechecking reports the invalid argument name.
///
/// Transformation:
/// - Runs the same named-argument validation used by declared functions
///   against compiler-owned primitive receiver method metadata.
#[test]
fn syntax_output_rejects_unknown_primitive_receiver_named_argument() {
    let diagnostics = check_syntax_output(
        "\
module primitive_receiver_named_arg_bad.\n\
pub demo(): Bool ->\n\
    \"hello\".contains(needle = \"ell\").\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("unknown named argument `needle` for call to `contains`")),
        "diagnostics: {:?}",
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

/// Verifies syntax HTML blocks typecheck against the public template facade.
///
/// Inputs:
/// - A module importing `std.template.Template`.
/// - A function returning `Template.Html` with an `html { ... }` body.
///
/// Output:
/// - Test passes when the internal syntax HTML value type unifies with the
///   public standard-library template fragment type.
///
/// Transformation:
/// - Parses through the formal syntax-output path and exercises return-type
///   unification for `Html[Dynamic]` against `Template.Html`.
#[test]
fn syntax_output_html_blocks_assign_to_template_html_facade() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_html_blocks.\n\
import std.template.Template.\n\
pub view(title: Binary): Template.Html ->\n\
    html {\n\
        <section>{title}</section>\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
