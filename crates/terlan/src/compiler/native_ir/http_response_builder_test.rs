//! Builder names and defaults belong to providers, not HTTP compiler tables.

use super::*;
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CoreType};

pub(super) fn primitive(name: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::NativeOperation {
            operation: format!("std.http.response.{name}"),
            parameter_types: vec![],
        },
        args,
        return_type: CoreType::Named("Response".into()),
        effects: CoreEffectSet { effects: vec![] },
        span: crate::terlan_syntax::span::Span::new(0, 0),
    })
}

#[test]
fn builder_names_execute_source_bodies_and_source_defaults() {
    let provider = r#"module std.http.Response.
pub text(value: String, status: Int = 219): String -> value + ":source".
pub html(value: String): String -> value + ":html".
pub html(value: Int): String -> "integer-html".
pub json(value: String): String -> value + ":json".
pub json_text(value: String): String -> value + ":json_text".
pub file(value: String): String -> value + ":file".
pub stream(value: String): String -> value + ":stream".
pub redirect(value: String, status: Int = 307): Int -> status.
"#;
    let caller = r#"module builder_authority.
import std.http.Response.
import std.http.Response.{text as plain, redirect as moved}.
pub check(): Bool ->
    Response.text("body") == "body:source" and plain("alias") == "alias:source"
        and Response.html("body") == "body:html"
        and Response.html(7) == "integer-html"
        and Response.json("body") == "body:json"
        and Response.json_text("body") == "body:json_text"
        and Response.file("body") == "body:file"
        and Response.stream("body") == "body:stream"
        and Response.redirect("location") == 307 and moved("location", 308) == 308.
"#;
    super::super::source_constructor_test::check_sources(&[caller, provider]);
    super::super::source_constructor_test::check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}

#[test]
fn response_imports_and_callable_names_do_not_install_runtime_metadata() {
    for name in [
        "text",
        "html",
        "json",
        "json_text",
        "redirect",
        "file",
        "stream",
    ] {
        let mut core = http_core();
        for expression in [
            CoreExpr::Call {
                function: format!("std.http.Response.{name}"),
                type_args: vec![CoreType::String],
                args: vec![string("body")],
            },
            CoreExpr::RemoteCall {
                module: "std.http.Response".into(),
                function: name.into(),
                type_args: vec![CoreType::String],
                args: vec![string("body")],
            },
        ] {
            *body(&mut core) = expression.clone();
            lower_http_values(&mut core).unwrap();
            assert_eq!(body(&mut core), &expression);
            assert!(http_managed_layouts(&core).unwrap().is_empty());
        }
    }
}

#[test]
fn retired_native_builders_never_install_metadata_or_supply_defaults() {
    for name in ["text", "html", "json_text", "redirect", "file", "stream"] {
        for arity in 0..=6 {
            let mut core = http_core();
            core.imports.clear();
            let expression = primitive(name, vec![CoreExpr::Int(0); arity]);
            *body(&mut core) = expression.clone();
            lower_http_values(&mut core).unwrap();
            assert_eq!(body(&mut core), &expression);
            assert!(http_managed_layouts(&core).unwrap().is_empty());
        }
    }
}
