//! JSON source helpers must not be replaced by operation-name dispatch.

use super::source_constructor_test::check_sources;

#[test]
fn json_string_fields_follows_source_in_std_and_application_namespaces() {
    let provider = r#"module std.data.Json.
import type std.core.{Option, Result}.
import std.core.Option.{Some}.
import std.core.Result.{Ok}.
pub struct Json { #text: String }.
pub string(text: String): Json -> Json { #text: text }.
pub (json: Json) string_fields(fields: List[String]): Result[List[Option[String]], String] ->
    Ok([Some(json.#text + field) | field <- fields]).
"#;
    let caller = r#"module json_source_authority.
import std.data.Json.
import std.core.Result.{Ok}.
import std.core.Option.{Some}.
pub check(): Bool ->
    case Json.string("source:").string_fields(["a", "a", "b"]) {
        Ok([Some("source:a"), Some("source:a"), Some("source:b")]) -> true;
        _ -> false
    }.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.data.Json", "app.Json"),
        &provider.replace("std.data.Json", "app.Json"),
    ]);
    assert_eq!(
        crate::terlan_native_boundary::dispatch::operation_arity("std.data.json.string_fields"),
        None
    );
}
