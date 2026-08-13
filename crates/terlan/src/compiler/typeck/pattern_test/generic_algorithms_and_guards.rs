use super::*;

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
pub(super) fn syntax_output_generic_bubble_pass_typechecks() {
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
            let right = items[index + 1];\n\
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
        "std/native/collections/Vector.terl",
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
pub(super) fn syntax_output_generic_insertion_shift_typechecks() {
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
        "std/native/collections/Vector.terl",
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
pub(super) fn syntax_output_generic_selection_sort_driver_typechecks() {
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
            let current = values[best];\n\
            case compare(candidate, current) {\n\
                Lt -> min_index(values, compare, cursor, cursor + 1, length);\n\
                Eq -> min_index(values, compare, best, cursor + 1, length);\n\
                Gt -> min_index(values, compare, best, cursor + 1, length)\n\
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
pub(super) fn syntax_output_generic_insertion_sort_driver_typechecks() {
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
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_refines_case_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_case_guards.\n\
pub to_int(value: Dynamic): Int ->\n\
    case value {\n\
        x where is_type(x, Int) -> x\n\
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
pub(super) fn syntax_output_refines_function_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_function_guards.\n\
pub to_int(value) where is_type(value, Int) -> value.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_refines_where_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_where_guards.\n\
pub case_to_int(value: Dynamic): Int ->\n\
    case value {\n\
        x where is_type(x, Int) -> x\n\
    }.\n\
\n\
pub function_to_int(value) where is_type(value, Int) -> value.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_non_bool_case_guard() {
    let diagnostics = check_syntax_output(
        "\
module syntax_non_bool_case_guard.\n\
pub classify(value: Int): Int ->\n\
    case value {\n\
        x where x -> x;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("case guard expected Bool found Int")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_non_bool_function_guard() {
    let diagnostics = check_syntax_output(
        "\
module syntax_non_bool_function_guard.\n\
pub classify(value) where 1 -> value;\n\
classify(_value) -> 0.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message.contains("case guard expected Bool found 1")
                || diag
                    .message
                    .contains("function guard expected Bool found 1")
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_impure_case_guard_assignment() {
    let diagnostics = check_syntax_output(
        "\
module syntax_impure_case_guard_assignment.\n\
pub classify(value: Int): Int ->\n\
    let items = [1];\n\
    case value {\n\
        x where items[0] = 2 -> x;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("case guard must be pure; found indexed assignment")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_impure_case_guard_template_call() {
    let diagnostics = check_syntax_output(
        "\
module syntax_impure_case_guard_template_call.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub classify(value: Int): Int ->\n\
    case value {\n\
        x where Page(title = \"guard\") -> x;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("case guard must be pure; found template instantiation")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_impure_case_guard_file_exists_call() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module syntax_impure_case_guard_file_exists.\n\
\n\
import std.io.File.\n\
\n\
pub classify(value: Int): Int ->\n\
    case value {\n\
        x where File.exists(\"/tmp/terlan.txt\") -> x;\n\
        _ -> 0\n\
    }.\n\
",
        "std/io/File.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("case guard must be pure; found effectful imported function call")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_accepts_inferred_pure_helper_case_guard() {
    let diagnostics = check_syntax_output(
        "\
module syntax_pure_helper_case_guard.\n\
\n\
is_visible(value: Int): Bool ->\n\
    value > 0.\n\
\n\
pub classify(value: Int): Int ->\n\
    case value {\n\
        x where is_visible(x) -> x;\n\
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

#[test]
pub(super) fn syntax_output_rejects_impure_function_guard_assignment() {
    let diagnostics = check_syntax_output(
        "\
module syntax_impure_function_guard_assignment.\n\
pub classify(value) where value[0] = 2 -> 1;\n\
classify(_value) -> 0.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function guard must be pure; found indexed assignment")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_accepts_inferred_pure_helper_function_guard() {
    let diagnostics = check_syntax_output(
        "\
module syntax_pure_helper_function_guard.\n\
\n\
is_visible(value: Int): Bool ->\n\
    value > 0.\n\
\n\
pub classify(value) where is_visible(value) -> value;\n\
classify(_value) -> 0.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_impure_function_guard_console_println_call() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module syntax_impure_function_guard_console_println.\n\
\n\
import std.io.Console.\n\
\n\
pub classify(value) where Console.println(\"guard\") -> value;\n\
classify(_value) -> 0.\n\
",
        "std/io/Console.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function guard must be pure; found effectful imported function call")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
pub(super) fn syntax_output_rejects_impure_function_guard_template_call() {
    let diagnostics = check_syntax_output(
        "\
module syntax_impure_function_guard_template_call.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub classify(value) where Page(title = \"guard\") -> value;\n\
classify(_value) -> 0.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function guard must be pure; found template instantiation")),
        "diagnostics: {:?}",
        diagnostics
    );
}
