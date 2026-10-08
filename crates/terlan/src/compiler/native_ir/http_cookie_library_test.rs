//! Cookie functions follow source bodies, not names known to the compiler.

use super::source_constructor_test::check_sources;

#[test]
fn duplicate_cookie_precedence_is_executed_from_source_not_ingress() {
    for module in ["std.http.Cookies", "app.SourceCookies"] {
        for (condition, expected) in [("values.contains_key(name)", "first"), ("false", "last")] {
            let provider = include_str!("../../../../../std/http/Cookies.terl")
                .replace("module std.http.Cookies.", &format!("module {module}."))
                .replace("values.contains_key(name)", condition);
            check_sources(&[
                &format!(
                    r#"{provider}
pub check(): Bool ->
    let jar = from_pairs([{{"sid", "first"}}, {{"SID", "upper"}}, {{"empty", ""}}, {{"sid", "last"}}]);
    jar.get("sid") == Some("{expected}") and jar.get("SID") == Some("upper")
        and jar.get("empty") == Some("") and jar.get("missing") == None
        and jar.headers() == [] and from_pairs([]).get("sid") == None.
"#
                ),
                include_str!("../../../../../std/collections/Enumerable.terl"),
                include_str!("../../../../../std/collections/List.terl"),
                include_str!("../../../../../std/collections/Iterator.terl"),
            ]);
        }
    }
}

#[test]
fn cookie_jar_construction_executes_source_under_both_module_names() {
    let provider = include_str!("../../../../../std/http/Cookies.terl");
    for module in ["std.http.Cookies", "app.SourceCookies"] {
        // A changed constructor body must be observable, not replaced by ingress.
        for pending in ["[]", "[\"source-owned\"]"] {
            let provider = provider
                .replace("module std.http.Cookies.", &format!("module {module}."))
                .replace("#pending: []", &format!("#pending: {pending}"));
            check_sources(&[&format!(
                r#"{provider}
pub check(): Bool ->
    let incoming = Map({{"key", "before"}}, {{"empty", ""}});
    let jar = from_map(incoming);
    incoming.put("key", "after");
    jar.get("key") == Some("before") and jar.get("empty") == Some("")
        and jar.get("missing") == None and jar.headers() == {pending}
        and from_map(Map.new[String, String]()).get("key") == None.
"#
            )]);
        }
    }
}

#[test]
fn cookie_defaults_and_deletion_policy_execute_actual_source() {
    let provider = include_str!("../../../../../std/http/Cookies.terl")
        .replace("@compiler.native {std.http.cookies.encode}\n", "")
        .replace("    native.", r#"    if {
        domain != None or same_site != None -> "unexpected options";
        max_age == Some(0) and value == "" and not http_only and not secure -> name + ":" + path + ":" + (case expires { Some(text) -> text; None -> "missing" });
        max_age == None and expires == None -> name + ":" + value + ":" + path
            + (if { http_only -> ":private"; true -> ":public" })
            + (if { secure -> ":tls"; true -> ":plain" });
        true -> "unexpected policy"
    }."#);
    assert!(!provider.contains("@compiler.native"));
    for module in ["std.http.Cookies", "app.SourceCookies"] {
        for expiry in [
            "Thu, 01 Jan 1970 00:00:00 GMT",
            "Wed, 21 Oct 2015 07:28:00 GMT",
        ] {
            let provider = provider
                .replace("module std.http.Cookies.", &format!("module {module}."))
                .replace("Thu, 01 Jan 1970 00:00:00 GMT", expiry);
            check_sources(&[&format!(
                r#"{provider}
pub check(): Bool ->
    set_header("sid", "value") == "sid:value:/:public:plain"
        and set_header("sid", "value", "/private", true, true) == "sid:value:/private:private:tls"
        and delete_header("sid") == "sid:/:{expiry}"
        and delete_header("sid", "/private") == "sid:/private:{expiry}".
"#
            )]);
        }
    }
}

#[test]
fn cookie_jar_methods_execute_the_provider_not_a_compiler_name_table() {
    let provider = r#"
module std.http.Cookies.
pub struct Jar { text: String }.
pub start(): Jar -> Jar { text: "incoming" }.
pub (jar: Jar) get(name: String): String -> jar.text + ":" + name.
pub (mut jar: Jar) set(name: String, value: String = "default"): Unit ->
    Jar { text: jar.text + ":" + name + "=" + value }.
pub (mut jar: Jar) delete(name: String): Unit ->
    Jar { text: jar.text + ":removed=" + name }.
"#;
    let caller = r#"
module cookie_jar_source_authority.
import std.http.Cookies.
import std.core.Unit.
pub check(): Bool ->
    let jar = Cookies.start();
    let snapshot = jar;
    let done = jar.set("first");
    jar.delete("second");
    done == Unit and snapshot.get("old") == "incoming:old"
        and jar.get("end") == "incoming:first=default:removed=second:end".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Cookies", "app.Cookies"),
        &provider.replace("std.http.Cookies", "app.Cookies"),
    ]);
}

#[test]
fn response_cookie_composition_executes_provider_bodies() {
    let provider = r#"
module std.http.Response.
pub struct Response { label: String }.
pub start(): Response -> Response { label: "start" }.
pub (mut response: Response) cookie(name: String, value: String): Response ->
    Response { label: response.label + ":" + name + "=" + value }.
pub (mut response: Response) with_cookie(name: String, value: String): Response ->
    response |> cookie(name, value).
pub (mut response: Response) cookie_with_options(name: String, value: String): Response ->
    response |> cookie("options-" + name, value).
pub (mut response: Response) with_cookie_options(name: String, value: String): Response ->
    response |> cookie_with_options(name, value).
pub (mut response: Response) delete_cookie(name: String): Response ->
    response |> cookie("delete", name).
pub (mut response: Response) with_deleted_cookie(name: String): Response ->
    response |> delete_cookie(name).
"#;
    let caller = r#"
module cookie_composition_authority.
import std.http.Response.
import std.http.Response.{cookie as put, delete_cookie as remove}.
pub check(): Bool ->
    let first = Response.start().with_cookie("a", "b");
    let second = Response.cookie_with_options(first, "c", "d");
    let third = second.with_cookie_options("e", "f");
    let fourth = put(third, "g", "h");
    let fifth = remove(fourth, "i");
    let sixth = fifth.with_deleted_cookie("j");
    sixth.label == "start:a=b:options-c=d:options-e=f:g=h:delete=i:delete=j".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}

#[test]
fn generic_cookie_calls_and_namesakes_execute_declared_bodies() {
    check_sources(&[
        r#"module cookie_generics.
import std.http.Cookies.
import std.http.Cookies.{source_policy as alias}.
import app.CookiesPolicy.
pub check(): Bool ->
    Cookies.source_policy[String]("value") == "value"
    and alias[Int](42) == 42 and CookiesPolicy.source_policy[String]("value") == 19.
"#,
        "module std.http.Cookies. pub source_policy[T](value: T): T -> value.",
        "module app.CookiesPolicy. pub source_policy[T](value: T): Int -> 19.",
    ]);
}

#[test]
fn cookie_header_functions_are_source_owned() {
    let provider = r#"
module std.http.Cookies.
pub set_header(name: String, value: String): String -> name + ":" + value.
pub set_header_with_options(name: String): String -> "options:" + name.
pub delete_header(name: String): String -> "delete:" + name.
"#;
    let caller = r#"
module cookie_source_authority.
import std.http.Cookies.
import std.http.Cookies.{set_header as header}.
pub check(): Bool ->
    Cookies.set_header("x", "y") == "x:y"
        and header("a", "b") == "a:b"
        and Cookies.set_header_with_options("x") == "options:x"
        and Cookies.delete_header("x") == "delete:x".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Cookies", "app.Cookies"),
        &provider.replace("std.http.Cookies", "app.Cookies"),
    ]);
}
