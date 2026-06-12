//! Core string conversion traits.
//!
//! `std.core.String` defines the canonical conversion contracts for values
//! that render to or parse from Terlan `String` values. `String` is the
//! source-level UTF-8 text type; each backend owns its runtime representation.

module std.core.String.

import std.core.Option.{None, Some}.
import type std.core.Option.Option.
import std.core.Ordering.{Eq, Gt, Lt}.

/// Describes values that can be rendered as a `String`.
///
/// Input: a value of type `T`.
/// Output: a UTF-8 `String`.
/// Transformation: converts the input value into its human-readable string
/// representation without changing the input value.
///
/// ```terlan
/// module show_trait_example.
///
/// pub trait Show[T] {
///     to_string(value: T): String.
/// }.
/// ```
pub trait Show[T] {
    to_string(value: T): String.
}.

/// Describes values that can be parsed from a `String`.
///
/// Input: a UTF-8 `String`.
/// Output: `Option[T]`, where `Some(value)` means parsing succeeded and
/// `None` means the string is not a valid representation of `T`.
/// Transformation: validates the input string and constructs a typed value
/// only when the string matches the target type's accepted representation.
///
/// ```terlan
/// module parse_trait_example.
///
/// pub trait Parse[T] {
///     from_string(value: String): Option[T].
/// }.
/// ```
pub trait Parse[T] {
    from_string(value: String): Option[T].
}.

/// Returns whether two `String` values are equal.
///
/// Input: two UTF-8 `String` values.
/// Output: `true` when both inputs contain the same text; otherwise `false`.
/// Transformation: delegates to exact Terlan equality for the two string
/// values without changing either input.
///
/// This function exists as the named implementation hook for future
/// `Comparable[String]` / equality-style trait conformance. Ordinary Terlan
/// code may use the `==` operator directly.
///
/// ```terlan
/// module string_equal_example.
///
/// pub demo(): Bool ->
///     "hello".equal("hello").
/// ```
@compiler.intrinsic {core.string.equal}
pub (left: String) equal(right: String): Bool ->
    left == right.

/// Compares two `String` values using the selected stable source ordering.
///
/// Input: two UTF-8 `String` values.
/// Output: `Lt()` when `left` sorts before `right`, `Eq()` when both strings
/// are equal, and `Gt()` when `left` sorts after `right`.
/// Transformation: maps Terlan's selected string ordering relation onto
/// `std.core.Ordering.Comparison` without changing either input.
///
/// This function is the named implementation hook for compiler-known
/// `std.core.Ordering.Ordering[String]` conformance in 0.0.1. Rich Unicode
/// collation is not part of this hook; targets must preserve the stable source
/// ordering selected by the compiler contract.
///
/// ```terlan
/// module string_compare_example.
///
/// pub demo(): std.core.Ordering.Comparison ->
///     "a".compare("b").
/// ```
@compiler.intrinsic {core.string.compare}
pub (left: String) compare(right: String): std.core.Ordering.Comparison ->
    case left == right {
        true ->
            Eq();

        false ->
            case left < right {
                true ->
                    Lt();

                false ->
                    Gt()
            }
    }.

/// Renders a `String` as itself.
///
/// Input: one UTF-8 `String` value.
/// Output: the same `String` value.
/// Transformation: preserves the input unchanged because `String` is already
/// the canonical textual representation of itself.
///
/// ```terlan
/// module string_to_string_example.
///
/// pub demo(): String ->
///     "hello".to_string().
/// ```
@compiler.intrinsic {core.string.to_string}
pub (value: String) to_string(): String ->
    value.

/// Parses a `String` into a `String`.
///
/// Input: one UTF-8 `String` value.
/// Output: `Some(value)` because every `String` value is already a valid
/// `String` representation.
/// Transformation: wraps the unchanged input in `Option[String]`.
///
/// ```terlan
/// module string_from_string_example.
///
/// pub demo(): Option[String] ->
///     "hello".from_string().
/// ```
@compiler.intrinsic {core.string.from_string}
pub (value: String) from_string(): Option[String] ->
    Some(value).

/// Returns whether a `String` contains no text.
///
/// Input: one UTF-8 `String` value.
/// Output: `true` when the string is empty; otherwise `false`.
/// Transformation: compares the input with the canonical empty string literal.
///
/// ```terlan
/// module string_is_empty_example.
///
/// pub demo(): Bool ->
///     "".is_empty().
/// ```
@compiler.intrinsic {core.string.is_empty}
pub (value: String) is_empty(): Bool ->
    value == "".

/// Appends two `String` values.
///
/// Input: two UTF-8 `String` values.
/// Output: one `String` containing `left` followed by `right`.
/// Transformation: lowers through the compiler-owned `core.string.append`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_append_example.
///
/// pub demo(): String ->
///     "hello".append("world").
/// ```
@compiler.intrinsic {core.string.append}
pub (left: String) append(right: String): String ->
    left.

/// Concatenates a list of `String` values.
///
/// Input: a `List[String]`.
/// Output: one `String` containing each input string in list order.
/// Transformation: lowers through the compiler-owned `core.string.concat`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_concat_example.
///
/// pub demo(): String ->
///     std.core.String.concat(["hello", "world"]).
/// ```
@compiler.intrinsic {core.string.concat}
pub concat(values: List[String]): String ->
    "".

/// Returns whether a `String` contains a pattern.
///
/// Input: a source `String` and a pattern `String`.
/// Output: `true` when the pattern is present; otherwise `false`.
/// Transformation: lowers through the compiler-owned `core.string.contains`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_contains_example.
///
/// pub demo(): Bool ->
///     "hello".contains("ell").
/// ```
@compiler.intrinsic {core.string.contains}
pub (value: String) contains(pattern: String): Bool ->
    false.

/// Returns whether a `String` starts with a prefix.
///
/// Input: a source `String` and a prefix `String`.
/// Output: `true` when the prefix is present at the start; otherwise `false`.
/// Transformation: lowers through the compiler-owned `core.string.starts_with`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_starts_with_example.
///
/// pub demo(): Bool ->
///     "hello".starts_with("he").
/// ```
@compiler.intrinsic {core.string.starts_with}
pub (value: String) starts_with(prefix: String): Bool ->
    false.

/// Returns whether a `String` ends with a suffix.
///
/// Input: a source `String` and a suffix `String`.
/// Output: `true` when the suffix is present at the end; otherwise `false`.
/// Transformation: lowers through the compiler-owned `core.string.ends_with`
/// intrinsic for each target backend. The empty suffix is accepted for every
/// string.
///
/// ```terlan
/// module string_ends_with_example.
///
/// pub demo(): Bool ->
///     "hello".ends_with("lo").
/// ```
@compiler.intrinsic {core.string.ends_with}
pub (value: String) ends_with(suffix: String): Bool ->
    false.

/// Counts user-visible text units in a `String`.
///
/// Input: one UTF-8 `String` value.
/// Output: the selected portable text length for the text.
/// Transformation: lowers through the compiler-owned `core.string.length`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_length_example.
///
/// pub demo(): Int ->
///     "hello".length().
/// ```
@compiler.intrinsic {core.string.length}
pub (value: String) length(): Int ->
    0.

/// Counts encoded bytes in a `String`.
///
/// Input: one UTF-8 `String` value.
/// Output: the number of bytes in the canonical UTF-8 representation.
/// Transformation: lowers through the compiler-owned `core.string.byte_size`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_byte_size_example.
///
/// pub demo(): Int ->
///     "hello".byte_size().
/// ```
@compiler.intrinsic {core.string.byte_size}
pub (value: String) byte_size(): Int ->
    0.

/// Converts a `String` to lowercase.
///
/// Input: one UTF-8 `String` value.
/// Output: the lowercase form of the string.
/// Transformation: lowers through the compiler-owned `core.string.lowercase`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_lowercase_example.
///
/// pub demo(): String ->
///     "HELLO".lowercase().
/// ```
@compiler.intrinsic {core.string.lowercase}
pub (value: String) lowercase(): String ->
    value.

/// Converts a `String` to uppercase.
///
/// Input: one UTF-8 `String` value.
/// Output: the uppercase form of the string.
/// Transformation: lowers through the compiler-owned `core.string.uppercase`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_uppercase_example.
///
/// pub demo(): String ->
///     "hello".uppercase().
/// ```
@compiler.intrinsic {core.string.uppercase}
pub (value: String) uppercase(): String ->
    value.

/// Removes leading and trailing whitespace from a `String`.
///
/// Input: one UTF-8 `String` value.
/// Output: the string with surrounding whitespace removed.
/// Transformation: lowers through the compiler-owned `core.string.trim`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_trim_example.
///
/// pub demo(): String ->
///     " hello ".trim().
/// ```
@compiler.intrinsic {core.string.trim}
pub (value: String) trim(): String ->
    value.

/// Removes leading whitespace from a `String`.
///
/// Input: one UTF-8 `String` value.
/// Output: the string with leading whitespace removed.
/// Transformation: lowers through the compiler-owned `core.string.trim_start`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_trim_start_example.
///
/// pub demo(): String ->
///     " hello ".trim_start().
/// ```
@compiler.intrinsic {core.string.trim_start}
pub (value: String) trim_start(): String ->
    value.

/// Removes trailing whitespace from a `String`.
///
/// Input: one UTF-8 `String` value.
/// Output: the string with trailing whitespace removed.
/// Transformation: lowers through the compiler-owned `core.string.trim_end`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_trim_end_example.
///
/// pub demo(): String ->
///     " hello ".trim_end().
/// ```
@compiler.intrinsic {core.string.trim_end}
pub (value: String) trim_end(): String ->
    value.

/// Replaces every occurrence of a pattern in a `String`.
///
/// Input: a source `String`, a pattern `String`, and a replacement `String`.
/// Output: a `String` where every occurrence of `pattern` has been replaced
/// by `replacement`.
/// Transformation: lowers through the compiler-owned `core.string.replace`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_replace_example.
///
/// pub demo(): String ->
///     "hello".replace("l", "x").
/// ```
@compiler.intrinsic {core.string.replace}
pub (value: String) replace(pattern: String, replacement: String): String ->
    value.

/// Splits a `String` on every occurrence of a separator.
///
/// Input: a source `String` and separator `String`.
/// Output: a `List[String]` of split segments.
/// Transformation: lowers through the compiler-owned `core.string.split`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_split_example.
///
/// pub demo(): List[String] ->
///     "a,b".split(",").
/// ```
@compiler.intrinsic {core.string.split}
pub (value: String) split(on: String): List[String] ->
    [].

/// Splits a `String` on the first occurrence of a separator.
///
/// Input: a source `String` and separator `String`.
/// Output: `Some({left, right})` when the separator is present; otherwise
/// `None()`.
/// Transformation: lowers through the compiler-owned `core.string.split_once`
/// intrinsic for each target backend.
///
/// ```terlan
/// module string_split_once_example.
///
/// pub demo(): Option[{String, String}] ->
///     "a,b".split_once(",").
/// ```
@compiler.intrinsic {core.string.split_once}
pub (value: String) split_once(on: String): Option[{String, String}] ->
    None().
