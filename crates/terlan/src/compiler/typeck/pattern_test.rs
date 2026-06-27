use super::test_support::*;

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
pub type Comparison = :lt | :eq | :gt.\n\
pub type Option[T] = :none | {:some, T}.\n\
pub compare_option(compare: (T, T) -> Comparison, left: Option[T], right: Option[T]): Comparison ->\n\
    case left {\n\
        :none ->\n\
            case right {\n\
                :none -> :eq;\n\
                {:some, _} -> :lt\n\
            };\n\
\n\
        {:some, left_value} ->\n\
            case right {\n\
                :none -> :gt;\n\
                {:some, right_value} -> compare(left_value, right_value)\n\
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
pub type Some = {:some, Int}.\n\
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
        "std/core/option.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
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
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
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
        "std/native/collections/vector.terl",
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
        "std/native/collections/vector.terl",
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
        "std/core/result.terl",
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
                value = users[mid];\n\
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
        "std/native/collections/vector.terl",
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
                value = items[mid];\n\
            case compare(value, target) {\n\
                Lt -> lower_bound(items, target, compare, mid + 1, high);\n\
                Eq -> mid;\n\
                Gt -> lower_bound(items, target, compare, low, mid - 1)\n\
            }\n\
    }.\n\
",
        "std/native/collections/vector.terl",
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
                right = items[index + 1];\n\
            case compare(left, right) {\n\
                Gt -> false;\n\
                Eq -> is_sorted(items, compare, index + 1, last);\n\
                Lt -> is_sorted(items, compare, index + 1, last)\n\
            }\n\
    }.\n\
",
        "std/native/collections/vector.terl",
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
                current_best = items[best];\n\
            case compare(candidate, current_best) {\n\
                Lt -> min_index(items, compare, index + 1, index, high);\n\
                Eq -> min_index(items, compare, index + 1, best, high);\n\
                Gt -> min_index(items, compare, index + 1, best, high)\n\
            }\n\
    }.\n\
",
        "std/native/collections/vector.terl",
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
        right = items[right_index];\n\
    case compare(left, right) {\n\
        Gt -> items.swap(left_index, right_index);\n\
        Eq -> Unit;\n\
        Lt -> Unit\n\
    }.\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic bubble-sort pass helper typechecks.
///
/// Inputs:
/// - A mutable `Vector[T]`.
/// - A comparator callback `(T, T) -> Comparison`.
/// - Current and final indexes.
///
/// Output:
/// - Test passes when one recursive bubble-sort pass typechecks.
///
/// Transformation:
/// - Exercises recursive mutation-oriented algorithm code that compares
///   adjacent generic values, conditionally swaps them, and advances the
///   cursor without returning a new collection value.
#[test]
fn syntax_output_generic_bubble_pass_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_bubble_pass_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub bubble_pass(items: Vector[T], compare: (T, T) -> Comparison, index: Int, last: Int): Unit ->\n\
    if {\n\
        index >= last -> Unit;\n\
        _ ->\n\
            let left = items[index];\n\
                right = items[index + 1];\n\
            case compare(left, right) {\n\
                Gt -> swap_then_bubble(items, compare, index, last);\n\
                Eq -> bubble_pass(items, compare, index + 1, last);\n\
                Lt -> bubble_pass(items, compare, index + 1, last)\n\
            }\n\
    }.\n\
\n\
pub swap_then_bubble(items: Vector[T], compare: (T, T) -> Comparison, index: Int, last: Int): Unit ->\n\
    let _swap = items.swap(index, index + 1);\n\
    bubble_pass(items, compare, index + 1, last).\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic insertion shift helper typechecks.
///
/// Inputs:
/// - A mutable `Vector[T]`.
/// - A value being inserted.
/// - A comparator callback `(T, T) -> Comparison`.
/// - The current insertion cursor.
///
/// Output:
/// - Test passes when a recursive insertion-sort shift helper can move values
///   with `set_at` and place the inserted value.
///
/// Transformation:
/// - Exercises generic element reads, mutable receiver `set_at`, comparator
///   case analysis, and recursion in an insertion-sort-shaped algorithm.
#[test]
fn syntax_output_generic_insertion_shift_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_insertion_shift_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub insert_at(items: Vector[T], value: T, compare: (T, T) -> Comparison, index: Int): Unit ->\n\
    if {\n\
        index <= 0 -> items.set_at(0, value);\n\
        _ ->\n\
            let previous = items[index - 1];\n\
            case compare(previous, value) {\n\
                Gt -> shift_then_insert(items, value, compare, index, previous);\n\
                Eq -> items.set_at(index, value);\n\
                Lt -> items.set_at(index, value)\n\
            }\n\
    }.\n\
\n\
pub shift_then_insert(items: Vector[T], value: T, compare: (T, T) -> Comparison, index: Int, previous: T): Unit ->\n\
    let _move = items.set_at(index, previous);\n\
    insert_at(items, value, compare, index - 1).\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a full generic selection-sort driver typechecks.
///
/// Inputs:
/// - A mutable `Vector[T]`.
/// - A comparator callback `(T, T) -> Comparison`.
///
/// Output:
/// - Test passes when selection-sort pass composition returns the sorted
///   `Vector[T]` value.
///
/// Transformation:
/// - Exercises a complete recursive selection-sort shape: vector length,
///   generic minimum-index search, mutable `swap`, helper-based sequencing,
///   and final vector return.
#[test]
fn syntax_output_generic_selection_sort_driver_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_selection_sort_driver_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub selection_sort(values: Vector[T], compare: (T, T) -> Comparison): Vector[T] ->\n\
    let n = values.len();\n\
    selection_pass(values, compare, 0, n).\n\
\n\
pub selection_pass(values: Vector[T], compare: (T, T) -> Comparison, index: Int, length: Int): Vector[T] ->\n\
    if {\n\
        index >= length - 1 -> values;\n\
        _ ->\n\
            let best = min_index(values, compare, index, index + 1, length);\n\
            selection_swap_then_pass(values, compare, index, best, length)\n\
    }.\n\
\n\
pub selection_swap_then_pass(values: Vector[T], compare: (T, T) -> Comparison, index: Int, best: Int, length: Int): Vector[T] ->\n\
    let _swap = values.swap(index, best);\n\
    selection_pass(values, compare, index + 1, length).\n\
\n\
pub min_index(values: Vector[T], compare: (T, T) -> Comparison, best: Int, cursor: Int, length: Int): Int ->\n\
    if {\n\
        cursor >= length -> best;\n\
        _ ->\n\
            let candidate = values[cursor];\n\
                current = values[best];\n\
            case compare(candidate, current) {\n\
                Lt -> min_index(values, compare, cursor, cursor + 1, length);\n\
                Eq -> min_index(values, compare, best, cursor + 1, length);\n\
                Gt -> min_index(values, compare, best, cursor + 1, length)\n\
            }\n\
    }.\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a full generic insertion-sort driver typechecks.
///
/// Inputs:
/// - A mutable `Vector[T]`.
/// - A comparator callback `(T, T) -> Comparison`.
///
/// Output:
/// - Test passes when insertion-sort pass composition returns the sorted
///   `Vector[T]` value.
///
/// Transformation:
/// - Exercises vector length, generic indexed reads, mutable `set_at`,
///   recursive insertion shifts, Unit sequencing through explicit helper
///   calls, and final vector return.
#[test]
fn syntax_output_generic_insertion_sort_driver_typechecks() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module generic_insertion_sort_driver_typecheck.\n\
\n\
import std.native.collections.Vector.\n\
import std.core.Ordering.{Comparison, Lt, Eq, Gt}.\n\
\n\
pub insertion_sort(values: Vector[T], compare: (T, T) -> Comparison): Vector[T] ->\n\
    let length = values.len();\n\
    insertion_outer(values, compare, 1, length).\n\
\n\
pub insertion_outer(values: Vector[T], compare: (T, T) -> Comparison, index: Int, length: Int): Vector[T] ->\n\
    if {\n\
        index >= length -> values;\n\
        _ ->\n\
            let value = values[index];\n\
            insertion_shift_then_outer(values, value, compare, index, length)\n\
    }.\n\
\n\
pub insertion_shift_then_outer(values: Vector[T], value: T, compare: (T, T) -> Comparison, index: Int, length: Int): Vector[T] ->\n\
    let _inserted = insert_at(values, value, compare, index);\n\
    insertion_outer(values, compare, index + 1, length).\n\
\n\
pub insert_at(values: Vector[T], value: T, compare: (T, T) -> Comparison, index: Int): Unit ->\n\
    if {\n\
        index <= 0 -> values.set_at(0, value);\n\
        _ ->\n\
            let previous = values[index - 1];\n\
            case compare(previous, value) {\n\
                Gt -> insertion_shift_step(values, value, compare, index, previous);\n\
                Eq -> values.set_at(index, value);\n\
                Lt -> values.set_at(index, value)\n\
            }\n\
    }.\n\
\n\
pub insertion_shift_step(values: Vector[T], value: T, compare: (T, T) -> Comparison, index: Int, previous: T): Unit ->\n\
    let _move = values.set_at(index, previous);\n\
    insert_at(values, value, compare, index - 1).\n\
",
        "std/native/collections/vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_refines_case_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_case_guards.\n\
pub to_int(value: Dynamic): Int ->\n\
    case value {\n\
        x when is_type(x, Int) -> x\n\
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
fn syntax_output_refines_function_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_function_guards.\n\
pub to_int(value) when is_type(value, Int) -> value.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
