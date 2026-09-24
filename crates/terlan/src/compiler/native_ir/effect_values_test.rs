//! Deferred plans must retain their source variants without evaluating callbacks.

use super::*;

#[test]
fn existential_storage_is_scoped_to_standard_effect_aliases() {
    let body = CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::AtomLiteral("failed".to_string())),
        CoreTupleTypeElem::Field {
            name: "error".to_string(),
            ty: CoreType::Dynamic,
        },
    ]);
    let changed = storage_type("std.core.Effect.Failed", &body);
    assert_ne!(changed, body);
    assert_eq!(storage_type("std.core.Effect.Failed", &changed), changed);
    for name in ["app.Effect.Failed", "Failed", "std.core.Effect.Other"] {
        assert_eq!(storage_type(name, &body), body);
    }
    assert_eq!(
        storage_type("std.core.Effect.Failed", &CoreType::Dynamic),
        CoreType::Dynamic
    );
}

#[test]
fn completed_effect_descriptors_cross_native_function_boundaries() {
    super::super::source_constructor_test::check_sources(&[
        r#"
module effect_completed_descriptor.
import std.core.Effect.
import std.core.Effect.{Pure}.
make(value: Int): Effect[Int] -> Effect.succeed(value).
inspect(effect: Effect[Int]): Bool -> case effect { Pure(value) -> value == 7; _ -> false }.
pub check(): Bool -> inspect(make(7)).
"#,
        include_str!("../../../../../std/core/Effect.terl"),
    ]);
}

#[test]
fn mapped_effect_descriptors_do_not_invoke_their_callback() {
    super::super::source_constructor_test::check_sources(&[
        r#"
module effect_mapped_descriptor.
import std.core.Effect.
import std.core.Effect.{Mapped}.
explode(value: Int): Int -> value div 0.
plan(value: Int, mapper: (Int) -> Int): Effect[Int] -> Effect.map(Effect.succeed(value), mapper).
inspect(effect: Effect[Int]): Bool -> case effect { Mapped(_, _) -> true; _ -> false }.
pub check(): Bool -> inspect(plan(7, explode)).
"#,
        include_str!("../../../../../std/core/Effect.terl"),
    ]);
}

#[test]
fn mapped_effect_preserves_captured_callbacks_with_different_output_types() {
    super::super::source_constructor_test::check_sources(&[
        r#"
module effect_captured_descriptor.
import std.core.{Int, String}.
import std.core.Effect.
import std.core.Effect.{Mapped}.
import std.vm.Process.
plan(value: Int, label: String): Effect[String] ->
    Effect.map(Effect.succeed(value), (item: Int) -> label.append(Int.to_string(item))).
inspect(effect: Effect[String]): Bool ->
    let _pause = Process.yield_now();
    case effect { Mapped(_, _) -> true; _ -> false }.
pub check(): Bool -> inspect(plan(7, "retained ")).
"#,
        include_str!("../../../../../std/core/Effect.terl"),
    ]);
}

#[test]
fn flat_map_descriptors_defer_effect_producing_callbacks() {
    super::super::source_constructor_test::check_sources(&[
        r#"
module effect_flat_map_descriptor.
import std.core.Int.
import std.core.Effect.
import std.core.Effect.{FlatMap}.
next(value: Int): Effect[String] -> Effect.succeed(Int.to_string(value div 0)).
plan(value: Int): Effect[String] -> Effect.flat_map(Effect.succeed(value), next).
inspect(effect: Effect[String]): Bool -> case effect { FlatMap(_, _) -> true; _ -> false }.
pub check(): Bool -> inspect(plan(7)).
"#,
        include_str!("../../../../../std/core/Effect.terl"),
    ]);
}

#[test]
fn failure_and_cancellation_remain_inert_typed_descriptions() {
    super::super::source_constructor_test::check_sources(&[
        r#"
module effect_failed_descriptor.
import std.core.Effect.
import std.core.Effect.{Failed, Cancelled}.
failed(): Effect[Int] -> Effect.fail("retained failure").
cancelled(): Effect[Int] -> Effect.cancelled().
inspect_failure(effect: Effect[Int]): Bool -> case effect { Failed(_) -> true; _ -> false }.
inspect_cancellation(effect: Effect[Int]): Bool -> case effect { Cancelled(_) -> true; _ -> false }.
pub check(): Bool -> inspect_failure(failed()) and inspect_cancellation(cancelled()).
"#,
        include_str!("../../../../../std/core/Effect.terl"),
    ]);
}
