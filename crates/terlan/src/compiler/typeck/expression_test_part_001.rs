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

/// Verifies different integer literals remain comparable through a binding.
///
/// Inputs:
/// - A local binding initialized with integer literal `0`.
/// - A second local binding initialized with integer literal `2`.
/// - Later numeric comparisons against the literal and bound value.
///
/// Output:
/// - Test passes when typechecking accepts the comparison without forcing the
///   two literal values to unify exactly.
///
/// Transformation:
/// - Locks comparison typing to widen literal integers for comparison while
///   preserving exact literal types for pattern matching.
#[test]
fn syntax_output_accepts_bound_integer_literal_comparison() {
    let diagnostics = check_syntax_output(
        "\
module literal_comparison.\n\
\n\
pub decide(): Bool ->\n\
    let a = 0; let b = 2; if { a > 1 -> true; b > a -> true; _ -> false }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
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
/// - A local function updating `user#User { #email: ... }`.
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
    user#User { #email: \"next@example.com\" }.\n\
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
/// - A local function updating `user#User { email: ... }`.
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
    user#User { email: \"next@example.com\" }.\n\
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
/// - A local case expression matching `User { #email: email }`.
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
      User { #email: email } -> email\n\
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
/// - A local case expression matching `User { email: email }`.
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
      User { email: email } -> email\n\
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
/// - A transparent alias `Pair = {Atom["pair"], left: Int, right: Int}`.
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
pub type Pair = {Atom[\"pair\"], left: Int, right: Int}.\n\
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
/// - A transparent alias `Pair = {Atom["pair"], left: Int, right: Int}`.
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
pub type Pair = {Atom[\"pair\"], left: Int, right: Int}.\n\
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
/// - A provider interface exporting `Ok[T] = {Atom["ok"], value: T}`.
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
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
    callback().\n\
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
    callback(1).\n\
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
    callback(1).\n\
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
        {Atom[\"ok\"], value} -> value;\n\
        Atom[\"stop\"] -> 0\n\
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
        {Atom[\"ok\"], value} -> value\n\
    catch\n\
        Atom[\"error\"] -> 0\n\
    }.\n\
risky(): {Atom[\"ok\"], Int} ->\n\
    {Atom[\"ok\"], 1}.\n\
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
