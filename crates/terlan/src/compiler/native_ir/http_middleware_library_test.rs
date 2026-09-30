//! Middleware variants use ordinary source constructors and union layouts.

use super::super::source_constructor_test::check_sources;

#[test]
fn middleware_names_do_not_override_provider_variant_shapes() {
    let provider = r#"module std.http.Router.
pub type Continue = Atom["advance"].
pub type Respond = {Atom["halt"], code: Int, reason: String}.
pub type Retry = {Atom["retry"], delay: Int}.
pub type MiddlewareResult = Continue | Respond | Retry.
pub choose(code: Int): MiddlewareResult ->
    if { code == 0 -> Continue; code < 0 -> Retry(5); true -> Respond(code, "source") }.
"#;
    let caller = r#"module middleware_authority.
import std.http.Router.{Continue, Respond, Retry, MiddlewareResult, choose}.
inspect(result: MiddlewareResult): Int ->
    case result {
        Continue -> 1;
        Respond(code, reason) where reason == "source" -> code;
        Respond(_, _) -> -1;
        Retry(delay) -> delay
    }.
pub check(): Bool -> inspect(choose(0)) == 1 and inspect(choose(42)) == 42
    and inspect(choose(-1)) == 5 and inspect(Respond(42, "other")) == -1.
"#;
    for owner in ["std.http.Router", "app.Policy"] {
        let modules = check_sources(&[
            &caller.replace("std.http.Router", owner),
            &provider.replace("std.http.Router", owner),
        ]);
        for encoded in modules.iter().flat_map(|module| &module.managed_layouts) {
            let layout =
                crate::runtime::native_image::managed::decode_aggregate_layout(encoded).unwrap();
            assert_ne!(layout.canonical_type(), "Named(MiddlewareResult)");
            assert_ne!(layout.canonical_type(), "Named(Response)");
        }
    }
}

#[test]
fn canonical_middleware_variants_support_ordinary_pattern_matching() {
    check_sources(&[
        r#"module middleware_patterns.
import std.http.Router.{Continue, Respond, MiddlewareResult}.
import std.http.Response.
choose(blocked: Bool): MiddlewareResult ->
    if { blocked -> Respond(Response.text("denied", 403)); true -> Continue }.
inspect(result: MiddlewareResult): Bool ->
    case result { Continue -> true; Respond(_) -> false }.
pub check(): Bool -> inspect(choose(false)) and not inspect(choose(true)).
"#,
        include_str!("../../../../../std/http/Router.terl"),
        include_str!("../../../../../std/http/Response.terl"),
    ]);
}
