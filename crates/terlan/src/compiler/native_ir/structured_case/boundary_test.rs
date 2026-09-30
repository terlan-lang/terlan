//! Case boundaries retain their checked union carrier without library-specific rewrites.

use super::super::source_constructor_test::check_sources;

#[test]
fn case_returns_box_nullary_variants_and_preserve_payloads_and_guards() {
    check_sources(&[
        r#"module case_boundary.
import std.core.Option.{Some, None}.
import type std.core.Option.
type Skipped.
type Chosen = {Atom["chosen"], value: Int}.
type Decision = Skipped | Chosen.
decide(value: Option[Int]): Decision ->
    case value {
        Some(number) where number > 0 -> Chosen(number);
        Some(_) -> Skipped;
        None -> Skipped
    }.
nested(value: Option[Int]): Decision ->
    let offset = 1;
    case value {
        Some(number) -> case number { 0 -> Skipped; _ -> Chosen(number + offset) };
        None -> Skipped
    }.
read(value: Decision): Int -> case value { Skipped -> -1; Chosen(number) -> number }.
pub check(): Bool -> read(decide(None)) == -1 and read(decide(Some(0))) == -1
    and read(decide(Some(42))) == 42 and read(nested(None)) == -1
    and read(nested(Some(0))) == -1 and read(nested(Some(41))) == 42.
"#,
        include_str!("../../../../../../std/core/Option.terl"),
    ]);
}

#[test]
fn escaping_case_callback_preserves_managed_result_contract() {
    check_sources(&[
        r#"module case_callback_boundary.
import std.core.Option.{Some, None}.
import type std.core.Option.
type Skipped.
type Chosen = {Atom["chosen"], value: Int}.
type Decision = Skipped | Chosen.
factory(): (Option[Int]) -> Decision ->
    (value) -> case value { Some(number) -> Chosen(number); None -> Skipped }.
read(value: Decision): Int -> case value { Skipped -> -1; Chosen(number) -> number }.
pub check(): Bool -> let callback = factory();
    read(callback(None)) == -1 and read(callback(Some(42))) == 42.
"#,
        include_str!("../../../../../../std/core/Option.terl"),
    ]);
}
