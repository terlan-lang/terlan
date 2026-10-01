//! Builder names and defaults belong to providers, not HTTP compiler tables.

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
