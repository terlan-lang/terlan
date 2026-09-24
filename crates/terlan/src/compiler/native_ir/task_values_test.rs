//! Completed Task values execute across functions and managed-value boundaries.

use super::*;

fn check(source: &str) {
    super::super::source_constructor_test::check_sources(&[
        source,
        include_str!("../../../../../std/core/Task.terl"),
        include_str!("../../../../../std/core/Result.terl"),
        include_str!("../../../../../std/core/Error.terl"),
    ]);
}

#[test]
fn completed_tasks_retain_scalar_and_managed_payloads() {
    check(
        r#"
module completed_tasks.
import std.core.Task.
import std.core.Result.{Ok}.
pub check(): Bool ->
    Task.done(42).result() == Ok(42)
        and Task.done("completed").result() == Ok("completed")
        and Task.done([1, 2]).result() == Ok([1, 2]).
"#,
    );
}

#[test]
fn completed_task_parameters_and_aliases_survive_suspension_and_repeated_reads() {
    check(
        r#"
module completed_task_parameters.
import std.core.Task.
import std.core.Result.{Ok}.
import std.vm.Process.
observe(value: Task[List[Int]]): Bool ->
    let alias = value;
    let _parked = Process.yield_now();
    alias.result() == Ok([1, 2]) and value.result() == Ok([1, 2]).
pub check(): Bool -> observe(Task.done([1, 2])).
"#,
    );
}

#[test]
fn completed_task_producers_preserve_empty_and_nested_payloads() {
    check(
        r#"
module completed_task_producers.
import std.core.Task.
import std.core.Result.{Ok}.
import std.vm.Process.
produce(): Task[List[Int]] ->
    let _parked = Process.yield_now();
    Task.done([]).
pub check(): Bool ->
    let nested = Task.done(Task.done(42));
    case nested.result() {
        Ok(inner) -> inner.result() == Ok(42) and produce().result() == Ok([]);
        _ -> false
    }.
"#,
    );
}

#[test]
fn completed_task_results_cross_declared_and_assertion_boundaries() {
    check(
        r#"
module completed_task_result_boundaries.
import std.core.Task.
import std.core.Result.{Ok}.
import std.test.Test.{assert_equal}.
import type std.core.{Error, Result, Task}.
observe(task: Task[Int]): Result[Int, Error] -> task.result().
pub check(): Bool -> assert_equal(Ok(7), observe(Task.done(7))).
"#,
    );
}

#[test]
fn completed_task_results_keep_union_context_in_generic_calls() {
    check(
        r#"
module completed_task_generic_results.
import std.core.Task.
import std.core.Result.{Ok}.
import type std.core.{Error, Result, Task}.
observe(task: Task[Int]): Result[Int, Error] -> task.result().
equal[T](left: T, right: T): Bool -> left == right.
pub check(): Bool ->
    equal(Ok(7), observe(Task.done(7))) and equal(observe(Task.done(7)), Ok(7)).
"#,
    );
}

#[test]
fn completed_task_result_keeps_the_standard_error_identity() {
    check(
        r#"
module completed_task_error_identity.
import std.core.Task.
import std.core.Result.{Ok}.
pub type Error = Int.
pub check(): Bool -> Task.done(42).result() == Ok(42).
"#,
    );
}

#[test]
fn normalized_case_conditions_preserve_short_circuiting_without_tasks() {
    check(
        r#"
module normalized_case_conditions.
value(pair: {Int, Int}): Int -> case pair { {left, right} -> left + right }.
verify(pair: {Int, Int}): Bool ->
    (case pair { {left, right} -> left + right }) == 42
        and (case pair { {left, right} -> left - right }) == 0
        and (true or value({1 div 0, 0}) == 0).
pub check(): Bool -> verify({21, 21}).
"#,
    );
}

#[test]
fn private_task_storage_does_not_capture_user_task_names() {
    assert!(declaration_storage("app.Task.Task", &["T".into()]).is_none());
    assert!(element(&CoreType::Apply {
        constructor: "app.Task.Task".into(),
        args: vec![CoreType::Int],
    })
    .is_none());
    assert_eq!(element(&storage(CoreType::Int)), Some(&CoreType::Int));
}
