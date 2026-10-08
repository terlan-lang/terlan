//! Request ABI follows source types in immediate and suspended route handlers.

use super::*;
use crate::commands::serve::request_dispatch::handle_vm_stream_http1_request;

const SOURCE: &str = r#"module app.Api.
import std.collections.Map.
import std.core.{Int, Bool}.
import std.core.Option.{with_default}.
import std.http.{Request, Response}.
import std.vm.Process.

pub record(request: Request, id: Int, enabled: Bool): Response ->
    Response.text(request.method() + "|" + request.body_text() + "|" +
        with_default(request.param("id"), "missing") + "|" +
        with_default(request.query("q"), "missing") + "|" +
        with_default(request.header("X-Check"), "missing") + "|" +
        with_default(request.cookie("sid"), "missing") + "|" +
        Int.to_string(id) + "|" + Bool.to_string(enabled)).with_status(202).

pub suspended(request: Request, id: Int, enabled: Bool): Response ->
    Process.sleep(Process.timer(1)); record(request, id, enabled).

pub tuple(
    {Atom["request"], method, _path, params, body, _raw_query, query, headers, cookies, _file}:
        {Atom["request"], String, String, Map[String, String], String, String,
            Map[String, String], Map[String, String], Map[String, String], String},
    id: Int, enabled: Bool
): Response ->
    Process.sleep(Process.timer(1));
    Response.text(method + "|" + body + "|" + with_default(params.get("id"), "missing") + "|" +
        with_default(query.get("q"), "missing") + "|" +
        with_default(headers.get("x-check"), "missing") + "|" +
        with_default(cookies.get("sid"), "missing") + "|" +
        Int.to_string(id) + "|" + Bool.to_string(enabled)).with_status(203).

pub tuple_only(
    {Atom["request"], method, _path, params, body, _raw_query, _query, _headers, _cookies, _file}:
        {Atom["request"], String, String, Map[String, String], String, String,
            Map[String, String], Map[String, String], Map[String, String], String}
): Response ->
    Response.text(method + "|" + body + "|" + with_default(params.get("id"), "missing"))
        .with_status(201).
"#;

fn wire(route: &str) -> String {
    format!("POST {route}?q=first&q=last HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 4\r\nX-Check: header\r\nCookie: sid=first; sid=second\r\n\r\nbody")
}

#[test]
fn source_request_abi_is_independent_of_arity_and_suspension() {
    with_source_handler_project(
        SOURCE,
        &[
            ("/record/{id:Int}/{enabled:Bool}", "record", 3),
            ("/suspended/{id:Int}/{enabled:Bool}", "suspended", 3),
            ("/tuple/{id:Int}/{enabled:Bool}", "tuple", 3),
            ("/tuple-only", "tuple_only", 1),
            ("/tuple-only/{id:Int}", "tuple_only", 1),
        ],
        |web| {
            let request = wire("/record/42/true");
            let immediate = handle_vm_stream_http1_request(web, request.as_bytes()).unwrap();
            let immediate = String::from_utf8(immediate).unwrap();
            assert!(immediate.starts_with("HTTP/1.1 202 "), "{immediate}");
            assert!(
                immediate.ends_with("\r\n\r\nPOST|body|42|last|header|first|42|true"),
                "{immediate}"
            );
            let tuple =
                handle_vm_stream_http1_request(web, wire("/tuple-only/42").as_bytes()).unwrap();
            let tuple = String::from_utf8(tuple).unwrap();
            assert!(tuple.starts_with("HTTP/1.1 201 "), "{tuple}");
            assert!(tuple.ends_with("\r\n\r\nPOST|body|42"), "{tuple}");

            with_source_protocol_server(web.to_path_buf(), |send| {
                for (route, status, expected) in [
                    (
                        "/record/42/true",
                        202,
                        "POST|body|42|last|header|first|42|true",
                    ),
                    (
                        "/suspended/-7/false",
                        202,
                        "POST|body|-7|last|header|first|-7|false",
                    ),
                    (
                        "/tuple/42/true",
                        203,
                        "POST|body|42|last|header|first|42|true",
                    ),
                    ("/tuple-only", 201, "POST|body|missing"),
                    ("/tuple-only/42", 201, "POST|body|42"),
                    (
                        "/record/0/false",
                        202,
                        "POST|body|0|last|header|first|0|false",
                    ),
                ] {
                    let response = send(&wire(route));
                    assert!(
                        response.starts_with(&format!("HTTP/1.1 {status} ")),
                        "{route}: {response}"
                    );
                    assert!(
                        response.ends_with(&format!("\r\n\r\n{expected}")),
                        "{route}: {response}"
                    );
                }
                for route in [
                    "/record/no/true",
                    "/record/42/TRUE",
                    "/record/9223372036854775808/true",
                ] {
                    let response = send(&wire(route));
                    // Rejected captures fall through to the static server, which rejects POST.
                    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
                    assert!(
                        response.ends_with("\r\n\r\nmethod not allowed"),
                        "{response}"
                    );
                }
            });
        },
    );
}
