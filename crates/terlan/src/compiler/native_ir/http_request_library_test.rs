//! Request decoding is an ordinary source method, including its result policy.

use super::source_constructor_test::check_sources;

#[test]
fn body_json_uses_source_policy_and_ordinary_guarded_result_matching() {
    let provider = r#"
module std.http.Request.
import type std.core.Result.
import std.core.Result.{Ok, Err}.
pub struct Request { #payload: Int }.
pub from_value(value: Int): Request -> Request { #payload: value }.
pub (request: Request) body_text(): String -> "source accessor".
pub (request: Request) body_json(): Result[Int, String] ->
    if { request.#payload >= 0 -> Ok(request.#payload + 1); true -> Err("source error") }.
"#;
    let caller = r#"
module request_source_authority.
import std.http.Request.
import std.http.Request.{body_json as decode}.
import std.core.Result.{Ok, Err}.
pub check(): Bool ->
    let request = Request.from_value(41);
    let success = case request.body_json() {
        Ok(value) where value == 42 -> true;
        _ -> false
    };
    let failed = case decode(Request.from_value(-1)) {
        Err(message) where message == "source error" -> true;
        _ -> false
    };
    success and failed and request.body_text() == "source accessor"
        and Request.body_text(request) == "source accessor".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Request", "app.Request"),
        &provider.replace("std.http.Request", "app.Request"),
    ]);
}

#[test]
fn request_library_reads_its_own_fields_and_preserves_missing_and_empty_values() {
    let request = include_str!("../../../../../std/http/Request.terl")
        .replace("import type std.http.Cookies.Jar.", "import std.http.Cookies.\nimport type std.http.Cookies.Jar.\nimport std.core.Option.{Some}.");
    let request = format!(
        "{request}\n{}",
        r#"
pub exercise(): Bool ->
    let request = Request {
        #method: "POST", #path: "/source",
        #params: Map({"id", "42"}), #body: "raw body",
        #query_string: "q=&q=last", #query: Map({"q", ""}),
        #headers: Map({"x-test", "header"}, {"key", "ascii"}), #cookies: Map({"session", "cookie"}),
        #cookie_jar: Cookies.empty(), #body_file_path: "/tmp/body"
    };
    request.method() == "POST" and request.path() == "/source"
        and request.body_text() == "raw body"
        and request.query_string() == "q=&q=last"
        and request.body_file_path() == "/tmp/body"
        and request.cookies().marker == 41
        and (case request.param("id") { Some(value) -> value == "42"; None -> false })
        and (case request.param("missing") { None -> true; _ -> false })
        and (case request.query("q") { Some(value) -> value == ""; None -> false })
        and (case request.query("missing") { None -> true; _ -> false })
        and (case request.header("X-TEST") { Some(value) -> value == "header"; None -> false })
        and (case request.header("missing") { None -> true; _ -> false })
        and (case request.header("\u212aey") { None -> true; _ -> false })
        and (case request.cookie("session") { Some(value) -> value == "cookie"; None -> false })
        and (case request.cookie("missing") { None -> true; _ -> false }).
"#
    );
    check_sources(&[
        "module request_values. import std.http.Request. pub check(): Bool -> Request.exercise().",
        &request,
        "module std.http.Cookies. pub struct Jar { marker: Int }. pub empty(): Jar -> Jar { marker: 41 }.",
    ]);
}
