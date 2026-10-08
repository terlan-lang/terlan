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
    let request = include_str!("../../../../../std/http/Request.terl");
    let request = format!(
        "{request}\n{}",
        r#"
pub exercise(): Bool ->
    let request = Request {
        #method: "POST", #path: "/source",
        #params: [{"id", "42"}], #body: "raw body",
        #query_string: "q=&q=last", #query: [{"q", ""}],
        #headers: [{"x-test", "header"}, {"key", "ascii"}], #cookies: [{"session", "cookie"}],
        #body_file_path: "/tmp/body"
    };
    request.method() == "POST" and request.path() == "/source"
        and request.body_text() == "raw body"
        and request.query_string() == "q=&q=last"
        and request.body_file_path() == "/tmp/body"
        and request.cookies().headers() == []
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
        include_str!("../../../../../std/http/Cookies.terl"),
        include_str!("../../../../../std/collections/Enumerable.terl"),
        include_str!("../../../../../std/collections/List.terl"),
        include_str!("../../../../../std/collections/Iterator.terl"),
        include_str!("../../../../../std/core/Option.terl"),
    ]);
}

#[test]
fn metadata_duplicate_precedence_is_executed_from_source() {
    for (predicate, expected) in [("key == name", "Some(\"last\")"), ("false", "None")] {
        let request =
            include_str!("../../../../../std/http/Request.terl").replace("key == name", predicate);
        let provider = format!(
            r#"{request}
pub check(): Bool ->
    let pairs = [{{"key", "first"}}, {{"key", ""}}, {{"KEY", "other"}}, {{"key", "last"}}];
    let request = Request {{
        #method: "GET", #path: "/", #params: pairs, #body: "",
        #query_string: "", #query: pairs, #headers: pairs, #cookies: [], #body_file_path: ""
    }};
    request.param("key") == {expected}
        and request.query("key") == {expected}
        and request.header("KEY") == {expected}.
"#,
        );
        for source in [
            provider.clone(),
            provider.replace("module std.http.Request.", "module app.SourceRequest."),
        ] {
            check_sources(&[
                &source,
                include_str!("../../../../../std/collections/Enumerable.terl"),
                include_str!("../../../../../std/collections/List.terl"),
                include_str!("../../../../../std/collections/Iterator.terl"),
                include_str!("../../../../../std/core/Option.terl"),
            ]);
        }
    }
}

#[test]
fn native_lookup_scenarios_execute_in_source_with_distinct_metadata_policies() {
    let provider = format!(
        "{}\n{}",
        include_str!("../../../../../std/http/Request.terl"),
        r#"
pub check(): Bool ->
    let pairs = [
        {"sid", "first"}, {"empty", ""}, {"sid", "second"},
        {"SID", "upper"}, {"empty", "later"}, {"last_empty", "before"},
        {"last_empty", ""}, {"binary_to_atom", "text"}, {"list_to_atom", "text"}
    ];
    let request = Request {
        #method: "GET", #path: "/", #params: pairs, #body: "",
        #query_string: "", #query: pairs, #headers: pairs,
        #cookies: pairs, #body_file_path: ""
    };
    request.param("sid") == Some("second")
        and request.query("sid") == Some("second")
        and request.header("sId") == Some("second")
        and request.cookie("sid") == Some("first")
        and request.cookie("SID") == Some("upper")
        and request.cookie("empty") == Some("")
        and request.param("empty") == Some("later")
        and request.query("last_empty") == Some("")
        and request.header("LAST_EMPTY") == Some("")
        and request.cookie("absent") == None
        and request.param("absent") == None
        and request.query("absent") == None
        and request.header("absent") == None
        and request.param("binary_to_atom") == Some("text")
        and request.query("list_to_atom") == Some("text")
        and request.header("BINARY_TO_ATOM") == Some("text")
        and request.cookie("list_to_atom") == Some("text").
"#,
    );
    for source in [
        provider.clone(),
        provider.replace("module std.http.Request.", "module app.RequestSnapshot."),
    ] {
        check_sources(&[
            &source,
            include_str!("../../../../../std/http/Cookies.terl"),
            include_str!("../../../../../std/collections/Enumerable.terl"),
            include_str!("../../../../../std/collections/List.terl"),
            include_str!("../../../../../std/collections/Iterator.terl"),
            include_str!("../../../../../std/core/Option.terl"),
        ]);
    }
}
