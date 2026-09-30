//! HTTP imports do not replace source option policy or restrict matching.

use super::*;

#[test]
fn retired_option_helpers_have_no_type_or_lowering() {
    for name in [
        "option_is_none",
        "option_some",
        "session_get_is_none",
        "session_get_some",
    ] {
        for arity in 0..4 {
            let expression = CoreExpr::RemoteCall {
                module: "$terlan.managed.http".into(),
                function: name.into(),
                type_args: vec![],
                args: vec![CoreExpr::Int(0); arity],
            };
            assert_eq!(managed_http_operation_type(&expression), None);
            assert!(lower_managed_http_operation(&expression, |_| panic!(
                "retired helper must not evaluate inputs"
            ))
            .unwrap()
            .is_none());
        }
    }
}

#[test]
fn map_lookup_patterns_support_literals_guards_and_source_option_defaults() {
    let caller = r#"
module http_option_patterns.
import std.http.Request.
import std.collections.Map.
import std.core.Option.
import std.core.Option.{Some, None, with_default as fallback}.
pub check(): Bool ->
    let values = Map({"literal", "match"}, {"empty", ""}, {"guard", "guarded"});
    let literal = case values.get("literal") {
        Some("wrong") -> false;
        Some("match") -> true;
        _ -> false
    };
    let guarded = case values.get("guard") {
        Some(value) where value == "wrong" -> false;
        Some(value) where value == "guarded" -> true;
        _ -> false
    };
    let empty = case values.get("empty") { Some("") -> true; _ -> false };
    let missing = case values.get("absent") { Some(_) -> false; None -> true };
    literal and guarded and empty and missing
        and Option.with_default(values.get("literal"), "unused") == "match"
        and fallback(values.get("empty"), "unused") == ""
        and ("prefix:" + fallback(values.get("absent"), "missing")) == "prefix:missing".
"#;
    super::super::source_constructor_test::check_sources(&[
        caller,
        include_str!("../../../../../std/core/Option.terl"),
    ]);
}

#[test]
fn option_with_default_obeys_provider_body_under_http_imports() {
    let provider = r#"
module std.core.Option.
pub type None.
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
pub with_default(value: Option[String], default: String): String ->
    "source:" + default.
"#;
    let caller = r#"
module http_option_authority.
import std.http.Request.
import std.collections.Map.
import std.core.Option.
import std.core.Option.{with_default as fallback}.
pub check(): Bool ->
    let values = Map({"present", "must not be substituted"});
    Option.with_default(values.get("present"), "first") == "source:first"
        and fallback(values.get("absent"), "second") == "source:second".
"#;
    super::super::source_constructor_test::check_sources(&[caller, provider]);
}
