//! Linked native execution, including plans received through ordinary parameters.

use super::super::source_constructor_test::check_sources;

const EFFECT: &str = include_str!("../../../../../std/core/Effect.terl");

#[test]
fn effect_runner_schedules_nested_named_suspending_callbacks() {
    check_sources(&[
        r#"
module effect_nested_named_callback.
import std.core.Effect.
import std.vm.Process.
increment(value: Int): Int -> let _parked = Process.yield_now(); value + 1.
nested(value: Int): Int -> Effect.run(Effect.map(Effect.succeed(value), increment)) + 1.
pub check(): Bool -> Effect.run(Effect.map(Effect.succeed(40), nested)) == 42.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_executes_suspending_comprehension_guards() {
    check_sources(&[
        r#"
module effect_deferred_comprehension.
import std.core.Effect.
import std.core.GuardResult.
import std.vm.Process.
accept(value: Int): Effect[Bool] ->
    Effect.map(Effect.succeed(value), (item: Int) ->
        let _parked = Process.yield_now(); item > 1).
plan(values: List[Int]): Effect[List[Int]] -> [value | value <- values, accept(value)].
pub check(): Bool -> Effect.run(plan([1, 2, 3])) == [2, 3].
"#,
        EFFECT,
    ]);
}

#[test]
#[should_panic(expected = "error[native_ir.effect_callback]: callback must take one argument")]
fn effect_runner_rejects_direct_descriptors_with_wrong_callback_arity() {
    check_sources(&[
        r#"
module effect_bad_callback_arity.
import std.core.Effect.
import std.core.Effect.{Mapped}.
callback(left: Int, right: Int): Int -> left + right.
plan(): Effect[Int] -> Mapped(Effect.succeed(1), callback).
pub check(): Bool -> Effect.run(plan()) == 1.
"#,
        EFFECT,
    ]);
}

#[test]
#[should_panic(expected = "error[native_ir.effect_callback]: descriptor value is not a callback")]
fn effect_runner_rejects_non_callable_direct_descriptors() {
    check_sources(&[
        r#"
module effect_bad_callback_value.
import std.core.Effect.
import std.core.Effect.{Mapped}.
plan(): Effect[Int] -> Mapped(Effect.succeed(1), 2).
pub check(): Bool -> Effect.run(plan()) == 1.
"#,
        EFFECT,
    ]);
}

#[test]
#[should_panic(expected = "error[native_ir.effect_callback]: continuation must return Effect")]
fn effect_runner_rejects_flat_map_callbacks_without_effect_results() {
    check_sources(&[
        r#"
module effect_bad_callback_result.
import std.core.Effect.
import std.core.Effect.{FlatMap}.
callback(value: Int): Int -> value + 1.
plan(): Effect[Int] -> FlatMap(Effect.succeed(1), callback).
pub check(): Bool -> Effect.run(plan()) == 2.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_retains_direct_named_callbacks_and_captured_lambda_scopes() {
    check_sources(&[
        r#"
module effect_direct_callbacks.
import std.core.Effect.
import std.core.Effect.{Mapped}.
import std.vm.Process.
increment(value: Int): Int -> value + 1.
named(): Effect[Int] -> Mapped(Effect.succeed(40), increment).
captured(offset: Int): Effect[Int] ->
    let callback = ((value: Int) -> let _parked = Process.yield_now(); value + offset);
    let {first, second} = {callback, callback};
    case {first, second} {
        {selected, _} -> Mapped(named(), selected)
    }.
run(plan: Effect[Int]): Int -> Effect.run(plan).
pub check(): Bool -> run(captured(1)) == 42.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_executes_checked_direct_mapped_descriptors() {
    check_sources(&[
        r#"
module effect_direct_mapped.
import std.core.Effect.
import std.core.Effect.{Mapped}.
increment(value: Int): Int -> value + 1.
plan(input: Effect[Int], callback: (Int) -> Int): Effect[Int] -> Mapped(input, callback).
run(input: Effect[Int]): Int -> Effect.run(input).
pub check(): Bool -> run(plan(Effect.succeed(41), increment)) == 42.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_executes_checked_direct_flat_map_descriptors() {
    check_sources(&[
        r#"
module effect_direct_flat_map.
import std.core.Effect.
import std.core.Effect.{FlatMap}.
next(value: Int): Effect[Int] -> Effect.succeed(value + 1).
plan(input: Effect[Int], callback: (Int) -> Effect[Int]): Effect[Int] -> FlatMap(input, callback).
run(input: Effect[Int]): Int -> Effect.run(input).
pub check(): Bool -> run(plan(Effect.succeed(41), next)) == 42.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_uses_specialized_results_of_lexically_bound_producers() {
    check_sources(&[
        r#"
module effect_run_inferred.
import std.core.Effect.
increment(value: Int): Int -> value + 1.
next(value: Int): Effect[Int] -> Effect.succeed(value * 2).
pub check(): Bool ->
    let mapped = Effect.map(Effect.succeed(20), increment);
    let flat = Effect.flat_map(mapped, next);
    Effect.run(flat) == 42 and Effect.run(mapped) == 21.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_distinguishes_callback_input_types_with_the_same_result() {
    check_sources(&[
        r#"
module effect_run_signature_selection.
import std.core.String.
import std.core.Effect.
number(value: Int): Int -> value + 1.
length(value: String): Int -> value.length().
run(plan: Effect[Int]): Int -> Effect.run(plan).
pub check(): Bool ->
    run(Effect.map(Effect.succeed(41), number)) == 42
        and run(Effect.map(Effect.succeed("typed"), length)) == 5.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_handles_deep_runtime_plans_without_native_stack_growth() {
    let modules = check_sources(&[
        r#"
module effect_run_deep.
import std.core.Effect.
increment(value: Int): Int -> value + 1.
build(depth: Int, plan: Effect[Int]): Effect[Int] ->
    case depth { 0 -> plan; _ -> build(depth - 1, Effect.map(plan, increment)) }.
run(plan: Effect[Int]): Int -> Effect.run(plan).
pub check(): Bool -> run(build(512, Effect.succeed(0))) == 512.
"#,
        EFFECT,
    ]);
    assert!(modules
        .iter()
        .any(|module| module.name == "$terlan.recursive_suspension"));
}

#[test]
fn effect_runner_sequences_suspending_callbacks_with_a_different_result_type() {
    check_sources(&[
        r#"
module effect_run_flat_suspension.
import std.core.Int.
import std.core.Effect.
import std.vm.Process.
next(value: Int): Effect[String] ->
    let _parked = Process.yield_now();
    Effect.succeed(Int.to_string(value)).
run(plan: Effect[String]): String -> Effect.run(plan).
pub check(): Bool -> run(Effect.flat_map(Effect.succeed(42), next)) == "42".
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_executes_parameter_carried_completed_plans() {
    check_sources(&[
        r#"
module effect_run_parameter.
import std.core.Effect.
run(plan: Effect[Int]): Int -> Effect.run(plan).
pub check(): Bool -> run(Effect.succeed(42)) == 42.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_executes_mapped_and_sequenced_plans() {
    check_sources(&[
        r#"
module effect_run_composition.
import std.core.Effect.
increment(value: Int): Int -> value + 1.
next(value: Int): Effect[Int] -> Effect.succeed(value * 2).
run(plan: Effect[Int]): Int -> Effect.run(plan).
pub check(): Bool ->
    let mapped = Effect.map(Effect.succeed(20), increment);
    run(Effect.flat_map(mapped, next)) == 42.
"#,
        EFFECT,
    ]);
}

#[test]
fn effect_runner_preserves_mixed_types_captures_and_callback_suspension() {
    check_sources(&[
        r#"
module effect_run_suspension.
import std.core.{Int, String}.
import std.core.Effect.
import std.vm.Process.
plan(prefix: String): Effect[String] ->
    Effect.map(Effect.succeed(42), (value: Int) ->
        let _parked = Process.yield_now();
        prefix.append(Int.to_string(value))).
run(effect: Effect[String]): String -> Effect.run(effect).
pub check(): Bool -> run(plan("answer ")) == "answer 42".
"#,
        EFFECT,
    ]);
}
