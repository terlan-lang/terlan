//! Mutation names must not override ordinary provider methods or local receivers.

use super::check_sources;

#[test]
fn response_mutators_execute_provider_bodies_and_preserve_command_results() {
    let provider = r#"module std.http.Response.
pub struct Response { code: Int, label: String }.
pub start(): Response -> Response { code: 10, label: "source" }.
pub (mut response: Response) status(code: Int): Unit ->
    Response { code: response.code + code, label: response.label }.
pub (mut response: Response) header(name: String, value: String): Unit ->
    Response { code: response.code, label: response.label + ":" + name + ":" + value }.
pub (mut response: Response) set_cookie_header(value: String): Unit ->
    Response { code: response.code, label: response.label + ":cookie:" + value }.
"#;
    let caller = r#"module response_mutation_authority.
import std.http.Response.
import std.core.Unit.
pub struct Other { count: Int }.
pub (mut value: Other) status(code: Int): Unit -> Other { count: value.count + code }.
pub check(): Bool ->
    let response = Response.start();
    let snapshot = response;
    let changed = response.status(7);
    let appended = response.header("name", "value");
    let cookie = response.set_cookie_header("header");
    let other = Other { count: 2 };
    let other_done = other.status(3);
    changed == Unit and appended == Unit and cookie == Unit and other_done == Unit
        and response.code == 17 and response.label == "source:name:value:cookie:header"
        and snapshot.code == 10 and snapshot.label == "source" and other.count == 5.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}
