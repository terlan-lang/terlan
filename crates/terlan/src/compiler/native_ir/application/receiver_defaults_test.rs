//! Provider defaults and statement mutation must not depend on library names.

use crate::compiler::native_ir::source_constructor_test::check_sources;

#[test]
fn receiver_defaults_execute_in_provider_scope_and_mutations_write_back() {
    check_sources(&[
        r#"
module receiver_defaults_caller.
import app.Counter.
pub check(): Bool ->
    let counter = Counter.start();
    counter.advance();
    counter.advance(5);
    counter.advance(7, 2);
    counter.value == 18 and counter.inspect() == 25 and counter.inspect(1) == 19.
"#,
        r#"
module app.Counter.
pub struct Counter { value: Int }.
pub start(): Counter -> Counter { value: 0 }.
pub (mut counter: Counter) advance(amount: Int = 2, extra: Int = 1): Unit ->
    Counter { value: counter.value + amount + extra }.
pub (counter: Counter) inspect(offset: Int = 7): Int -> counter.value + offset.
"#,
    ]);
}

#[test]
fn immutable_receiver_statement_does_not_overwrite_its_value() {
    check_sources(&[r#"
module immutable_receiver_statement.
pub struct Counter { value: Int }.
pub (counter: Counter) advance(amount: Int = 10): Counter ->
    Counter { value: counter.value + amount }.
pub check(): Bool ->
    let counter = Counter { value: 1 };
    counter.advance();
    counter.value == 1 and counter.advance(2).value == 3.
"#]);
}

#[test]
fn bound_command_results_are_unit_and_write_back_without_library_names() {
    check_sources(&[r#"
module bound_receiver_command.
import std.core.Unit.
pub struct Counter { value: Int }.
pub start(): Counter -> Counter { value: 0 }.
pub (mut counter: Counter) advance(amount: Int = 2): Unit ->
    Counter { value: counter.value + amount }.
pub check(): Bool ->
    let counter = start();
    let original = counter;
    let first = counter.advance();
    let second = advance(counter, 5);
    first == Unit and second == Unit and counter.value == 7 and original.value == 0.
"#]);
}

#[test]
fn generic_receiver_defaults_preserve_parameter_types() {
    check_sources(&[r#"
module generic_receiver_defaults.
pub struct Box[T] { value: T }.
pub (box: Box[T]) choose[T](fallback: T, use_value: Bool = true): T ->
    case use_value { true -> box.value; false -> fallback }.
integer_box(): Box[Int] -> Box { value: 42 }.
boolean_box(): Box[Bool] -> Box { value: true }.
pub check(): Bool ->
    let integer = integer_box();
    let boolean = boolean_box();
    let inferred_integer = Box { value: 42 };
    let inferred_boolean = Box { value: true };
    integer.choose(9) == 42 and integer.choose(9, false) == 9
        and boolean.choose(false) and inferred_integer.choose(0) == 42
        and inferred_boolean.choose(false).
"#]);
}

#[test]
fn imported_generic_receiver_results_use_concrete_receiver_arguments() {
    check_sources(&[
        r#"
module generic_receiver_projection_consumer.
import std.core.Option.{Some}.
import app.Pair.{Pair}.
import app.Builder.{Builder}.
import app.Model.{Model}.
import type app.Builder.{Builder}.
import type app.Pair.{Pair}.
import type std.core.Option.{Option}.
build_with[M, First, Second](model: M)[Builder[M, First, Second]]: Option[Pair[First, Second]] ->
    Builder.build(model).
same[T](left: T, right: T): Bool -> left == right.
pub check(): Bool ->
    let Some(pair) <- build_with(Model()) else { _ -> false };
    same(true, pair.second()).
"#,
        r#"
module app.Pair.
pub struct Pair[First, Second] { first: First, second: Second }.
pub constructor Pair[First, Second] {
    (first: First, second: Second): Pair[First, Second] ->
        Pair {first: first, second: second}
}.
pub (pair: Pair[First, Second]) second(): Second -> pair.second.
"#,
        r#"
module app.Builder.
import type app.Pair.{Pair}.
import type std.core.Option.{Option}.
pub trait Builder[Model, First, Second] {
    build(model: Model): Option[Pair[First, Second]].
}.
"#,
        r#"
module app.Model.
import std.core.Option.{Some}.
import app.Pair.{Pair}.
import type app.Builder.{Builder}.
import type app.Pair.{Pair}.
import type std.core.Option.{Option}.
pub struct Model implements Builder[Model, Int, Bool] {}.
pub constructor Model { (): Model -> Model {} }.
pub (model: Model) build(): Option[Pair[Int, Bool]] -> Some(Pair(42, true)).
"#,
        include_str!("../../../../../../std/core/Option.terl"),
    ]);
}
