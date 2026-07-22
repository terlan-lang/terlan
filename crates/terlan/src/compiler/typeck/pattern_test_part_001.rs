use super::test_support::*;
use crate::terlan_typeck::DiagSeverity;

#[test]
fn syntax_output_accepts_typed_binary_layout_case_pattern() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_pattern_typecheck.\n\
\n\
import std.vm.BitString.{BitString}.\n\
\n\
pub decode(input: BitString): Int ->\n\
    case input {\n\
        Binary[big] { source_port: UInt[16], payload: Rest } -> source_port + 1;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_accepts_typed_binary_layout_function_head_pattern() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_function_head_pattern_typecheck.\n\
\n\
pub decode(Binary[big] { source_port: UInt[16], payload: Rest }): Int ->\n\
    source_port + 1.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies generic comparator callbacks keep their declared return type.
///
/// Inputs:
/// - A local `Option[T]` alias and a `Comparison` result alias.
/// - A generic `compare_option` function that accepts `(T, T) ->
///   Comparison` and calls it from a nested `case` branch.
///
/// Output:
/// - Test passes when the syntax-output typechecker accepts the callback
///   result as `Comparison` rather than inferring the contained `T`.
///
/// Transformation:
/// - Parses the formal syntax-output path, infers the higher-order
///   callback invocation inside pattern-refined branches, and validates the
///   enclosing function return annotation.
#[test]
fn syntax_output_generic_comparator_callback_preserves_declared_return_type() {
    let diagnostics = check_syntax_output(
        "\
module comparator_callback_return.\n\
pub type Comparison = Atom[\"lt\"] | Atom[\"eq\"] | Atom[\"gt\"].\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], T}.\n\
pub compare_option(compare: (T, T) -> Comparison, left: Option[T], right: Option[T]): Comparison ->\n\
    case left {\n\
        Atom[\"none\"] ->\n\
            case right {\n\
                Atom[\"none\"] -> Atom[\"eq\"];\n\
                {Atom[\"some\"], _} -> Atom[\"lt\"]\n\
            };\n\
\n\
        {Atom[\"some\"], left_value} ->\n\
            case right {\n\
                Atom[\"none\"] -> Atom[\"gt\"];\n\
                {Atom[\"some\"], right_value} -> compare(left_value, right_value)\n\
            }\n\
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
fn syntax_output_list_cons_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module list_cons_patterns.\n\
pub prepend(head: Int, tail: List[Int]): List[Int] ->\n\
    [head | tail].\n\
\n\
pub head(input: List[Int]): Int ->\n\
    case input {\n\
        [head | _tail] -> head;\n\
        [] -> 0\n\
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
fn syntax_output_binds_case_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_case_patterns.\n\
pub type Some = {Atom[\"some\"], Int}.\n\
pub unwrap(input: Some): Int ->\n\
    case input {\n\
        Some(value) -> value\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported generic constructor patterns bind substituted payloads.
///
/// Inputs:
/// - A module importing `std.core.Option.{Option, Some, None}`.
/// - A case expression matching `Option[Int]` with `Some(index)`.
///
/// Output:
/// - Test passes when `index` is inferred as `Int` and can participate in
///   string-plus-scalar concatenation.
///
/// Transformation:
/// - Loads checked-in std summaries, resolves imported alias constructors, and
///   applies constructor-return substitutions before pattern locals are
///   inserted into the case branch environment.
#[test]
fn syntax_output_imported_generic_constructor_pattern_binds_payload_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module option_pattern_payload_type.\n\
\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub label(input: Option[Int]): String ->\n\
    case input {\n\
        Some(index) -> \"index: \" + index;\n\
        None -> \"none\"\n\
    }.\n\
",
        "std/core/Option.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies case expressions over finite union aliases must be exhaustive.
///
/// Inputs:
/// - A module importing `Option`, `Some`, and `None`.
/// - A `case Option[Int]` expression that handles only `Some`.
///
/// Output:
/// - Test passes when typechecking emits a hard non-exhaustive case error
///   naming the missing `None` variant.
///
/// Transformation:
/// - Locks `case` exhaustiveness to the same finite-union model used by
///   function-clause patterns so runtime match failure is not accepted for
///   known closed variants.
#[test]
fn syntax_output_rejects_non_exhaustive_option_case() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module option_case_exhaustiveness.\n\
\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub label(input: Option[Int]): String ->\n\
    case input {\n\
        Some(index) -> \"index: \" + index\n\
    }.\n\
",
        "std/core/Option.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic.severity, DiagSeverity::Error)
                && diagnostic
                    .message
                    .contains("non-exhaustive case expression")
                && diagnostic.message.contains("None")
        }),
        "expected non-exhaustive Option case diagnostic, got: {:?}",
        diagnostics
    );
}

/// Verifies imported constructor-pattern payloads survive call boundaries.
///
/// Inputs:
/// - A module importing console output, native vectors, and `Option`.
/// - A `main` function matching the result of a local function returning
///   `Option[Int]`.
///
/// Output:
/// - Test passes when `Some(index)` binds `index` as `Int` through the local
///   function return annotation and `println(\"...\" + index)` typechecks.
///
/// Transformation:
/// - Exercises the same import/call/case/string-concat shape used by external
///   binary-search examples so constructor payload substitutions are preserved
///   across ordinary local call inference.
#[test]
fn syntax_output_option_pattern_payload_survives_local_call_boundary() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module option_pattern_call_boundary.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
    let users = Vector(1, 2, 3);\n\
    case binarySearch(users, 2, 0, users.len() - 1) {\n\
        Some(index) -> println(\"Element found at index: \" + index);\n\
        None -> println(\"Element not found\")\n\
    }.\n\
\n\
binarySearch(users: Vector[Int], target: Int, low: Int = 0, high: Int = 100): Option[Int] ->\n\
    Some(target).\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies typed string captures bind their annotated type.
///
/// Inputs:
/// - A case expression over `String` with `${id: Int}` in the pattern.
///
/// Output:
/// - Test passes when the capture variable typechecks as `Int` inside the
///   matched branch.
///
/// Transformation:
/// - Exercises parser syntax-output capture metadata through the ordinary
///   pattern typechecker without requiring VM capture execution.
#[test]
fn syntax_output_string_capture_pattern_binds_explicit_type() {
    let diagnostics = check_syntax_output(
        "\
module string_capture_explicit_type.\n\
\n\
pub capture(path: String): Int ->\n\
    case path {\n\
        \"users/${id: Int}.txt\" -> id + 1;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies untyped string captures default to `String`.
///
/// Inputs:
/// - A case expression with `${slug}` and a string-returning branch.
///
/// Output:
/// - Test passes when the capture variable is available as a string-like value.
///
/// Transformation:
/// - Locks the first omitted-annotation rule before richer contextual capture
///   inference is implemented.
#[test]
fn syntax_output_string_capture_pattern_defaults_untyped_capture_to_string() {
    let diagnostics = check_syntax_output(
        "\
module string_capture_default_type.\n\
\n\
pub capture(path: String): String ->\n\
    case path {\n\
        \"posts/${slug}.html\" -> slug;\n\
        _ -> \"\"\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies duplicate capture names are rejected before lowering.
///
/// Inputs:
/// - A string pattern that repeats `${id}` in one pattern.
///
/// Output:
/// - Test passes when typechecking emits a stable duplicate-capture diagnostic.
///
/// Transformation:
/// - Prevents ambiguous branch-local capture bindings from reaching CoreIR or
///   runtime matching.
#[test]
fn syntax_output_string_capture_pattern_rejects_duplicate_capture_names() {
    let diagnostics = check_syntax_output(
        "\
module string_capture_duplicate_name.\n\
\n\
pub capture(path: String): String ->\n\
    case path {\n\
        \"users/${id}/${id}\" -> id;\n\
        _ -> \"\"\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("duplicate string capture name id")
        }),
        "expected duplicate capture diagnostic, got: {:?}",
        diagnostics
    );
}

/// Verifies invalid capture annotations are rejected before lowering.
///
/// Inputs:
/// - A string pattern whose capture annotation is expression-shaped.
///
/// Output:
/// - Test passes when typechecking emits a stable invalid-annotation diagnostic.
///
/// Transformation:
/// - Routes capture annotations through the shared type parser instead of
///   accepting arbitrary text.
#[test]
fn syntax_output_string_capture_pattern_rejects_invalid_capture_annotation() {
    let diagnostics = check_syntax_output(
        "\
module string_capture_invalid_annotation.\n\
\n\
pub capture(path: String): Int ->\n\
    case path {\n\
        \"users/${id: 1 + 2}.txt\" -> 1;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("invalid string capture type annotation `1 + 2`")
        }),
        "expected invalid capture annotation diagnostic, got: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_inline_option_constructor_case_scrutinee_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module inline_option_constructor_case.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
    case Some(Vector(1, 2, 3)) {\n\
        Some(values) -> println(Int.to_string(values.len()));\n\
        None -> println(\"missing\")\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies inline `Result` constructors widen to the visible union alias.
///
/// Inputs:
/// - A module importing native vectors and `std.core.Result`.
/// - A case expression whose scrutinee is `Ok(Vector(...))` and whose branches
///   include both `Ok` and `Err` constructor patterns.
///
/// Output:
/// - Test passes when the `Ok` payload keeps its native vector receiver type
///   and the `Err` payload can be inferred independently.
///
/// Transformation:
/// - Exercises the same constructor-scrutinee widening as `Option`, but with a
///   two-parameter alias so the implementation cannot accidentally be
///   special-cased to one generic argument.
#[test]
fn syntax_output_inline_result_constructor_case_scrutinee_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module inline_result_constructor_case.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
import std.core.Result.{Result, Ok, Err}.\n\
\n\
pub main(): Unit ->\n\
    case Ok(Vector(1, 2, 3)) {\n\
        Ok(values) -> println(Int.to_string(values.len()));\n\
        Err(code) -> println(Int.to_string(code))\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic `Result` helpers can return constructor payloads.
///
/// Inputs:
/// - A module importing checked-in `std.core.Result` summaries.
/// - A generic `with_default` helper matching `Ok(x)` and `Err(_)`.
///
/// Output:
/// - Test passes when `Ok(x)` binds `x` as the success payload type `A`
///   instead of the whole `Result[A, E]` container.
///
/// Transformation:
/// - Exercises constructor-pattern matching against a transparent two-argument
///   union alias and then unifies branch results with the annotated generic
///   return type.
#[test]
fn syntax_output_result_constructor_pattern_binds_payload_in_generic_helper() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module result_pattern_payload_helper.\n\
\n\
import std.core.Result.{Result, Ok, Err}.\n\
\n\
pub with_default(value: Result[A, E], default: A): A ->\n\
    case value {\n\
        Ok(x) -> x;\n\
        Err(_reason) -> default\n\
    }.\n\
",
        "std/core/Result.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local generic union aliases bind constructor payloads.
///
/// Inputs:
/// - A module declaring local `Ok`, `Err`, and `Result` aliases.
/// - A generic helper returning the payload from `Ok(x)`.
///
/// Output:
/// - Test passes when local transparent union aliases refine constructor
///   payload bindings the same way imported std aliases do.
///
/// Transformation:
/// - Removes import-summary loading from the regression so the typechecker
///   proves the core alias/pattern path independently.
#[test]
fn syntax_output_local_result_constructor_pattern_binds_payload_in_generic_helper() {
    let diagnostics = check_syntax_output(
        "\
module local_result_pattern_payload_helper.\n\
\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
pub type Err[E] = {Atom[\"error\"], reason: E}.\n\
pub type Result[T, E] = Ok[T] | Err[E].\n\
\n\
pub with_default(value: Result[A, E], default: A): A ->\n\
    case value {\n\
        Ok(x) -> x;\n\
        Err(_reason) -> default\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies typed function-head tuple patterns refine their local bindings.
///
/// Inputs:
/// - A function-head parameter pattern `{left, right}: {Int, Int}`.
/// - A body that uses the destructured locals in an `Int` expression.
///
/// Output:
/// - Test passes when `left` and `right` are inserted into the function body
///   environment as `Int`, not as the whole tuple type or unrefined `Dynamic`.
///
/// Transformation:
/// - Exercises the function-head parameter typechecker path directly, before
///   CoreIR or VM lowering, so typed destructuring cannot silently depend on
///   runtime coercion.
#[test]
fn syntax_output_function_head_tuple_pattern_refines_local_bindings() {
    let diagnostics = check_syntax_output(
        "\
module function_head_tuple_binding_type.\n\
\n\
pub add({left, right}: {Int, Int}): Int ->\n\
    left + right.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies recursive vector binary search typechecks end to end.
///
/// Inputs:
/// - A module importing console output, native vectors, and `Option`.
/// - A recursive binary search over `Vector[Int]`.
///
/// Output:
/// - Test passes when the corrected binary-search shape typechecks without
///   diagnostics.
///
/// Transformation:
/// - Exercises vector constructor shorthand, receiver `len`, bracket indexing,
///   recursive local calls, `if` fallback clauses, `Option` constructor calls,
///   constructor-pattern matching, and string-plus-scalar concatenation in one
///   source-level algorithm.
#[test]
fn syntax_output_recursive_vector_binary_search_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module vector_binary_search_typecheck.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
    let users = Vector(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);\n\
    case binarySearch(users, 5, 0, users.len() - 1) {\n\
        Some(index) -> println(\"Element found at index: \" + index);\n\
        None -> println(\"Element not found\")\n\
    }.\n\
\n\
binarySearch(users: Vector[Int], target: Int, low: Int = 0, high: Int = 100): Option[Int] ->\n\
    if {\n\
        low > high -> None;\n\
        _ ->\n\
            let mid = low + ((high - low) / 2);\n\
            let value = users[mid];\n\
            case value == target {\n\
                true -> Some(mid);\n\
                false ->\n\
                    if {\n\
                        value < target -> binarySearch(users, target, mid + 1, high);\n\
                        _ -> binarySearch(users, target, low, mid - 1)\n\
                    }\n\
            }\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic lower-bound search typechecks with comparator callbacks.
///
/// Inputs:
/// - A generic `Vector[T]`.
/// - A `target: T`.
/// - A comparator callback `(T, T) -> Comparison`.
///
/// Output:
/// - Test passes when recursive lower-bound logic typechecks for generic
///   values without requiring primitive `<` on `T`.
///
/// Transformation:
/// - Exercises generic callback invocation, imported `Comparison` constructor
///   patterns, vector indexing, integer midpoint arithmetic, and recursive
///   calls in a sorting-adjacent binary insertion-point algorithm.
#[test]
fn syntax_output_generic_lower_bound_with_comparator_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_lower_bound_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub lower_bound(items: Vector[T], target: T, compare: (T, T) -> Comparison, low: Int, high: Int): Int ->\n\
    if {\n\
        low > high -> low;\n\
        _ ->\n\
            let mid = low + ((high - low) / 2);\n\
            let value = items[mid];\n\
            case compare(value, target) {\n\
                Lt -> lower_bound(items, target, compare, mid + 1, high);\n\
                Eq -> mid;\n\
                Gt -> lower_bound(items, target, compare, low, mid - 1)\n\
            }\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic sortedness checks typecheck with comparator callbacks.
///
/// Inputs:
/// - A generic `Vector[T]`.
/// - A comparator callback `(T, T) -> Comparison`.
/// - Current and final indexes.
///
/// Output:
/// - Test passes when recursive adjacent-pair checking returns `Bool`.
///
/// Transformation:
/// - Exercises a generic sorting validation algorithm that compares adjacent
///   vector elements through `Comparison` instead of target-specific operators.
#[test]
fn syntax_output_generic_is_sorted_with_comparator_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_is_sorted_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub is_sorted(items: Vector[T], compare: (T, T) -> Comparison, index: Int, last: Int): Bool ->\n\
    if {\n\
        index >= last -> true;\n\
        _ ->\n\
            let left = items[index];\n\
            let right = items[index + 1];\n\
            case compare(left, right) {\n\
                Gt -> false;\n\
                Eq -> is_sorted(items, compare, index + 1, last);\n\
                Lt -> is_sorted(items, compare, index + 1, last)\n\
            }\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic minimum-index selection typechecks.
///
/// Inputs:
/// - A generic `Vector[T]`.
/// - A comparator callback `(T, T) -> Comparison`.
/// - Cursor, current best index, and high bound.
///
/// Output:
/// - Test passes when the recursive selection-sort helper returns the best
///   index as `Int`.
///
/// Transformation:
/// - Exercises generic vector indexing at two positions, comparator callback
///   dispatch, comparison-result case analysis, and recursive index updates in
///   the core selection-sort helper shape.
#[test]
fn syntax_output_generic_selection_min_index_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_selection_min_index_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub min_index(items: Vector[T], compare: (T, T) -> Comparison, index: Int, best: Int, high: Int): Int ->\n\
    if {\n\
        index > high -> best;\n\
        _ ->\n\
            let candidate = items[index];\n\
            let current_best = items[best];\n\
            case compare(candidate, current_best) {\n\
                Lt -> min_index(items, compare, index + 1, index, high);\n\
                Eq -> min_index(items, compare, index + 1, best, high);\n\
                Gt -> min_index(items, compare, index + 1, best, high)\n\
            }\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic compare-and-swap helper typechecks.
///
/// Inputs:
/// - A mutable `Vector[T]`.
/// - Two indexes.
/// - A comparator callback `(T, T) -> Comparison`.
///
/// Output:
/// - Test passes when a sorting helper can compare two generic elements and
///   call the mutable vector `swap` receiver method.
///
/// Transformation:
/// - Exercises mutation-oriented algorithm validation without relying on
///   bracket assignment, using the current `std.native.collections.Vector`
///   receiver method contract directly.
#[test]
fn syntax_output_generic_compare_and_swap_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_compare_and_swap_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub compare_and_swap(items: Vector[T], left_index: Int, right_index: Int, compare: (T, T) -> Comparison): Unit ->\n\
    let left = items[left_index];\n\
    let right = items[right_index];\n\
    case compare(left, right) {\n\
        Gt -> items.swap(left_index, right_index);\n\
        Eq -> Unit;\n\
        Lt -> Unit\n\
    }.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
