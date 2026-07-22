use super::*;
use crate::terlan_syntax::{parse_interface_module, parse_module};

/// Verifies block documentation is emitted with canonical marker spacing.
///
/// Inputs:
/// - A source module containing a TypeDoc-style block where body lines are
///   written as `*Text` instead of `* Text`.
///
/// Output:
/// - Formatted source preserving the doc block with one space after each body
///   marker.
///
/// Transformation:
/// - Parses documentation through the lexer-normalized doc metadata and renders
///   it back as canonical `/** ... */` formatter output.
#[test]
fn formatter_normalizes_doc_block_marker_spacing() {
    let output = format_source_module(
        r#"
module doc_spacing_fmt.

/**
 *Core boolean conformance helpers.
 *@param value input value.
 *@returns canonical bool.
 */
pub value(value: Bool): Bool ->
    value.
"#,
    )
    .expect("source with doc block should format");

    assert!(output.contains(" * Core boolean conformance helpers."));
    assert!(output.contains(" * @param value input value."));
    assert!(output.contains(" * @returns canonical bool."));
    assert!(!output.contains("*Core boolean"));
    assert!(!output.contains("*@param"));
    assert!(!output.contains("*@returns"));
}

/// Verifies wildcard imports use the braced selector form after formatting.
///
/// Inputs:
/// - Canonical braced wildcard import syntax.
///
/// Output:
/// - Canonical import source using `.{*}.`.
///
/// Transformation:
/// - Renders the stable wildcard import selector form so the declaration
///   terminator stays visually clear.
#[test]
fn formatter_preserves_braced_wildcard_imports() {
    let output = format_source_module(
        r#"
module wildcard_import_fmt.

import test.Other.{*}.

pub main(): Int -> 1.
"#,
    )
    .expect("format wildcard import");

    assert!(output.contains("import test.Other.{*}."));
    assert!(!output.contains("import test.Other.*."));
}

/// Verifies declaration annotations are preserved by source formatting.
///
/// Inputs:
/// - A module with a marker `@test` annotation and a metadata annotation.
///
/// Output:
/// - Formatted source containing both annotations before their declarations.
///
/// Transformation:
/// - Exercises the formatter's declaration/annotation pairing so directory
///   formatting cannot silently remove test or target metadata.
#[test]
fn formatter_preserves_declaration_annotations() {
    let output = format_source_module(
        r#"
module annotation_fmt.

@test
pub parses_float(): Bool ->
    1.0 == 1.0.

@target.vm {process_mailbox: true}
pub timeout(): Int ->
    1.
"#,
    )
    .expect("format annotated source");

    assert!(output.contains("@test\npub parses_float(): Bool ->"));
    assert!(output.contains("@target.vm"));
    assert!(output.contains("process_mailbox"));
    assert!(output.contains("true"));
    assert!(output.contains("}\npub timeout(): Int ->"));
}

/// Verifies formatter output organizes imports alphabetically.
///
/// Inputs:
/// - A source module with imports in non-alphabetical order.
///
/// Output:
/// - Formatted source whose import declarations are sorted by canonical import
///   text before ordinary declarations.
///
/// Transformation:
/// - Parses and formats the module, then compares the emitted import line order
///   after wildcard spelling has been canonicalized.
#[test]
fn formatter_sorts_imports_alphabetically() {
    let output = format_source_module(
        r#"
module sorted_import_fmt.

import std.io.Console.{println}.
import app.z.Zed.
import app.alpha.Alpha.
import app.middle.Tools.{*}.

pub main(): Int -> 1.
"#,
    )
    .expect("format sorted imports");

    let import_lines = output
        .lines()
        .filter(|line| line.starts_with("import "))
        .collect::<Vec<_>>();
    let mut sorted_import_lines = import_lines.clone();
    sorted_import_lines.sort();
    assert_eq!(import_lines, sorted_import_lines);
    assert_eq!(import_lines.first(), Some(&"import app.alpha.Alpha."));
    assert_eq!(
        import_lines.last(),
        Some(&"import std.io.Console.{println}.")
    );
}

/// Verifies formatter groups regular imports before type imports.
///
/// Inputs:
/// - A source module with regular and type imports in mixed order.
///
/// Output:
/// - Formatted source with alphabetized regular imports, a blank line, then
///   alphabetized type imports.
///
/// Transformation:
/// - Parses and formats the module, then checks both import group ordering and
///   the visual separation between value/module imports and type imports.
#[test]
fn formatter_sorts_type_imports_after_regular_imports() {
    let output = format_source_module(
        r#"
module sorted_type_import_fmt.

import type app.zeta.Zeta.
import app.beta.Beta.
import type app.alpha.Alpha.
import app.alpha.Alpha.

pub main(): Int -> 1.
"#,
    )
    .expect("format sorted type imports");

    let import_lines = output
        .lines()
        .filter(|line| line.starts_with("import"))
        .collect::<Vec<_>>();
    assert_eq!(
        import_lines,
        vec![
            "import app.alpha.Alpha.",
            "import app.beta.Beta.",
            "import type app.alpha.Alpha.",
            "import type app.zeta.Zeta.",
        ]
    );
    assert!(output.contains("import app.alpha.Alpha.\nimport app.beta.Beta."));
    assert!(output.contains("import app.beta.Beta.\n\nimport type app.alpha.Alpha."));
}

/// Verifies formatter keeps import groups dense and alphabetized.
///
/// Inputs:
/// - A source module with selected regular imports and type imports in mixed
///   order.
///
/// Output:
/// - Formatted source with regular imports adjacent, one blank line, then type
///   imports adjacent.
///
/// Transformation:
/// - Parses and formats the module, asserting canonical import spelling and
///   group spacing so fmt enforces std import layout instead of relying on
///   manual cleanup.
#[test]
fn formatter_groups_regular_and_type_imports_without_extra_blank_lines() {
    let output = format_source_module(
        r#"
module grouped_import_fmt.

import type std.collections.Map.
import std.collections.Set.
import std.collections.Enumerable.{Enumerable}.
import type std.collections.List.
import std.collections.List.

pub main(): Int -> 1.
"#,
    )
    .expect("format grouped imports");

    assert!(output.contains(
        "import std.collections.{Enumerable, List, Set}.\n\nimport type std.collections.{List, Map}."
    ));
    assert!(!output.contains("std.collections. List"));
    assert!(!output.contains("std.collections. Set"));
    assert!(!output.contains("import std.collections.List."));
    assert!(!output.contains("import std.collections.Set."));
    assert!(!output.contains("import type std.collections.List."));
    assert!(!output.contains("import type std.collections.Map."));
    assert!(!output.contains("List}\n\nimport std.collections"));
    assert!(!output.contains("List}.\nimport type"));
}

/// Verifies case formatting keeps simple branch bodies compact and expands
/// nested case bodies.
///
/// Inputs:
/// - A function whose outer `case` contains one nested `case`.
///
/// Output:
/// - Formatted source with outer clauses indented one level, short branch
///   bodies on one line, and the nested `case` body expanded below `->`.
///
/// Transformation:
/// - Locks formatter behavior for pattern-heavy std functions so source
///   readability is owned by `terlc fmt`, not manual cleanup.
#[test]
fn formatter_formats_nested_case_with_compact_short_branches() {
    let output = format_source_module(
        r#"
module nested_case_fmt.

pub filter_iterator(iterator: Iterator[T], predicate: (T) -> Bool): List[T] ->
    case Iterator.next(iterator) {
    Some({value: value, next: next}) -> case predicate(value) {
        true -> [value | filter_iterator(next, predicate)];
        false -> filter_iterator(next, predicate)
        };
    None -> []
    }.
"#,
    )
    .expect("format nested case");

    assert!(output.contains(
        "case Iterator.next(iterator) {\n        Some({value: value, next: next}) ->\n            case predicate(value) {\n                true -> [value | filter_iterator(next, predicate)];\n                false -> filter_iterator(next, predicate)\n            };\n        None -> []\n    }."
    ));
    assert!(!output.contains("-> case predicate"));
    assert!(!output.contains("\n    Some({value: value, next: next}) -> case"));
}

/// Verifies formatter collapses imports sharing a module prefix.
///
/// Inputs:
/// - A std.collections-shaped import block with regular and type imports that
///   share the same module prefix.
///
/// Output:
/// - One selected regular import and one selected type import, separated by a
///   single blank line.
///
/// Transformation:
/// - Merges compatible module imports after promotion and before final import
///   ordering so std source and generated tests cannot drift into repeated
///   single-symbol import lines.
#[test]
fn formatter_collapses_regular_and_type_imports_by_module_prefix() {
    let output = format_source_module(
        r#"
module collapsed_import_fmt.

import std.collections.Iterator.
import std.collections.List.
import std.collections.Map.
import std.collections.Set.
import std.core.Option.{None, Some}.
import std.core.Unit.{Unit}.
import type std.collections.List.
import type std.collections.Map.
import type std.collections.Set.

pub main(): Int -> 1.
"#,
    )
    .expect("format collapsed imports");

    assert!(output.contains(
        "import std.collections.{Iterator, List, Map, Set}.\nimport std.core.Option.{None, Some}.\nimport std.core.Unit.\n\nimport type std.collections.{List, Map, Set}."
    ));
    assert!(!output.contains("import std.collections.Iterator."));
    assert!(!output.contains("import std.collections.List."));
    assert!(!output.contains("import type std.collections.List."));
}

/// Verifies default selected imports from sibling modules collapse together.
///
/// Inputs:
/// - Imports such as `std.collections.List.{List}` and
///   `std.collections.Map.{Map}`.
///
/// Output:
/// - One parent-module import selecting all default symbols.
///
/// Transformation:
/// - Normalizes default selected imports to their parent module before import
///   collapse so generated and hand-written std tests do not repeat sibling
///   module paths.
#[test]
fn formatter_collapses_default_selected_sibling_imports() {
    let output = format_source_module(
        r#"
module sibling_default_import_fmt.

import std.collections.Enumerable.{Enumerable}.
import std.collections.List.{List}.
import std.collections.Map.{Map}.
import std.collections.Set.{Set}.

pub main(): Int -> 1.
"#,
    )
    .expect("format default selected sibling imports");

    assert!(output.contains("import std.collections.{Enumerable, List, Map, Set}."));
    assert!(!output.contains("import std.collections.Enumerable.{Enumerable}."));
    assert!(!output.contains("import std.collections.List.{List}."));
    assert!(!output.contains("import std.collections.Map.{Map}."));
    assert!(!output.contains("import std.collections.Set.{Set}."));
}

/// Verifies multi-field structs keep explicit field separators.
///
/// Inputs:
/// - A source module with a struct containing adjacent fields.
///
/// Output:
/// - Formatted source with commas between fields.
///
/// Transformation:
/// - Keeps formatter output parse-stable so subsequent fmt runs cannot merge
///   `code: Atom` and `message: String` into one field annotation.
#[test]
fn formatter_separates_multi_field_struct_fields() {
    let output = format_source_module(
        r#"
module struct_field_separator_fmt.

pub struct Error {
    code: Atom,
    message: String
}.
"#,
    )
    .expect("format multi-field struct");

    assert!(output.contains("pub struct Error {\n    code: Atom,\n    message: String\n}."));
    assert!(!output.contains("code: Atom message"));
}

/// Verifies repeated direct type mappings are promoted to type imports.
///
/// Inputs:
/// - A generated-test-shaped module using `std.js.Number.JsNumber` in multiple
///   type positions.
///
/// Output:
/// - Formatted source imports `JsNumber` once and rewrites the repeated direct
///   type references to the local type name.
///
/// Transformation:
/// - Exercises formatter normalization for generated std.js tests so direct
///   mapped types do not remain noisy after two or more uses.
#[test]
fn formatter_promotes_repeated_direct_type_mappings_to_type_imports() {
    let output = format_source_module(
        r#"
module direct_type_import_fmt.

import std.js.ArrayBuffer.{ArrayBuffer}.

pub byte_length_typechecks(receiver: ArrayBuffer): std.js.Number.JsNumber ->
    receiver.byte_length().

pub slice_typechecks(receiver: ArrayBuffer, begin: std.js.Number.JsNumber, end: std.js.Number.JsNumber): ArrayBuffer ->
    receiver.slice(begin, end).
"#,
    )
    .expect("format repeated direct type mappings");

    assert!(output.contains("import std.js.ArrayBuffer.\n\nimport type std.js.Number.{JsNumber}."));
    assert!(output.contains("pub byte_length_typechecks(receiver: ArrayBuffer): JsNumber ->"));
    assert!(output.contains(
        "pub slice_typechecks(receiver: ArrayBuffer, begin: JsNumber, end: JsNumber): ArrayBuffer ->"
    ));
    assert!(!output.contains("std.js.Number.JsNumber"));
}

/// Verifies repeated fully-qualified value calls are promoted to imports.
///
/// Inputs:
/// - A std-test-shaped module that repeats `std.test.Test.assert_equal`.
///
/// Output:
/// - Formatted source imports `assert_equal` once and rewrites repeated direct
///   calls to the selected local function.
///
/// Transformation:
/// - Exercises formatter normalization for repeated std test assertions so
///   test files do not keep noisy fully-qualified assertion calls.
#[test]
fn formatter_promotes_repeated_direct_value_calls_to_imports() {
    let output = format_source_module(
        r#"
module direct_value_import_fmt.

@test
pub one(): Bool ->
    std.test.Test.assert_equal(1, 1).

@test
pub two(): Bool ->
    std.test.Test.assert_equal(2, 2).
"#,
    )
    .expect("format repeated direct value calls");

    assert!(output.contains("import std.test.Test.{assert_equal}."));
    assert!(output.contains("pub one(): Bool ->\n    assert_equal(1, 1)."));
    assert!(output.contains("pub two(): Bool ->\n    assert_equal(2, 2)."));
    assert!(!output.contains("std.test.Test.assert_equal"));
}

/// Verifies direct calls join an existing selected import from the same module.
///
/// Inputs:
/// - A std-test-shaped module importing `assert_equal` and using
///   `std.test.Test.assert` once.
///
/// Output:
/// - Formatted source imports both functions in one sorted selected import and
///   rewrites the direct call to `assert(...)`.
///
/// Transformation:
/// - Promotes direct calls from a module that already has a selected value
///   import, even when the direct call appears only once.
#[test]
fn formatter_promotes_direct_value_call_next_to_existing_selected_import() {
    let output = format_source_module(
        r#"
module selected_value_import_fmt.

import std.test.Test.{assert_equal}.

@test
pub one(): Bool ->
    std.test.Test.assert(1 == 1).

@test
pub two(): Bool ->
    assert_equal(2, 2).
"#,
    )
    .expect("format direct call next to selected import");

    assert!(output.contains("import std.test.Test.{assert, assert_equal}."));
    assert!(output.contains("pub one(): Bool ->\n    assert(1 == 1)."));
    assert!(output.contains("pub two(): Bool ->\n    assert_equal(2, 2)."));
    assert!(!output.contains("std.test.Test.assert("));
}

/// Verifies nested first-argument module calls are preserved by the formatter.
///
/// Inputs:
/// - A nested traversal call where the inner call result is the first argument
///   to the outer call.
///
/// Output:
/// - Canonical formatter output that keeps the nested call shape.
///
/// Transformation:
/// - Keeps semantic pipe canonicalization out of `terlc fmt`; lint owns any
///   later safe rewrite into pipe form.
#[test]
fn formatter_preserves_nested_module_calls_without_pipe_promotion() {
    let output = format_source_module(
        r#"
module nested_pipe_fmt.

pub main(collection: Set[Int], cb: (Int) -> Unit): Unit ->
    Iterator.each(Set.iterator(collection), cb).
"#,
    )
    .expect("format nested module call pipe");

    assert!(output.contains(
        "pub main(collection: Set[Int], cb: (Int) -> Unit): Unit ->\n    Iterator.each(Set.iterator(collection), cb)."
    ));
    assert!(!output.contains("|> Set.iterator"));
}

/// Verifies explicit pipe chains are canonicalized to one stage per line.
///
/// Inputs:
/// - Source that already uses `|>` inline across several stages.
///
/// Output:
/// - Canonical formatter output with the original pipe chain split across
///   continuation lines.
///
/// Transformation:
/// - Exercises explicit pipe formatting separately from nested-call promotion
///   so handwritten pipelines cannot remain as long single-line expressions.
#[test]
fn formatter_wraps_explicit_pipe_chains_to_one_stage_per_line() {
    let output = format_source_module(
        r#"
module explicit_pipe_fmt.

pub main(collection: Set[Int], cb: (Int) -> Int): Set[Int] ->
    collection |> Set.iterator() |> map_iterator(cb) |> Set.from_list().
"#,
    )
    .expect("format explicit pipe chain");

    assert!(output.contains(
        "pub main(collection: Set[Int], cb: (Int) -> Int): Set[Int] ->\n    collection\n    |> Set.iterator()\n    |> map_iterator(cb)\n    |> Set.from_list()."
    ));
}

/// Verifies receiver calls inside nested calls are preserved.
///
/// Inputs:
/// - A nested call whose first argument is a receiver call.
///
/// Output:
/// - Formatter output that keeps the nested receiver call shape.
///
/// Transformation:
/// - Keeps receiver-call pipe canonicalization out of `terlc fmt`; lint owns
///   any later safe rewrite.
#[test]
fn formatter_preserves_nested_receiver_calls_without_pipe_promotion() {
    let output = format_source_module(
        r#"
module receiver_pipe_fmt.

pub main(list: List[Int], cb: (Int) -> Unit): Unit ->
    Iterator.each(list.iterator(), cb).
"#,
    )
    .expect("format nested receiver call pipe");

    assert!(output.contains(
        "pub main(list: List[Int], cb: (Int) -> Unit): Unit ->\n    Iterator.each(list.iterator(), cb)."
    ));
    assert!(!output.contains("|> iterator"));
}

/// Verifies selected imports do not trigger pipe promotion.
///
/// Inputs:
/// - A selected value import used by the inner call of a nested traversal.
///
/// Output:
/// - Import remains canonical and the nested call shape is preserved.
///
/// Transformation:
/// - Ensures import normalization does not smuggle semantic pipe rewrites back
///   into the formatter pass.
#[test]
fn formatter_preserves_nested_selected_import_calls_without_pipe_promotion() {
    let output = format_source_module(
        r#"
module selected_import_pipe_fmt.

import std.collections.Set.{iterator}.

pub main(collection: Set[Int], cb: (Int) -> Unit): Unit ->
    Iterator.each(iterator(collection), cb).
"#,
    )
    .expect("format selected import pipe");

    assert!(output.contains("import std.collections.Set.{iterator}."));
    assert!(output.contains(
        "pub main(collection: Set[Int], cb: (Int) -> Unit): Unit ->\n    Iterator.each(iterator(collection), cb)."
    ));
    assert!(!output.contains("|> iterator"));
}

/// Verifies named arguments are not promoted into pipe stages.
///
/// Inputs:
/// - A nested call whose outer call has named arguments.
///
/// Output:
/// - Formatter output preserves the nested call shape.
///
/// Transformation:
/// - Guards against rewrites that could obscure named-argument meaning.
#[test]
fn formatter_does_not_promote_named_argument_calls_to_pipe_chain() {
    let output = format_source_module(
        r#"
module named_arg_pipe_fmt.

pub main(response: Response): Response ->
    Response.cookie(response, name = "session", value = "abc").
"#,
    )
    .expect("format named argument candidate");

    assert!(output.contains(
        "pub main(response: Response): Response ->\n    Response.cookie(response, name = \"session\", value = \"abc\")."
    ));
    assert!(!output.contains("|> Response.cookie"));
}

/// Verifies function-value calls are not promoted into pipe stages.
///
/// Inputs:
/// - A function-value call whose argument is a nested module call.
///
/// Output:
/// - Formatter output preserves the function-value call shape.
///
/// Transformation:
/// - Keeps `callee(arg)` distinct from normal module/local calls because its
///   invocation semantics are intentionally explicit.
#[test]
fn formatter_does_not_promote_function_value_calls_to_pipe_chain() {
    let output = format_source_module(
        r#"
module function_value_pipe_fmt.

pub main(collection: Set[Int], runner: (Iterator[Int]) -> Unit): Unit ->
    runner(Set.iterator(collection)).
"#,
    )
    .expect("format function-value candidate");

    assert!(output.contains(
        "pub main(collection: Set[Int], runner: (Iterator[Int]) -> Unit): Unit ->\n    runner(Set.iterator(collection))."
    ));
    assert!(!output.contains("|> runner"));
}

/// Verifies promotion does not leak into ordinary call argument lists.
///
/// Inputs:
/// - A nested call chain used as the second argument to another call.
///
/// Output:
/// - Formatter output keeps the argument inline instead of emitting a multiline
///   pipe expression inside the argument list.
///
/// Transformation:
/// - Restricts pipe promotion to statement/body contexts where multiline pipe
///   layout is syntactically clear.
#[test]
fn formatter_does_not_promote_nested_call_arguments_to_pipe_chain() {
    let output = format_source_module(
        r#"
module nested_argument_pipe_fmt.

pub main(collection: Set[Int], label: String): Unit ->
    Debug.log(label, Iterator.each(Set.iterator(collection), cb)).
"#,
    )
    .expect("format nested argument candidate");

    assert!(output.contains(
        "pub main(collection: Set[Int], label: String): Unit ->\n    Debug.log(label, Iterator.each(Set.iterator(collection), cb))."
    ));
}

/// Verifies formatter preserves callable generics and type delimiter spacing.
///
/// Inputs:
/// - An Enumerable-shaped trait and implementation with method-level generic
///   parameters and nested generic type expressions.
///
/// Output:
/// - Formatted source that keeps `[U]` on callable names and does not introduce
///   spaces before commas or closing delimiters in types.
///
/// Transformation:
/// - Parses and formats generic trait/method declarations so directory fmt
///   cannot corrupt std collection signatures.
#[test]
fn formatter_preserves_generic_callable_signatures() {
    let output = format_source_module(
        r#"
module generic_callable_fmt.

import std.collections.List.
import std.collections.Map.
import std.core.Unit.{Unit}.

pub type Step[T] =
    {value: T, next: List[T]}.

pub trait Enumerable[C[_]] {
    update[T](mut collection: C[T], value: T): Unit.
    map[T, U](collection: C[T], cb: (T) -> U): C[U].
    fold[T, U](collection: C[T], initial: U, reducer: (U, T) -> U): U.
}.

pub impl Enumerable[List] for List {
    map[U](collection: List[T], cb: (T) -> U): List[U] ->
        List.new().
    fold[U](collection: List[T], initial: U, reducer: (U, T) -> U): U ->
        initial.
}.
"#,
    )
    .expect("format generic callable signatures");

    assert!(output.contains("pub type Step[T] = {value: T, next: List[T]}."));
    assert!(output.contains("pub trait Enumerable[C[_]]"));
    assert!(output.contains("update[T](mut collection: C[T], value: T): Unit."));
    assert!(output.contains("map[T, U](collection: C[T], cb: (T) -> U): C[U]."));
    assert!(output.contains("fold[T, U](collection: C[T], initial: U, reducer: (U, T) -> U): U."));
    assert!(output.contains("pub impl Enumerable[List] for List"));
    assert!(output.contains("map[U](collection: List[T], cb: (T) -> U): List[U] ->"));
    assert!(output.contains("fold[U](collection: List[T], initial: U, reducer: (U, T) -> U): U ->"));
    assert!(!output.contains("T )"));
    assert!(!output.contains("K ,"));
    assert!(!output.contains("V }"));
    assert!(!output.contains("value :"));
}

/// Verifies canonical formatting preserves callable trait constraints.
///
/// Inputs:
/// - A free function, receiver method, trait method, and implementation method
///   with post-parameter constraint lists.
///
/// Output:
/// - Canonical source retaining every constraint list in its original callable
///   position.
///
/// Transformation:
/// - Round-trips all callable declaration forms so formatting cannot erase
///   typechecking evidence from source or generated standard interfaces.
#[test]
fn formatter_preserves_callable_constraint_lists() {
    let source = r#"
module callable_bounds_fmt.

pub struct User {
    name: String
}.

pub trait Show[T] {
    show[A](value: A)[Eq[A]]: String.
}.

pub debug[A](value: A)[Show[A], Eq[A]]: String ->
    Show.show(value).

pub (user: User) label[A](value: A)[Show[A]]: String ->
    Show.show(value).

pub impl Show[User] for User {
    show[A](value: A)[Eq[A]]: String ->
        "user".
}.
"#;

    let output = format_source_module(source).expect("format callable constraint lists");

    assert!(output.contains("show[A](value: A)[Eq[A]]: String."));
    assert!(output.contains("pub debug[A](value: A)[Show[A], Eq[A]]: String ->"));
    assert!(output.contains("pub (user: User) label[A](value: A)[Show[A]]: String ->"));
    assert!(output.contains("show[A](value: A)[Eq[A]]: String ->"));
    let reparsed = parse_module(&output).expect("reparse formatted callable constraint lists");
    assert_eq!(format_module(&reparsed), output);
}

/// Verifies structural implications retain their canonical generic-parameter
/// spelling through formatting.
#[test]
fn formatter_preserves_structural_generic_implication() {
    let output = format_source_module(
        r#"
module implication_fmt.

pub display_name[T=>{name:String,profile:{title:String}}](value: T): String ->
    value.name.
"#,
    )
    .expect("format structural generic implication");

    assert!(output.contains(
        "pub display_name[T => {name: String, profile: {title: String}}](value: T): String ->"
    ));
}

/// Verifies implication-constrained generic structs use the canonical spacing
/// shared by every generic parameter list.
#[test]
fn formatter_preserves_structural_generic_struct_implication() {
    let output = format_source_module(
        r#"
module struct_implication_fmt.

pub struct Page[T=>{title:String}] {
    model: T
}.
"#,
    )
    .expect("format generic struct implication");

    assert!(output.contains("pub struct Page[T => {title: String}] {"));
    assert!(output.contains("    model: T"));
}
