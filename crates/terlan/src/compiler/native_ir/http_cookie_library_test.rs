//! Cookie functions follow source bodies, not names known to the compiler.

use super::source_constructor_test::check_sources;

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
fn ordinary_cookie_calls_preserve_type_arguments_and_namespace_boundaries() {
    use crate::terlan_typeck::{CoreExpr, CoreType};
    let mut core = super::source_constructor_test::checked_provider(
        "module cookie_types. import std.http.Cookies. pub check(): Bool -> true.",
    );
    let remote = CoreExpr::RemoteCall {
        module: "std.http.Cookies".into(),
        function: "source_policy".into(),
        type_args: vec![CoreType::String],
        args: vec![CoreExpr::Binary("\"value\"".into())],
    };
    let qualified = CoreExpr::Call {
        function: "std.http.Cookies.source_policy".into(),
        type_args: vec![CoreType::String],
        args: vec![CoreExpr::Binary("\"value\"".into())],
    };
    let namesake = CoreExpr::Call {
        function: "std.http.CookiesPolicy.source_policy".into(),
        type_args: vec![CoreType::String],
        args: Vec::new(),
    };
    for (input, expected) in [
        (remote.clone(), remote.clone()),
        (qualified.clone(), qualified),
        (namesake.clone(), namesake),
    ] {
        core.functions[0].clauses[0].body.core_expr = Some(input);
        super::http_values::lower_http_values(&mut core).unwrap();
        assert_eq!(
            core.functions[0].clauses[0].body.core_expr.as_ref(),
            Some(&expected)
        );
    }
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
